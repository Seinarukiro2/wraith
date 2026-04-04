use crate::cache::PypiCache;
use crate::known_packages;
use anyhow::Result;
use codeguard_core::Config;
use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Clone)]
pub enum PackageStatus {
    Safe,
    Stdlib,
    NotFound,
    NotInstalled,
    Suspicious(Vec<String>),
    Unknown(String),
}

pub struct PackageChecker {
    cache: Mutex<PypiCache>,
    http_client: Option<reqwest::blocking::Client>,
    import_map: HashMap<&'static str, &'static str>,
    popular: Vec<String>,
    offline: bool,
    python_exec: String,
    results: Mutex<HashMap<String, PackageStatus>>,
}

impl PackageChecker {
    pub fn new(config: &Config) -> Result<Self> {
        let cache_path = config.cache_dir().join("pypi.db");
        let cache = PypiCache::open(&cache_path, config.pypi_cache_ttl())?;

        let http_client = if config.offline {
            None
        } else {
            Some(
                reqwest::blocking::Client::builder()
                    .timeout(std::time::Duration::from_secs(10))
                    .user_agent("wraith/0.1.0")
                    .build()?,
            )
        };

        Ok(Self {
            cache: Mutex::new(cache),
            http_client,
            import_map: known_packages::import_to_package_map(),
            popular: known_packages::popular_packages()
                .iter()
                .map(|s| s.to_string())
                .collect(),
            offline: config.offline,
            python_exec: config.python_exec().to_string(),
            results: Mutex::new(HashMap::new()),
        })
    }

    pub fn resolve_package_name(&self, import_name: &str) -> String {
        self.import_map
            .get(import_name)
            .map(|s| s.to_string())
            .unwrap_or_else(|| import_name.to_string())
    }

    pub fn prefetch(&self, packages: &[String]) {
        for pkg in packages {
            let pkg_name = self.resolve_package_name(pkg);
            let status = self.check_single(pkg, &pkg_name);
            self.results.lock().unwrap().insert(pkg.clone(), status);
        }
    }

    pub fn check(&self, import_name: &str) -> PackageStatus {
        if let Some(status) = self.results.lock().unwrap().get(import_name) {
            return status.clone();
        }
        let pkg_name = self.resolve_package_name(import_name);
        let status = self.check_single(import_name, &pkg_name);
        self.results
            .lock()
            .unwrap()
            .insert(import_name.to_string(), status.clone());
        status
    }

    fn check_single(&self, import_name: &str, package_name: &str) -> PackageStatus {
        // 1. Check if installed locally
        let installed = self.is_installed(import_name);

        // 2. Check PyPI
        let pypi_info = self.check_pypi(package_name);

        match (installed, pypi_info) {
            (true, Some(info)) | (false, Some(info)) => {
                let suspicious = self.check_suspicious(package_name, &info);
                if !suspicious.is_empty() {
                    PackageStatus::Suspicious(suspicious)
                } else if installed {
                    PackageStatus::Safe
                } else {
                    PackageStatus::NotInstalled
                }
            }
            (true, None) => PackageStatus::Safe,
            (false, None) if self.offline => {
                PackageStatus::Unknown("offline mode, no cache".to_string())
            }
            (false, None) => PackageStatus::NotFound,
        }
    }

    fn is_installed(&self, import_name: &str) -> bool {
        let output = std::process::Command::new(&self.python_exec)
            .args(["-c", &format!("import {import_name}")])
            .output();
        matches!(output, Ok(o) if o.status.success())
    }

    fn check_pypi(&self, package_name: &str) -> Option<PypiInfo> {
        // Try cache first
        {
            let cache = self.cache.lock().unwrap();
            if let Some(entry) = cache.get(package_name) {
                if entry.status == 404 {
                    return None;
                }
                if let Some(ref resp) = entry.response {
                    if let Ok(info) = parse_pypi_response(resp) {
                        return Some(info);
                    }
                }
            }
        }

        // Fetch from PyPI if online
        let client = self.http_client.as_ref()?;
        let url = format!("https://pypi.org/pypi/{package_name}/json");
        let resp = client.get(&url).send().ok()?;
        let status = resp.status().as_u16();

        if status == 404 {
            let _ = self.cache.lock().unwrap().put(package_name, 404, None);
            return None;
        }

        if status == 200 {
            let body = resp.text().ok()?;
            let _ = self
                .cache
                .lock()
                .unwrap()
                .put(package_name, 200, Some(&body));
            return parse_pypi_response(&body).ok();
        }

        None
    }

