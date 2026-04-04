use codeguard_core::{Diagnostic, RuleCode, Span};
use once_cell::sync::Lazy;
use regex::Regex;
use std::path::{Path, PathBuf};

static UNPINNED_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^([a-zA-Z0-9_-]+)\s*(>=|>|~=|\*|!=)").unwrap());

static PINNED_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^([a-zA-Z0-9_-]+)\s*==\s*\d").unwrap());

/// Dangerous file patterns that should not be in a project tree
const DANGEROUS_PATTERNS: &[(&str, &str)] = &[
    (".env", "environment file with secrets"),
    (".env.local", "local environment file with secrets"),
    (".env.production", "production environment file"),
    (".env.staging", "staging environment file"),
    ("id_rsa", "SSH private key"),
    ("id_ed25519", "SSH private key"),
    ("id_ecdsa", "SSH private key"),
    ("id_dsa", "SSH private key"),
    (".pem", "PEM certificate/key file"),
    (".key", "private key file"),
    (".p12", "PKCS#12 certificate"),
    (".pfx", "PKCS#12 certificate"),
    (".jks", "Java keystore"),
    ("credentials.json", "credentials file"),
    ("service-account.json", "service account credentials"),
    ("gcloud-service-key.json", "GCP service account key"),
    (".htpasswd", "HTTP password file"),
    ("shadow", "system shadow file"),
    (".npmrc", "npm config (may contain auth tokens)"),
    (".pypirc", "PyPI config (may contain auth tokens)"),
    (
        "docker-compose.override.yml",
        "docker override (may contain secrets)",
    ),
];

const DANGEROUS_EXTENSIONS: &[(&str, &str)] = &[
    ("pem", "PEM certificate/key"),
    ("key", "private key"),
    ("p12", "PKCS#12 certificate"),
    ("pfx", "PKCS#12 certificate"),
    ("jks", "Java keystore"),
    ("keystore", "keystore file"),
];

/// Run project-level checks on the directory tree
pub fn check_project(root: &Path) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    check_dangerous_files(root, root, &mut diagnostics);
    check_requirements_pinning(root, &mut diagnostics);
    check_missing_lockfile(root, &mut diagnostics);
    check_source_maps(root, root, &mut diagnostics);

    diagnostics
}

fn check_dangerous_files(root: &Path, dir: &Path, diags: &mut Vec<Diagnostic>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip hidden dirs (except .env files), node_modules, .git, target, venv
        if path.is_dir() {
            if name_str.starts_with('.')
                || name_str == "node_modules"
                || name_str == "__pycache__"
                || name_str == ".git"
                || name_str == "target"
                || name_str == "venv"
                || name_str == ".venv"
            {
                continue;
            }
            check_dangerous_files(root, &path, diags);
            continue;
        }

        // Check filename matches
        for &(pattern, description) in DANGEROUS_PATTERNS {
            if name_str == pattern || name_str.ends_with(pattern) {
                let rel = path.strip_prefix(root).unwrap_or(&path);
                diags.push(
                    Diagnostic::warning(
                        RuleCode::new("VC007"),
                        Span::new(rel.to_path_buf(), 1, 0, 1, 0),
                        format!("dangerous file in project: {} ({})", name_str, description),
                    )
                    .with_suggestion("add to .gitignore or remove from repository"),
                );
                break;
            }
        }

        // Check extension
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            for &(dangerous_ext, description) in DANGEROUS_EXTENSIONS {
                if ext == dangerous_ext {
                    // Don't double-report if already caught by filename
                    let already_reported = diags.iter().any(|d| {
                        d.code.0 == "VC007"
                            && d.span.file == path.strip_prefix(root).unwrap_or(&path)
                    });
                    if !already_reported {
                        let rel = path.strip_prefix(root).unwrap_or(&path);
                        diags.push(
                            Diagnostic::warning(
                                RuleCode::new("VC007"),
                                Span::new(rel.to_path_buf(), 1, 0, 1, 0),
                                format!(
                                    "dangerous file in project: {} ({})",
                                    name_str, description
                                ),
                            )
                            .with_suggestion("add to .gitignore or remove from repository"),
                        );
                    }
                    break;
                }
            }
        }
    }
}

