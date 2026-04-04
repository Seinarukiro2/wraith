use codeguard_ast::{extract_file_info, CallInfo, FileInfo};
use codeguard_core::{Diagnostic, RuleCode};
use std::collections::HashSet;
use std::path::Path;
use tree_sitter::Tree;

/// VC011: Secret leaked to unsafe sink (print, log, etc.)
/// Intraprocedural taint analysis — tracks secret variables within a file.
pub fn check_taint(tree: &Tree, source: &str, path: &Path) -> Vec<Diagnostic> {
    let info = extract_file_info(tree, source, path);
    let mut diagnostics = Vec::new();

    // Phase 1: identify tainted names (sources)
    let tainted = collect_tainted_names(&info);

    if tainted.is_empty() {
        return diagnostics;
    }

    // Phase 2: check if tainted names flow to sinks
    for call in &info.calls {
        if is_leak_sink(call) {
            // Check if any argument to this sink contains a tainted name
            // We check the call's source text for tainted variable references
            let call_text = &source[byte_range(call.span.start_line, call.span.start_col,
                                                call.span.end_line, call.span.end_col, source)];
            for name in &tainted {
                // Check if the tainted name appears in the call arguments
                // Simple heuristic: name appears as a word boundary in the call text
                if contains_name(call_text, name) {
                    diagnostics.push(
                        Diagnostic::warning(
                            RuleCode::new("VC011"),
                            call.span.clone(),
                            format!(
                                "potential secret leak: '{}' (from sensitive source) passed to {}()",
                                name, call.full_name
                            ),
                        )
                        .with_suggestion(format!("avoid logging or printing secret variable '{name}'"))
                        .with_confidence(0.6),
                    );
                    break; // one finding per call
                }
            }
        }
    }

    diagnostics
}

/// Collect names of variables that hold secret/sensitive data.
fn collect_tainted_names(info: &FileInfo) -> HashSet<String> {
    let mut tainted = HashSet::new();

    for assign in &info.assignments {
        // Source 1: os.environ["X"], os.environ.get("X"), os.getenv("X")
        if let Some(ref val) = assign.value {
            if val.contains("os.environ") || val.contains("os.getenv") || val.contains("getenv(") {
                tainted.insert(assign.target.clone());
                continue;
            }
        }

        // Source 2: variable name matches secret pattern
        let target_lower = assign.target.to_lowercase();
        if is_secret_name(&target_lower) {
            tainted.insert(assign.target.clone());
        }
    }

    // Source 3: function parameters with secret-like names
    // (would need symbol table integration for full support)

    tainted
}

fn is_secret_name(name: &str) -> bool {
    name.contains("secret")
        || name.contains("password")
        || name.contains("passwd")
        || name.contains("api_key")
        || name.contains("apikey")
        || name.contains("auth_token")
        || name.contains("access_token")
        || name.contains("private_key")
        || name.contains("token") && !name.contains("csrf") && !name.contains("preview")
        || name.contains("bot_token")
}

fn is_leak_sink(call: &CallInfo) -> bool {
    let name = &call.full_name;
    // print() family
    if name == "print" || name == "pprint" {
        return true;
    }
    // logging
    if let Some(ref recv) = call.receiver {
        let r = recv.as_str();
        if r == "logging" || r == "logger" || r == "log" {
            let func = call.function.as_str();
            return matches!(func, "info" | "debug" | "warning" | "error" | "critical" | "exception" | "log");
        }
    }
    false
}

fn contains_name(text: &str, name: &str) -> bool {
    // Check if `name` appears as a whole word in text
    // Simple: find name and check boundaries
    let mut start = 0;
    while let Some(pos) = text[start..].find(name) {
        let abs_pos = start + pos;
        let before_ok = abs_pos == 0
            || !text.as_bytes()[abs_pos - 1].is_ascii_alphanumeric()
                && text.as_bytes()[abs_pos - 1] != b'_';
        let after_pos = abs_pos + name.len();
        let after_ok = after_pos >= text.len()
            || !text.as_bytes()[after_pos].is_ascii_alphanumeric()
                && text.as_bytes()[after_pos] != b'_';
        if before_ok && after_ok {
            return true;
        }
        start = abs_pos + 1;
    }
    false
}

fn byte_range(start_line: u32, start_col: u32, end_line: u32, end_col: u32, source: &str) -> std::ops::Range<usize> {
    let mut line = 1u32;
    let mut start_byte = 0;
    let mut end_byte = source.len();

    for (i, ch) in source.char_indices() {
        if line == start_line && (i - line_start(source, line)) as u32 == start_col {
            start_byte = i;
        }
        if line == end_line && (i - line_start(source, line)) as u32 == end_col {
            end_byte = i;
            break;
        }
        if ch == '\n' {
            line += 1;
        }
    }
    start_byte..end_byte.min(source.len())
}

fn line_start(source: &str, target_line: u32) -> usize {
    let mut line = 1u32;
    for (i, ch) in source.char_indices() {
        if line == target_line {
            return i;
        }
        if ch == '\n' {
            line += 1;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use codeguard_ast::parse_python;
    use std::path::PathBuf;

    fn check(source: &str) -> Vec<Diagnostic> {
        let tree = parse_python(source).unwrap();
        check_taint(&tree, source, &PathBuf::from("test.py"))
    }

    #[test]
    fn test_print_secret_from_env() {
        let d = check(r#"
import os
api_key = os.environ["API_KEY"]
print(api_key)
"#);
        let vc011: Vec<_> = d.iter().filter(|d| d.code.0 == "VC011").collect();
        assert_eq!(vc011.len(), 1);
        assert!(vc011[0].message.contains("api_key"));
    }

    #[test]
    fn test_log_secret() {
        let d = check(r#"
import logging
password = "secret123"
logging.info(password)
"#);
        let vc011: Vec<_> = d.iter().filter(|d| d.code.0 == "VC011").collect();
        assert_eq!(vc011.len(), 1);
    }

    #[test]
    fn test_no_leak_if_not_tainted() {
        let d = check(r#"
name = "John"
print(name)
"#);
        let vc011: Vec<_> = d.iter().filter(|d| d.code.0 == "VC011").collect();
        assert_eq!(vc011.len(), 0);
    }

    #[test]
    fn test_no_leak_if_no_sink() {
        let d = check(r#"
import os
api_key = os.environ["API_KEY"]
result = api_key.strip()
"#);
        let vc011: Vec<_> = d.iter().filter(|d| d.code.0 == "VC011").collect();
        assert_eq!(vc011.len(), 0);
    }
}
