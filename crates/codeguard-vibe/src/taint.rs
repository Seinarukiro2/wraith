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
            let call_text = &source[byte_range(
                call.span.start_line,
                call.span.start_col,
                call.span.end_line,
                call.span.end_col,
                source,
            )];
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
/// Three-level approach inspired by Argus (arXiv 2512.08326):
///   Level 1: regex on variable name (bigram classification)
///   Level 2: entropy threshold on value (Gitleaks/TruffleHog approach)
///   Level 3: source analysis (os.environ → only if value is high-entropy or name is secret)
fn collect_tainted_names(info: &FileInfo) -> HashSet<String> {
    let mut tainted = HashSet::new();

    for assign in &info.assignments {
        let target_lower = assign.target.to_lowercase();
        let is_env_source = assign.value.as_ref().is_some_and(|v| {
            v.contains("os.environ") || v.contains("os.getenv") || v.contains("getenv(")
        });

        // Level 1: Bigram classification on variable name segments
        let name_is_secret = is_secret_name_bigram(&target_lower);

        // Level 2: Entropy check on value (if string literal)
        let value_is_high_entropy = assign.value_is_string
            && assign.value.as_ref().is_some_and(|v| {
                let unquoted = v
                    .trim_start_matches(|c: char| c == '\'' || c == '\"' || c == 'f')
                    .trim_end_matches(|c: char| c == '\'' || c == '\"');
                unquoted.len() >= 8 && shannon_entropy(unquoted) > 3.5
            });

        // Decision matrix (Argus-inspired three-level):
        // Level 1: secret name from env → taint (api_key = os.environ["API_KEY"])
        // Level 2: secret name + string literal → taint (password = "secret123")
        // Level 3: env source + non-secret name + high entropy value → taint
        // Skip: env source + config name (PORT, TIMEZONE, MAX_TOKENS)
        // Skip: string literal without secret name
        if name_is_secret && (is_env_source || assign.value_is_string) {
            tainted.insert(assign.target.clone());
        } else if is_env_source && !name_is_secret && value_is_high_entropy {
            // Unknown env var with high-entropy value — might be secret
            tainted.insert(assign.target.clone());
        }
    }

    tainted
}

/// Shannon entropy of a string (bits per character).
fn shannon_entropy(s: &str) -> f64 {
    let mut freq = [0u32; 256];
    let len = s.len() as f64;
    if len == 0.0 {
        return 0.0;
    }
    for &b in s.as_bytes() {
        freq[b as usize] += 1;
    }
    let mut entropy = 0.0f64;
    for &count in &freq {
        if count > 0 {
            let p = count as f64 / len;
            entropy -= p * p.log2();
        }
    }
    entropy
}

