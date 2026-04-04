use anyhow::Result;
use clap::{Parser, Subcommand};
use codeguard_core::config::Config;
use codeguard_core::diagnostic::TextEdit;
use codeguard_core::reporter::{format_diagnostics, OutputFormat};
use codeguard_core::rules;
use codeguard_core::Diagnostic;
use colored::Colorize;
use rayon::prelude::*;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "codeguard",
    version,
    about = "Deterministic linter for AI-generated Python code"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Lint Python files for AI-generated code issues
    Check {
        /// Files or directories to check
        paths: Vec<PathBuf>,

        /// Select specific rule prefixes (e.g., AG,PH,VC)
        #[arg(long, value_delimiter = ',')]
        select: Option<Vec<String>>,

        /// Apply auto-fixes
        #[arg(long)]
        fix: bool,

        /// Exit with code 1 if any issues found
        #[arg(long)]
        strict: bool,

        /// Show verbose output
        #[arg(long, short)]
        verbose: bool,

        /// Offline mode (skip PyPI HTTP checks)
        #[arg(long)]
        offline: bool,

        /// Output format (text or json)
        #[arg(long, default_value = "text")]
        format: String,

        /// Path to config file
        #[arg(long)]
        config: Option<PathBuf>,
    },
    /// List all available rules
    Rules,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Rules => {
            print_rules();
            Ok(())
        }
        Commands::Check {
            paths,
            select,
            fix,
            strict,
            verbose,
            offline,
            format,
            config: config_path,
        } => {
            let mut config = config_path
                .as_ref()
                .map(|p| Config::load_from_file(p))
                .transpose()?
                .unwrap_or_else(|| {
                    let cwd = std::env::current_dir().unwrap_or_default();
                    Config::discover(&cwd)
                });

            // CLI flags override config
            if let Some(sel) = select {
                config.select = Some(sel);
            }
            config.fix = fix;
            config.strict = strict;
            config.verbose = verbose;
            config.offline = offline;

            let output_format: OutputFormat = format.parse().unwrap_or(OutputFormat::Text);

            run_check(&config, &paths, output_format)
        }
    }
}

fn run_check(config: &Config, paths: &[PathBuf], format: OutputFormat) -> Result<()> {
    // 1. Discover .py files
    let files = discover_files(paths)?;
    if files.is_empty() {
        if config.verbose {
            eprintln!("No Python files found.");
        }
        return Ok(());
    }

    if config.verbose {
        eprintln!("Checking {} file(s)...", files.len());
    }

    // 2. Read and parse all files
    let parsed: Vec<_> = files
        .par_iter()
        .filter_map(|path| {
            let source = std::fs::read_to_string(path).ok()?;
            let tree = codeguard_ast::parse_python(&source)?;
            Some((path.clone(), source, tree))
        })
        .collect();

    // 3. Initialize linters
    let run_ag = config.is_rule_enabled("AG");
    let run_ph = config.is_rule_enabled("PH");
    let run_vc = config.is_rule_enabled("VC");

    // 4. Batch collect phase
    let api_guard = if run_ag {
        let ag = codeguard_api_guard::ApiGuardLinter::new(config.python_exec());
        let mut all_queries = Vec::new();
        for (path, source, tree) in &parsed {
            all_queries.extend(ag.collect_queries(tree, source, path));
        }
        // Deduplicate
        all_queries.sort();
        all_queries.dedup();
        if config.verbose {
            eprintln!("  API Guard: {} unique queries", all_queries.len());
        }
        ag.prefetch(&all_queries);
        Some(ag)
    } else {
        None
    };

    let phantom = if run_ph {
        match codeguard_phantom::PhantomLinter::new(config) {
            Ok(ph) => {
                let mut all_packages = Vec::new();
                for (path, source, tree) in &parsed {
                    all_packages.extend(ph.collect_packages(tree, source, path));
                }
                all_packages.sort();
                all_packages.dedup();
                if config.verbose {
                    eprintln!("  Phantom: {} unique packages", all_packages.len());
                }
                ph.prefetch(&all_packages);
                Some(ph)
            }
            Err(e) => {
                if config.verbose {
                    eprintln!("  Phantom init failed: {e}");
                }
                None
            }
        }
    } else {
        None
    };

    // 5. Lint phase (parallel)
    let all_diagnostics: Vec<Diagnostic> = parsed
        .par_iter()
        .flat_map(|(path, source, tree)| {
            let mut diags = Vec::new();

            if run_vc {
                diags.extend(codeguard_vibe::lint_vibe(tree, source, path));
            }

            if let Some(ref ph) = phantom {
                diags.extend(ph.lint(tree, source, path));
            }

            if let Some(ref ag) = api_guard {
                diags.extend(ag.lint(tree, source, path));
            }

            // Filter by selected rules
            diags.retain(|d| config.is_rule_enabled(&d.code.0));
            diags
        })
        .collect();

    // 6. Sort deterministically
    let mut diagnostics = all_diagnostics;
    diagnostics.sort_by(|a, b| {
        a.span
            .file
            .cmp(&b.span.file)
            .then(a.span.start_line.cmp(&b.span.start_line))
            .then(a.span.start_col.cmp(&b.span.start_col))
            .then(a.code.0.cmp(&b.code.0))
    });

    // 7. Apply fixes if requested
    if config.fix {
        apply_fixes(&parsed, &diagnostics)?;
    }

    // 8. Report
    if diagnostics.is_empty() {
        if config.verbose {
            eprintln!("{}", "All checks passed!".green());
        }
        return Ok(());
    }

    let output = format_diagnostics(&diagnostics, format);
    print!("{output}");

    // 9. Exit code
    if config.strict {
        std::process::exit(1);
    }

    Ok(())
}

