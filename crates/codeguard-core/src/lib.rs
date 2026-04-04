pub mod config;
pub mod diagnostic;
pub mod reporter;
pub mod rules;
#[cfg(test)]
mod tests;

pub use config::Config;
pub use diagnostic::{Diagnostic, Severity, Span, TextEdit};
pub use rules::RuleCode;