fn check_requirements_pinning(root: &Path, diags: &mut Vec<Diagnostic>) {
    let req_path = root.join("requirements.txt");
    if !req_path.exists() {
        return;
    }

    let content = match std::fs::read_to_string(&req_path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let rel = PathBuf::from("requirements.txt");
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('-') {
            continue;
        }

        // Skip lines with == (properly pinned)
        if PINNED_RE.is_match(trimmed) {
            continue;
        }

        // Flag unpinned: >=, >, ~=, *, no version at all
        let has_version_spec = trimmed.contains("==")
            || trimmed.contains(">=")
            || trimmed.contains("<=")
            || trimmed.contains("~=")
            || trimmed.contains("!=")
            || trimmed.contains(">")
            || trimmed.contains("<");

        if !has_version_spec {
            // No version at all: `requests`
            let pkg = trimmed.split('[').next().unwrap_or(trimmed).trim();
            diags.push(
                Diagnostic::warning(
                    RuleCode::new("VC008"),
                    Span::new(
                        rel.clone(),
                        i as u32 + 1,
                        0,
                        i as u32 + 1,
                        trimmed.len() as u32,
                    ),
                    format!("unpinned dependency: '{pkg}' has no version constraint"),
                )
                .with_suggestion(format!("pin version: {pkg}==<version>")),
            );
        } else if UNPINNED_RE.is_match(trimmed) {
            // Has version but not pinned: `requests>=2.0`
            let pkg = trimmed
                .split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_')
                .next()
                .unwrap_or(trimmed);
            diags.push(
                Diagnostic::info(
                    RuleCode::new("VC008"),
                    Span::new(
                        rel.clone(),
                        i as u32 + 1,
                        0,
                        i as u32 + 1,
                        trimmed.len() as u32,
                    ),
                    format!("loosely pinned dependency: '{trimmed}'"),
                )
                .with_suggestion(format!("consider pinning: {pkg}==<exact_version>")),
            );
        }
    }
}

fn check_missing_lockfile(root: &Path, diags: &mut Vec<Diagnostic>) {
    let has_requirements = root.join("requirements.txt").exists();
    let has_pyproject = root.join("pyproject.toml").exists();
    let has_pipfile = root.join("Pipfile").exists();

    if !has_requirements && !has_pyproject && !has_pipfile {
        return;
    }

    let has_lock = root.join("poetry.lock").exists()
        || root.join("Pipfile.lock").exists()
        || root.join("pdm.lock").exists()
        || root.join("uv.lock").exists()
        || root.join("requirements.lock").exists();

    if !has_lock {
        let manifest = if has_pipfile {
            "Pipfile"
        } else if has_pyproject {
            "pyproject.toml"
        } else {
            "requirements.txt"
        };
        diags.push(
            Diagnostic::warning(
                RuleCode::new("VC009"),
                Span::new(PathBuf::from(manifest), 1, 0, 1, 0),
                format!(
                    "no lockfile found (has {manifest} but no poetry.lock/Pipfile.lock/uv.lock)"
                ),
            )
            .with_suggestion("generate a lockfile to ensure reproducible builds"),
        );
    }
}

fn check_source_maps(root: &Path, dir: &Path, diags: &mut Vec<Diagnostic>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if path.is_dir() {
            // Only scan build/dist directories for source maps
            if name_str == "dist"
                || name_str == "build"
                || name_str == "static"
                || name_str == "public"
                || name_str == "assets"
            {
                check_source_maps(root, &path, diags);
            }
            continue;
        }

        if path.extension().is_some_and(|e| e == "map") {
            // Parse .map file as JSON, check for sourcesContent
            let rel = path.strip_prefix(root).unwrap_or(&path);
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(sources_content) = json.get("sourcesContent") {
                        if let Some(arr) = sources_content.as_array() {
                            if !arr.is_empty() {
                                let total_size: usize =
                                    arr.iter().filter_map(|v| v.as_str()).map(|s| s.len()).sum();
                                diags.push(
                                    Diagnostic::error(
                                        RuleCode::new("VC010"),
                                        Span::new(rel.to_path_buf(), 1, 0, 1, 0),
                                        format!(
                                            "source map with full source code: {} ({} sources, ~{} bytes of source)",
                                            name_str,
                                            arr.len(),
                                            total_size,
                                        ),
                                    )
                                    .with_suggestion("remove sourcesContent from .map file or exclude .map files from distribution"),
                                );
                                continue;
                            }
                        }
                    }
                    // .map without sourcesContent — minor info leak
                    diags.push(
                        Diagnostic::info(
                            RuleCode::new("VC010"),
                            Span::new(rel.to_path_buf(), 1, 0, 1, 0),
                            format!("source map file: {} (positional mappings only)", name_str),
                        )
                        .with_suggestion("consider excluding .map files from distribution"),
                    );
                }
            }
        }
    }
}