/// Bigram classification on variable name segments.
/// Splits name by _ and checks if segment pairs indicate a secret vs config.
/// "api_key" → (api, key) → SECRET
/// "max_tokens" → (max, tokens) → CONFIG (skip)
/// "access_token" → (access, token) → SECRET
/// "token_count" → (token, count) → CONFIG (skip)
fn is_secret_name_bigram(name: &str) -> bool {
    let segments: Vec<&str> = name.split('_').collect();

    // Single-word checks (exact matches only)
    if segments.len() == 1 {
        return matches!(
            segments[0],
            "password" | "passwd" | "secret" | "credential" | "credentials"
        );
    }

    // Secret-indicating bigrams: (modifier, noun) pairs
    const SECRET_BIGRAMS: &[(&str, &str)] = &[
        ("api", "key"),
        ("api", "secret"),
        ("api", "token"),
        ("access", "key"),
        ("access", "token"),
        ("access", "secret"),
        ("secret", "key"),
        ("private", "key"),
        ("auth", "token"),
        ("auth", "key"),
        ("auth", "secret"),
        ("bot", "token"),
        ("client", "secret"),
        ("client", "id"), // debatable, but often sensitive
        ("bearer", "token"),
        ("refresh", "token"),
        ("session", "token"),
        ("session", "secret"),
        ("signing", "key"),
        ("encryption", "key"),
        ("master", "key"),
        ("db", "password"),
        ("database", "password"),
        ("jwt", "secret"),
        ("jwt", "key"),
        ("webhook", "secret"),
        ("stripe", "key"),
        ("openai", "key"),
        ("anthropic", "key"),
    ];

    // Config-indicating segments that neutralize "token"
    const CONFIG_MODIFIERS: &[&str] = &[
        "max", "min", "num", "count", "total", "default", "timeout", "limit", "size", "length",
        "retry", "poll", "interval", "batch", "chunk", "page", "per",
    ];

    const CONFIG_NOUNS: &[&str] = &[
        "count",
        "limit",
        "size",
        "length",
        "timeout",
        "interval",
        "retries",
        "attempts",
        "path",
        "dir",
        "directory",
        "folder",
        "file",
        "name",
        "host",
        "port",
        "url",
        "uri",
        "endpoint",
        "version",
        "level",
        "mode",
        "type",
        "format",
        "encoding",
        "timezone",
        "locale",
        "region",
        "zone",
        "env",
        "environment",
        "prefix",
        "suffix",
        "separator",
        "delimiter",
        "processed",
        "remaining",
        "used",
        "available",
        "capacity",
    ];

    // Check all adjacent segment pairs
    for pair in segments.windows(2) {
        let (a, b) = (pair[0], pair[1]);

        // Check if pair is a known secret bigram
        if SECRET_BIGRAMS.iter().any(|(sa, sb)| a == *sa && b == *sb) {
            return true;
        }
    }

    // Check if name contains "token" or "secret" or "password"
    let has_sensitive_word = segments.iter().any(|s| {
        matches!(
            *s,
            "token" | "secret" | "password" | "passwd" | "key" | "credential"
        )
    });

    if !has_sensitive_word {
        return false;
    }

    // If has sensitive word, check if neutralized by config context
    static MOD_SET: once_cell::sync::Lazy<std::collections::HashSet<&str>> =
        once_cell::sync::Lazy::new(|| CONFIG_MODIFIERS.iter().copied().collect());
    static NOUN_SET: once_cell::sync::Lazy<std::collections::HashSet<&str>> =
        once_cell::sync::Lazy::new(|| CONFIG_NOUNS.iter().copied().collect());

    let has_config_modifier = segments.iter().any(|s| MOD_SET.contains(s));
    let has_config_noun = segments.iter().any(|s| NOUN_SET.contains(s));

    if has_config_modifier || has_config_noun {
        return false; // e.g., "max_tokens", "token_count", "key_length"
    }

    // Has sensitive word without config neutralizer → likely secret
    true
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
            return matches!(
                func,
                "info" | "debug" | "warning" | "error" | "critical" | "exception" | "log"
            );
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

fn byte_range(
    start_line: u32,
    start_col: u32,
    end_line: u32,
    end_col: u32,
    source: &str,
) -> std::ops::Range<usize> {
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
        let d = check(
            r#"
import os
api_key = os.environ["API_KEY"]
print(api_key)
"#,
        );
        let vc011: Vec<_> = d.iter().filter(|d| d.code.0 == "VC011").collect();
        assert_eq!(vc011.len(), 1);
        assert!(vc011[0].message.contains("api_key"));
    }

    #[test]
    fn test_log_secret() {
        let d = check(
            r#"
import logging
password = "secret123"
logging.info(password)
"#,
        );
        let vc011: Vec<_> = d.iter().filter(|d| d.code.0 == "VC011").collect();
        assert_eq!(vc011.len(), 1);
    }

    #[test]
    fn test_no_leak_if_not_tainted() {
        let d = check(
            r#"
name = "John"
print(name)
"#,
        );
        let vc011: Vec<_> = d.iter().filter(|d| d.code.0 == "VC011").collect();
        assert_eq!(vc011.len(), 0);
    }

    #[test]
    fn test_no_leak_if_no_sink() {
        let d = check(
            r#"
import os
api_key = os.environ["API_KEY"]
result = api_key.strip()
"#,
        );
        let vc011: Vec<_> = d.iter().filter(|d| d.code.0 == "VC011").collect();
        assert_eq!(vc011.len(), 0);
    }
}