    fn check_suspicious(&self, package_name: &str, info: &PypiInfo) -> Vec<String> {
        let mut reasons = Vec::new();

        // Age check (< 30 days)
        if let Some(ref first_release) = info.first_release {
            if let Ok(date) = chrono::NaiveDate::parse_from_str(first_release, "%Y-%m-%d") {
                let today = chrono::Utc::now().date_naive();
                let age = today.signed_duration_since(date);
                if age.num_days() < 30 {
                    reasons.push(format!("package created {} days ago", age.num_days()));
                }
            }
        }

        // Download check (< 1000 weekly)
        if let Some(downloads) = info.weekly_downloads {
            if downloads < 1000 {
                reasons.push(format!("low weekly downloads ({downloads})"));
            }
        }

        // Version count as additional signal (single version = higher risk)
        if info.version_count == 1 {
            // Only flag if combined with another reason (single version alone is normal for new legit packages)
            if !reasons.is_empty() {
                reasons.push("only 1 release version".to_string());
            }
        }

        // Temporal check: package created after major LLM training cutoffs
        // Packages registered only after training cutoff are higher-risk slopsquatting targets
        if let Some(ref first_release) = info.first_release {
            if let Ok(date) = chrono::NaiveDate::parse_from_str(first_release, "%Y-%m-%d") {
                // Training cutoffs for major models
                let cutoffs = [
                    ("2024-04-01", "GPT-4o/Claude 3"),
                    ("2025-04-01", "GPT-4.5/Claude 3.5"),
                ];
                for (cutoff_str, models) in &cutoffs {
                    if let Ok(cutoff) = chrono::NaiveDate::parse_from_str(cutoff_str, "%Y-%m-%d") {
                        if date > cutoff {
                            reasons.push(format!(
                                "package first released after {} training cutoff ({first_release})",
                                models
                            ));
                            break;
                        }
                    }
                }
            }
        }

        // Typosquat check
        let closest = self.find_closest_popular(package_name);
        if let Some((name, dist)) = closest {
            if dist > 0 && dist <= 2 {
                reasons.push(format!(
                    "name similar to popular package '{name}' (edit distance {dist})"
                ));
            }
        }

        reasons
    }

    fn find_closest_popular(&self, name: &str) -> Option<(String, usize)> {
        let name_lower = name.to_lowercase();
        let mut best: Option<(String, usize)> = None;
        for popular in &self.popular {
            let pop_lower = popular.to_lowercase();
            if pop_lower == name_lower {
                return None; // Exact match, not a typosquat
            }
            let dist = strsim::levenshtein(&name_lower, &pop_lower);
            if dist <= 2 {
                match &best {
                    None => best = Some((popular.clone(), dist)),
                    Some((_, bd)) if dist < *bd => best = Some((popular.clone(), dist)),
                    _ => {}
                }
            }
        }
        best
    }

    pub fn suggest_similar(&self, name: &str) -> Option<String> {
        self.find_closest_popular(name).map(|(n, _)| n)
    }
}

#[derive(Debug)]
struct PypiInfo {
    first_release: Option<String>,
    weekly_downloads: Option<u64>,
    version_count: usize,
}

fn parse_pypi_response(body: &str) -> Result<PypiInfo, serde_json::Error> {
    let v: serde_json::Value = serde_json::from_str(body)?;

    // Get first release date from releases
    let releases = v.get("releases").and_then(|r| r.as_object());
    let first_release = releases.and_then(|rels| {
        rels.values()
            .filter_map(|files| {
                files.as_array().and_then(|arr| {
                    arr.first().and_then(|f| {
                        f.get("upload_time")
                            .and_then(|t| t.as_str().map(|s| s[..10].to_string()))
                    })
                })
            })
            .min()
    });

    let version_count = releases.map(|r| r.len()).unwrap_or(0);

    // PyPI JSON API doesn't expose weekly downloads directly in the main endpoint.
    // We use info.downloads (deprecated but sometimes present) or version count as proxy.
    let weekly_downloads = v
        .get("info")
        .and_then(|i| i.get("downloads"))
        .and_then(|d| d.get("last_week"))
        .and_then(|w| w.as_u64());

    Ok(PypiInfo {
        first_release,
        weekly_downloads,
        version_count,
    })
}
