use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Error => write!(f, "error"),
            Severity::Warning => write!(f, "warning"),
            Severity::Info => write!(f, "info"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub file: PathBuf,
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
}

impl Span {
    pub fn new(file: PathBuf, start_line: u32, start_col: u32, end_line: u32, end_col: u32) -> Self {
        Self {
            file,
            start_line,
            start_col,
            end_line,
            end_col,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextEdit {
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
    pub replacement: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    pub code: crate::RuleCode,
    pub severity: Severity,
    pub span: Span,
    pub message: String,
    pub suggestion: Option<String>,
    pub fix: Option<TextEdit>,
}

impl Diagnostic {
    pub fn error(code: crate::RuleCode, span: Span, message: impl Into<String>) -> Self {
        Self {
            code,
            severity: Severity::Error,
            span,
            message: message.into(),
            suggestion: None,
            fix: None,
        }
    }

    pub fn warning(code: crate::RuleCode, span: Span, message: impl Into<String>) -> Self {
        Self {
            code,
            severity: Severity::Warning,
            span,
            message: message.into(),
            suggestion: None,
            fix: None,
        }
    }

    pub fn info(code: crate::RuleCode, span: Span, message: impl Into<String>) -> Self {
        Self {
            code,
            severity: Severity::Info,
            span,
            message: message.into(),
            suggestion: None,
            fix: None,
        }
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    pub fn with_fix(mut self, fix: TextEdit) -> Self {
        self.fix = Some(fix);
        self
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}:{} {} {}",
            self.span.file.display(),
            self.span.start_line,
            self.span.start_col,
            self.code,
            self.message,
        )?;
        if let Some(ref suggestion) = self.suggestion {
            write!(f, "\n  -> {suggestion}")?;
        }
        Ok(())
    }
}