fn discover_files(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for path in paths {
        if path.is_file() {
            if path.extension().map_or(false, |e| e == "py") {
                files.push(path.clone());
            }
        } else if path.is_dir() {
            walk_dir(path, &mut files)?;
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn walk_dir(dir: &PathBuf, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        // Skip hidden dirs and common non-source dirs
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.')
                || name == "node_modules"
                || name == "__pycache__"
                || name == ".venv"
                || name == "venv"
                || name == ".git"
                || name == "target"
                || name == ".tox"
                || name == ".eggs"
            {
                continue;
            }
        }

        if path.is_dir() {
            walk_dir(&path, files)?;
        } else if path.extension().map_or(false, |e| e == "py") {
            files.push(path);
        }
    }
    Ok(())
}

fn apply_fixes(
    parsed: &[(PathBuf, String, tree_sitter::Tree)],
    diagnostics: &[Diagnostic],
) -> Result<()> {
    use std::collections::HashMap;

    // Group fixes by file
    let mut fixes_by_file: HashMap<&PathBuf, Vec<&TextEdit>> = HashMap::new();
    for d in diagnostics {
        if let Some(ref fix) = d.fix {
            let file = &d.span.file;
            fixes_by_file.entry(file).or_default().push(fix);
        }
    }

    for (path, source, _) in parsed {
        if let Some(fixes) = fixes_by_file.get::<PathBuf>(path) {
            let fixed = apply_text_edits(source, fixes);
            std::fs::write(path, fixed)?;
            eprintln!(
                "  {} {} fix(es) applied to {}",
                "Fixed:".green().bold(),
                fixes.len(),
                path.display()
            );
        }
    }

    Ok(())
}

fn apply_text_edits(source: &str, edits: &[&TextEdit]) -> String {
    let line_index = codeguard_ast::LineIndex::new(source);
    let mut sorted: Vec<_> = edits.to_vec();
    // Sort by start position descending so we can apply from end to start
    sorted.sort_by(|a, b| {
        let a_off = line_index.byte_offset(a.start_line, a.start_col);
        let b_off = line_index.byte_offset(b.start_line, b.start_col);
        b_off.cmp(&a_off)
    });

    let mut result = source.to_string();
    for edit in sorted {
        let start = line_index.byte_offset(edit.start_line, edit.start_col);
        let end = line_index.byte_offset(edit.end_line, edit.end_col);
        if start <= end && end <= result.len() {
            result.replace_range(start..end, &edit.replacement);
        }
    }
    result
}

fn print_rules() {
    let rules = rules::all_rules();
    println!("{:<8} {:<30} {:<6} {}", "Code", "Name", "Fix", "Description");
    println!("{}", "-".repeat(80));
    for rule in rules {
        println!(
            "{:<8} {:<30} {:<6} {}",
            rule.code,
            rule.name,
            if rule.fixable { "yes" } else { "-" },
            rule.description,
        );
    }
}
