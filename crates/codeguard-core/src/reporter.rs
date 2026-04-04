use crate::Diagnostic;
use colored::Colorize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
}

impl std::str::FromStr for OutputFormat {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "text" => Ok(OutputFormat::Text),
            "json" => Ok(OutputFormat::Json),
            _ => Err(format!("unknown format: {s}")),
        }
    }
}

pub fn format_diagnostics(diagnostics: &[Diagnostic], format: OutputFormat) -> String {
    match format {
        OutputFormat::Text => format_text(diagnostics),
        OutputFormat::Json => format_json(diagnostics),
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
            let pad = " ".repeat(padding_len);
            out.push_str(&format!(
                "{} {} {}{}\n",
                pad,
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
        out.push_str(&format!(
            "\nFound {} issue{} ({} auto-fixable). Run with {} to apply.\n",
            diagnostics.len(),
            if diagnostics.len() == 1 { "" } else { "s" },
            fixable_count,
            "--fix".bold(),
        ));
    }

    out
}

fn format_json(diagnostics: &[Diagnostic]) -> String {
    serde_json::to_string_pretty(diagnostics).unwrap_or_default()
}
