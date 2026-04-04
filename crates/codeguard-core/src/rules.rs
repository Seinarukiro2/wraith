use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RuleCode(pub String);

impl RuleCode {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    pub fn prefix(&self) -> &str {
        let end = self
            .0
            .find(|c: char| c.is_ascii_digit())
            .unwrap_or(self.0.len());
        &self.0[..end]
    }

    pub fn matches_selector(&self, selector: &str) -> bool {
        let selector_upper = selector.to_uppercase();
        let code_upper = self.0.to_uppercase();
        code_upper.starts_with(&selector_upper)
    }
}

impl std::fmt::Display for RuleCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone)]
pub struct RuleInfo {
    pub code: RuleCode,
    pub name: &'static str,
    pub description: &'static str,
    pub fixable: bool,
}

// AG rules
pub const AG001: &str = "AG001";
pub const AG002: &str = "AG002";
pub const AG003: &str = "AG003";

// PH rules
pub const PH001: &str = "PH001";
pub const PH002: &str = "PH002";
pub const PH003: &str = "PH003";

// VC rules
pub const VC001: &str = "VC001";
pub const VC002: &str = "VC002";
pub const VC003: &str = "VC003";
pub const VC004: &str = "VC004";
pub const VC005: &str = "VC005";
pub const VC006: &str = "VC006";

pub fn all_rules() -> Vec<RuleInfo> {
    vec![
        RuleInfo {
            code: RuleCode::new(AG001),
            name: "non-existent-attribute",
            description: "Module attribute or method does not exist",
            fixable: true,
        },
        RuleInfo {
            code: RuleCode::new(AG002),
            name: "non-existent-kwarg",
            description: "Keyword argument does not exist in function signature",
            fixable: true,
        },
        RuleInfo {
            code: RuleCode::new(AG003),
            name: "deprecated-api",
            description: "API is deprecated",
            fixable: false,
        },
        RuleInfo {
            code: RuleCode::new(PH001),
            name: "package-not-found",
            description: "Package not found on PyPI",
            fixable: false,
        },
        RuleInfo {
            code: RuleCode::new(PH002),
            name: "package-not-installed",
            description: "Package not installed in current environment",
            fixable: false,
        },
        RuleInfo {
            code: RuleCode::new(PH003),
            name: "suspicious-package",
            description: "Package appears suspicious (new, low downloads, or typosquat)",
            fixable: false,
        },
        RuleInfo {
            code: RuleCode::new(VC001),
            name: "hardcoded-secret",
            description: "Hardcoded secret detected in source",
            fixable: true,
        },
        RuleInfo {
            code: RuleCode::new(VC002),
            name: "ai-artifact-comment",
            description: "AI-generated artifact in comment",
            fixable: true,
        },
        RuleInfo {
            code: RuleCode::new(VC003),
            name: "debug-code",
            description: "Debug code left in source (print, breakpoint, pdb)",
            fixable: true,
        },
        RuleInfo {
            code: RuleCode::new(VC004),
            name: "pdb-import",
            description: "Debug import (pdb) left in source",
            fixable: true,
        },
        RuleInfo {
            code: RuleCode::new(VC005),
            name: "source-map-exposure",
            description: "Source map file or reference exposed",
            fixable: false,
        },
        RuleInfo {
            code: RuleCode::new(VC006),
            name: "suspicious-endpoint",
            description: "Debug/admin endpoint without auth decorator",
            fixable: false,
        },
    ]
}
