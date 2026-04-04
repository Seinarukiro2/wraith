use crate::config::Config;
use crate::diagnostic::{Diagnostic, Severity, Span, TextEdit};
use crate::reporter::{format_diagnostics, OutputFormat};
use crate::rules::{all_rules, RuleCode};
use std::path::PathBuf;

#[test]
fn test_rule_code_display() {
    let code = RuleCode::new("AG001");
    assert_eq!(format!("{code}"), "AG001");
}

#[test]
fn test_rule_code_prefix() {
    let code = RuleCode::new("AG001");
    assert_eq!(code.prefix(), "AG");
    let code2 = RuleCode::new("PH003");
    assert_eq!(code2.prefix(), "PH");
}

#[test]
fn test_rule_code_matches_selector() {
    let code = RuleCode::new("AG001");
    assert!(code.matches_selector("AG"));
    assert!(code.matches_selector("AG001"));
    assert!(code.matches_selector("ag")); // case-insensitive
    assert!(!code.matches_selector("PH"));
    assert!(!code.matches_selector("VC"));
}

#[test]
fn test_diagnostic_builders() {
    let span = Span::new(PathBuf::from("test.py"), 1, 0, 1, 10);
    let d = Diagnostic::error(RuleCode::new("AG001"), span.clone(), "test message");
    assert_eq!(d.severity, Severity::Error);
    assert_eq!(d.code.0, "AG001");
    assert_eq!(d.message, "test message");
    assert!(d.suggestion.is_none());
    assert!(d.fix.is_none());

    let d2 = Diagnostic::warning(RuleCode::new("VC001"), span.clone(), "warning msg")
        .with_suggestion("fix this")
        .with_fix(TextEdit {
            start_line: 1,
            start_col: 0,
            end_line: 1,
            end_col: 10,
            replacement: "fixed".to_string(),
        });
    assert_eq!(d2.severity, Severity::Warning);
    assert_eq!(d2.suggestion.as_deref(), Some("fix this"));
    assert!(d2.fix.is_some());
}

#[test]
fn test_config_rule_enabled() {
    let mut config = Config::default();
    assert!(config.is_rule_enabled("AG001")); // all enabled by default
    assert!(config.is_rule_enabled("VC003"));

    config.select = Some(vec!["AG".to_string()]);
    assert!(config.is_rule_enabled("AG001"));
    assert!(config.is_rule_enabled("AG002"));
    assert!(!config.is_rule_enabled("VC001"));
    assert!(!config.is_rule_enabled("PH001"));

    config.select = Some(vec!["AG".to_string(), "VC".to_string()]);
    assert!(config.is_rule_enabled("AG001"));
    assert!(config.is_rule_enabled("VC003"));
    assert!(!config.is_rule_enabled("PH001"));
}

#[test]
fn test_config_defaults() {
    let config = Config::default();
    assert!(!config.strict);
    assert!(!config.offline);
    assert!(!config.fix);
    assert_eq!(config.pypi_cache_ttl(), 86400);
    assert_eq!(config.python_exec(), "python3");
}

#[test]
fn test_all_rules_defined() {
    let rules = all_rules();
    assert_eq!(rules.len(), 12); // 3 AG + 3 PH + 6 VC

    let codes: Vec<&str> = rules.iter().map(|r| r.code.0.as_str()).collect();
    assert!(codes.contains(&"AG001"));
    assert!(codes.contains(&"AG002"));
    assert!(codes.contains(&"AG003"));
    assert!(codes.contains(&"PH001"));
    assert!(codes.contains(&"PH002"));
    assert!(codes.contains(&"PH003"));
    assert!(codes.contains(&"VC001"));
    assert!(codes.contains(&"VC002"));
    assert!(codes.contains(&"VC003"));
    assert!(codes.contains(&"VC004"));
    assert!(codes.contains(&"VC005"));
    assert!(codes.contains(&"VC006"));
}

#[test]
fn test_text_format_output() {
    let span = Span::new(PathBuf::from("test.py"), 42, 5, 42, 20);
    let d = Diagnostic::error(RuleCode::new("AG001"), span, "test error")
        .with_suggestion("did you mean 'foo'?");
    let output = format_diagnostics(&[d], OutputFormat::Text);
    assert!(output.contains("test.py:42:5"));
    assert!(output.contains("AG001"));
    assert!(output.contains("test error"));
    assert!(output.contains("did you mean 'foo'?"));
    assert!(output.contains("Found 1 issue"));
}

#[test]
fn test_json_format_output() {
    let span = Span::new(PathBuf::from("test.py"), 1, 0, 1, 10);
    let d = Diagnostic::warning(RuleCode::new("VC001"), span, "secret found");
    let output = format_diagnostics(&[d], OutputFormat::Json);
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert!(parsed.is_array());
    assert_eq!(parsed[0]["code"], "VC001");
    assert_eq!(parsed[0]["severity"], "warning");
    assert_eq!(parsed[0]["message"], "secret found");
}

#[test]
fn test_severity_display() {
    assert_eq!(format!("{}", Severity::Error), "error");
    assert_eq!(format!("{}", Severity::Warning), "warning");
    assert_eq!(format!("{}", Severity::Info), "info");
}
