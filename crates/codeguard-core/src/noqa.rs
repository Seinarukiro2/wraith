use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashSet;

/// Regex matching `# noqa` with optional code list.
/// Matches: `# noqa`, `# noqa: VC003`, `# noqa: VC003, AG001`, `# NOQA: vc003`
/// Also matches: `# type: ignore` style won't interfere (different prefix).
static NOQA_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)#\s*noqa(?:\s*:\s*([A-Za-z0-9,\s]+))?").unwrap()
});

/// Parsed noqa directive for a single line.
#[derive(Debug, Clone)]
pub enum NoqaDirective {
    /// `# noqa` — suppress all rules on this line
    All,
    /// `# noqa: VC003, AG001` — suppress specific codes
    Codes(HashSet<String>),
}

/// Parse a source line for a noqa directive.
/// Returns None if no noqa found.
pub fn parse_noqa(line: &str) -> Option<NoqaDirective> {
    let caps = NOQA_RE.captures(line)?;
    match caps.get(1) {
        None => Some(NoqaDirective::All),
        Some(codes_match) => {
            let codes: HashSet<String> = codes_match
                .as_str()
                .split(',')
                .map(|s| s.trim().to_uppercase())
                .filter(|s| !s.is_empty())
                .collect();
            if codes.is_empty() {
                Some(NoqaDirective::All)
            } else {
                Some(NoqaDirective::Codes(codes))
            }
        }
    }
}

/// Build a line → NoqaDirective map for the entire source.
/// Lines are 1-indexed to match Span.start_line.
pub fn build_noqa_map(source: &str) -> Vec<Option<NoqaDirective>> {
    // Index 0 is unused (lines are 1-indexed), but we keep it for simple indexing
    let mut map = vec![None]; // placeholder for line 0
    for line in source.lines() {
        map.push(parse_noqa(line));
    }
    map
}

/// Check if a diagnostic at a given line should be suppressed.
pub fn is_suppressed(noqa_map: &[Option<NoqaDirective>], line: u32, code: &str) -> bool {
    let idx = line as usize;
    if idx >= noqa_map.len() {
        return false;
    }
    match &noqa_map[idx] {
        None => false,
        Some(NoqaDirective::All) => true,
        Some(NoqaDirective::Codes(codes)) => {
            let code_upper = code.to_uppercase();
            codes.contains(&code_upper)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_noqa() {
        assert!(parse_noqa("x = 1").is_none());
        assert!(parse_noqa("# regular comment").is_none());
    }

    #[test]
    fn test_bare_noqa() {
        match parse_noqa("print('hello')  # noqa") {
            Some(NoqaDirective::All) => {}
            other => panic!("expected All, got {:?}", other),
        }
    }

    #[test]
    fn test_noqa_single_code() {
        match parse_noqa("print('hello')  # noqa: VC003") {
            Some(NoqaDirective::Codes(codes)) => {
                assert!(codes.contains("VC003"));
                assert_eq!(codes.len(), 1);
            }
            other => panic!("expected Codes, got {:?}", other),
        }
    }

    #[test]
    fn test_noqa_multiple_codes() {
        match parse_noqa("x = read_csv('f')  # noqa: AG004, VC003") {
            Some(NoqaDirective::Codes(codes)) => {
                assert!(codes.contains("AG004"));
                assert!(codes.contains("VC003"));
                assert_eq!(codes.len(), 2);
            }
            other => panic!("expected Codes, got {:?}", other),
        }
    }

    #[test]
    fn test_noqa_case_insensitive() {
        match parse_noqa("x = 1  # NOQA: vc003") {
            Some(NoqaDirective::Codes(codes)) => {
                assert!(codes.contains("VC003"));
            }
            other => panic!("expected Codes, got {:?}", other),
        }
    }

    #[test]
    fn test_build_map_and_suppress() {
        let source = "import os\nprint('debug')  # noqa: VC003\nprint('also debug')  # noqa\nx = 1\n";
        let map = build_noqa_map(source);
        assert!(!is_suppressed(&map, 1, "VC003")); // import os — no noqa
        assert!(is_suppressed(&map, 2, "VC003"));   // noqa: VC003
        assert!(!is_suppressed(&map, 2, "AG001"));   // noqa: VC003 doesn't suppress AG001
        assert!(is_suppressed(&map, 3, "VC003"));    // noqa (bare) suppresses everything
        assert!(is_suppressed(&map, 3, "AG001"));    // noqa (bare) suppresses everything
        assert!(!is_suppressed(&map, 4, "VC003"));   // x = 1 — no noqa
    }
}
