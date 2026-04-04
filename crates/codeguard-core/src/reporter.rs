use crate::Diagnostic;
use colored::Colorize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
    Sarif,
}

impl std::str::FromStr for OutputFormat {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "text" => Ok(OutputFormat::Text),
            "json" => Ok(OutputFormat::Json),
            "sarif" => Ok(OutputFormat::Sarif),
            _ => Err(format!("unknown format: {s}")),
        }
    }
}

pub fn format_diagnostics(diagnostics: &[Diagnostic], format: OutputFormat) -> String {
    match format {
        OutputFormat::Text => format_text(diagnostics),
        OutputFormat::Json => format_json(diagnostics),
        OutputFormat::Sarif => format_sarif(diagnostics),
    }
}

fn format_text(diagnostics: &[Diagnostic]) -> String {
    let mut out = String::new();
    let mut fixable_count = 0;

    for d in diagnostics {
        let location = format!(
            "{}:{}:{}",
            d.span.file.display(),
            d.span.start_line,
            d.span.start_col
        );
        let padding_len = location.len();

        let code_str = format!("{}", d.code);
        let severity_colored = match d.severity {
            crate::Severity::Error => code_str.red().bold().to_string(),
            crate::Severity::Warning => code_str.yellow().bold().to_string(),
            crate::Severity::Info => code_str.blue().to_string(),
        };

        out.push_str(&format!(
            "{} {} {}\n",
            location.dimmed(),
            severity_colored,
            d.message,
        ));

        if let Some(ref suggestion) = d.suggestion {
            let fixable_marker = if d.fix.is_some() {
                " (auto-fixable)"
            } else {
                ""
            };
            out.push_str(&format!(
                "  {} {}{}\n",
                "\u{2192}".cyan(),
                suggestion,
                fixable_marker.green(),
            ));
        }

        if d.fix.is_some() {
            fixable_count += 1;
        }
    }

    if !diagnostics.is_empty() {
        let count = diagnostics.len();
        if fixable_count > 0 {
            out.push_str(&format!(
                "\nFound {} issue{} ({} auto-fixable). Run with {} to apply.\n",
                count,
                if count == 1 { "" } else { "s" },
                fixable_count,
                "--fix".bold(),
            ));
        } else {
            out.push_str(&format!(
                "\nFound {} issue{}.\n",
                count,
                if count == 1 { "" } else { "s" },
            ));
        }
    }

    out
}

fn format_json(diagnostics: &[Diagnostic]) -> String {
    serde_json::to_string_pretty(diagnostics).unwrap_or_default()
}

fn format_sarif(diagnostics: &[Diagnostic]) -> String {
    let results: Vec<serde_json::Value> = diagnostics
        .iter()
        .map(|d| {
            let level = match d.severity {
                crate::Severity::Error => "error",
                crate::Severity::Warning => "warning",
                crate::Severity::Info => "note",
            };

            let mut result = serde_json::json!({
                "ruleId": d.code.0,
                "level": level,
                "message": {
                    "text": d.message
                },
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": {
                            "uri": d.span.file.display().to_string()
                        },
                        "region": {
                            "startLine": d.span.start_line,
                            "startColumn": d.span.start_col + 1,
                            "endLine": d.span.end_line,
                            "endColumn": d.span.end_col + 1
                        }
                    }
                }]
            });

            if let Some(ref suggestion) = d.suggestion {
                result["message"]["text"] =
                    serde_json::Value::String(format!("{} — {}", d.message, suggestion));
            }

            if let Some(ref fix) = d.fix {
                result["fixes"] = serde_json::json!([{
                    "description": {
                        "text": d.suggestion.as_deref().unwrap_or("auto-fix")
                    },
                    "artifactChanges": [{
                        "artifactLocation": {
                            "uri": d.span.file.display().to_string()
                        },
                        "replacements": [{
                            "deletedRegion": {
                                "startLine": fix.start_line,
                                "startColumn": fix.start_col + 1,
                                "endLine": fix.end_line,
                                "endColumn": fix.end_col + 1
                            },
                            "insertedContent": {
                                "text": fix.replacement
                            }
                        }]
                    }]
                }]);
            }

            result
        })
        .collect();

    let rules: Vec<serde_json::Value> = {
        let mut seen = std::collections::HashSet::new();
        diagnostics
            .iter()
            .filter(|d| seen.insert(d.code.0.clone()))
            .map(|d| {
                serde_json::json!({
                    "id": d.code.0,
                    "shortDescription": {
                        "text": d.code.0
                    }
                })
            })
            .collect()
    };

    let sarif = serde_json::json!({
        "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/main/sarif-2.1/schema/sarif-schema-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "wraith",
                    "version": env!("CARGO_PKG_VERSION"),
                    "informationUri": "https://github.com/Seinarukiro2/codeguard",
                    "rules": rules
                }
            },
            "results": results
        }]
    });

    serde_json::to_string_pretty(&sarif).unwrap_or_default()
}
