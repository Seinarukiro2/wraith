pub mod context_match;
pub mod introspect;
pub mod known_functions;

use codeguard_ast::extract_file_info;
use codeguard_core::diagnostic::TextEdit;
use codeguard_core::{Diagnostic, RuleCode};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use tree_sitter::Tree;

pub struct ApiGuardLinter {
    introspector: introspect::PythonIntrospector,
    results: std::sync::Mutex<HashMap<String, introspect::IntrospectResult>>,
}

impl ApiGuardLinter {
    pub fn new(python_exec: &str) -> Self {
        Self {
            introspector: introspect::PythonIntrospector::new(python_exec.to_string()),
            results: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Collect unique (module, attribute) pairs needed for introspection
    pub fn collect_queries(
        &self,
        tree: &Tree,
        source: &str,
        path: &Path,
    ) -> Vec<(String, String)> {
        let info = extract_file_info(tree, source, path);
        let mut queries = Vec::new();
        let mut seen = HashSet::new();

        // Build alias → module mapping from imports
        let mut alias_map: HashMap<String, String> = HashMap::new();
        for imp in &info.imports {
            for name in &imp.names {
                let alias = name.alias.as_ref().unwrap_or(&name.name);
                if imp.is_from {
                    // from X import Y — Y refers to X.Y
                    alias_map.insert(alias.clone(), format!("{}.{}", imp.module, name.name));
                } else {
                    // import X — X refers to X
                    alias_map.insert(alias.clone(), name.name.clone());
                }
            }
        }

        for call in &info.calls {
            if let Some(ref receiver) = call.receiver {
                // Only introspect if top-level receiver is a known import
                let top = receiver.split('.').next().unwrap_or(receiver);
                if !alias_map.contains_key(top) {
                    continue;
                }
                let module = resolve_module(receiver, &alias_map);
                let key = format!("{}.{}", module, call.function);
                if seen.insert(key) {
                    queries.push((module, call.function.clone()));
                }
            }
        }

        queries
    }

    /// Batch introspect all collected queries
    pub fn prefetch(&self, queries: &[(String, String)]) {
        if queries.is_empty() {
            return;
        }
        match self.introspector.batch_introspect(queries) {
            Ok(results) => {
                let mut cache = self.results.lock().unwrap();
                for (key, result) in results {
                    cache.insert(key, result);
                }
            }
            Err(_) => {
                // Python not available or introspection failed — skip AG rules
            }
        }
    }

    /// Lint a single file (call after prefetch)
    pub fn lint(&self, tree: &Tree, source: &str, path: &Path) -> Vec<Diagnostic> {
        let info = extract_file_info(tree, source, path);
        let symtable = codeguard_ast::SymbolTable::build(tree, source);
        let cache = self.results.lock().unwrap();
        let mut diagnostics = Vec::new();

        // Build alias → module mapping
        let mut alias_map: HashMap<String, String> = HashMap::new();
        for imp in &info.imports {
            for name in &imp.names {
                let alias = name.alias.as_ref().unwrap_or(&name.name);
                if imp.is_from {
                    alias_map.insert(alias.clone(), format!("{}.{}", imp.module, name.name));
                } else {
                    alias_map.insert(alias.clone(), name.name.clone());
                }
            }
        }

        // AG001-AG003: introspection-based checks (need cache)
        if !cache.is_empty() {
        for call in &info.calls {
            if let Some(ref receiver) = call.receiver {
                let top = receiver.split('.').next().unwrap_or(receiver);
                if !alias_map.contains_key(top) {
                    continue;
                }
                let module = resolve_module(receiver, &alias_map);
                let key = format!("{}.{}", module, call.function);

                if let Some(result) = cache.get(&key) {
                    // Skip if module wasn't importable (e.g. os.environ is a dict, not a module)
                    if !result.module_found {
                        continue;
                    }

                    // AG001: attribute/method doesn't exist
                    if !result.exists {
                        let mut d = Diagnostic::error(
                            RuleCode::new("AG001"),
                            call.span.clone(),
                            format!(
                                "{}.{}: no such attribute in module '{}'",
                                receiver, call.function, module
                            ),
                        );
                        if let Some(ref suggestion) = result.closest_match {
                            d = d.with_suggestion(format!("did you mean '{suggestion}'?"));
                            // Autofix: replace the hallucinated attribute with the suggestion
                            let fs = &call.function_span;
                            d = d.with_fix(TextEdit {
                                start_line: fs.start_line,
                                start_col: fs.start_col,
                                end_line: fs.end_line,
                                end_col: fs.end_col,
                                replacement: suggestion.clone(),
                            });
                        }
                        diagnostics.push(d);
                        continue;
                    }

                    // AG002: non-existent keyword argument
                    if let Some(ref sig) = result.signature {
                        if !sig.has_var_keyword {
                            for kwarg in &call.keyword_args {
                                if !sig.params.iter().any(|p| p.name == kwarg.name) {
                                    let suggestion = find_closest_param(&kwarg.name, &sig.params);
                                    let mut d = Diagnostic::error(
                                        RuleCode::new("AG002"),
                                        call.span.clone(),
                                        format!(
                                            "{}: unknown parameter '{}'",
                                            call.full_name, kwarg.name,
                                        ),
                                    );
                                    if let Some(ref s) = suggestion {
                                        d = d.with_suggestion(format!("did you mean '{s}'?"));
                                        // Autofix: replace the hallucinated kwarg name
                                        let ks = &kwarg.name_span;
                                        d = d.with_fix(TextEdit {
                                            start_line: ks.start_line,
                                            start_col: ks.start_col,
                                            end_line: ks.end_line,
                                            end_col: ks.end_col,
                                            replacement: s.clone(),
                                        });
                                    }
                                    diagnostics.push(d);
                                }
                            }
                        }
                    }

                    // AG003: deprecated
                    if result.deprecated {
                        diagnostics.push(
                            Diagnostic::warning(
                                RuleCode::new("AG003"),
                                call.span.clone(),
                                format!("{} is deprecated", call.full_name),
                            )
                            .with_suggestion("check documentation for replacement"),
                        );
                    }
                }
            }
        }
        } // end if !cache.is_empty()

        // AG006: contextual mismatch (file extension vs function semantics)
        // Only check qualified calls (pd.read_csv, json.load) — not bare load()
        for call in &info.calls {
            if !call.receiver_is_name || call.receiver.is_none() {
                continue;
            }
            if let Some(ref filename) = call.first_string_arg {
                if let Some(mismatch) = context_match::check_extension_match(&call.function, filename) {
                    let correct_func = suggest_correct_function(&call.function, filename);
                    let mut d = Diagnostic::warning(
                        RuleCode::new("AG006"),
                        call.span.clone(),
                        format!("{}: {}", call.full_name, mismatch),
                    );
                    if let Some(ref correct) = correct_func {
                        d = d.with_suggestion(format!("use {correct}() instead"));
                        if call.receiver.is_some() {
                            d = d.with_fix(TextEdit {
                                start_line: call.function_span.start_line,
                                start_col: call.function_span.start_col,
                                end_line: call.function_span.end_line,
                                end_col: call.function_span.end_col,
                                replacement: correct.clone(),
                            });
                        }
                    }
                    diagnostics.push(d);
                }
            }
        }

        // AG004: bare library function calls without module qualifier
        let bare_map = known_functions::bare_call_map();
        // Build set of names imported via "from X import func"
        let mut from_imported: HashSet<String> = HashSet::new();
        for imp in &info.imports {
            if imp.is_from {
                for name in &imp.names {
                    let actual = name.alias.as_ref().unwrap_or(&name.name);
                    from_imported.insert(actual.clone());
                }
            }
        }

        for call in &info.calls {
            if call.receiver.is_some() {
                continue; // qualified call, not bare
            }
            // Skip if function was explicitly imported via "from X import func"
            if from_imported.contains(&call.function) {
                continue;
            }
            // Skip if name is visible at this line (parameter, local var in enclosing scope)
            if symtable.is_visible_at(&call.function, call.span.start_line) {
                continue;
            }
            // Skip builtins
            if is_python_builtin(&call.function) {
                continue;
            }
            if let Some(&(module, alias)) = bare_map.get(call.function.as_str()) {
                let prefix = if alias != module { alias } else { module };
                let replacement = format!("{}.{}", prefix, call.function);
                diagnostics.push(
                    Diagnostic::warning(
                        RuleCode::new("AG004"),
                        call.span.clone(),
                        format!(
                            "{}() called without module qualifier",
                            call.function,
                        ),
                    )
                    .with_suggestion(format!("use {replacement}()"))
                    .with_fix(TextEdit {
                        start_line: call.function_span.start_line,
                        start_col: call.function_span.start_col,
                        end_line: call.function_span.end_line,
                        end_col: call.function_span.end_col,
                        replacement,
                    }),
                );
            }
        }

        // AG005: module used but never imported (symbol table approach)
        // Two-pass: symbol table already built above tells us what each name is bound to.
        // If X.method() and X is bound as import → already checked by AG001-AG003.
        // If X is bound as local (assignment, param, for, with) → skip.
        // If X is NOT bound at all → it's used but never defined. Flag if it looks like a module.
        let alias_imports = common_alias_map();
        let mut reported_modules: HashSet<String> = HashSet::new();
        for call in &info.calls {
            // Only check calls where receiver is a plain identifier chain (pd.X, os.path.X)
            // Skip calls on expressions: super().X, Path(x).X, "str".X, obj[i].X
            if !call.receiver_is_name {
                continue;
            }
            if let Some(ref receiver) = call.receiver {
                let top = receiver.split('.').next().unwrap_or(receiver);

                // Skip single-char receivers — almost always local variables (x, e, f, v)
                if top.len() < 2 {
                    continue;
                }

                // PEP 227: check if name is visible at the usage line.
                if symtable.is_visible_at(top, call.span.start_line) {
                    continue;
                }

                // Skip Python builtins
                if is_python_builtin(top) {
                    continue;
                }

                // Name is used but never bound — likely a missing import
                if reported_modules.insert(top.to_string()) {
                    let import_stmt = alias_imports
                        .get(top)
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| format!("import {top}"));
                    diagnostics.push(
                        Diagnostic::warning(
                            RuleCode::new("AG005"),
                            call.span.clone(),
                            format!("'{top}' is used but never imported"),
                        )
                        .with_suggestion(format!("add '{import_stmt}' at the top of the file"))
                        .with_fix(TextEdit {
                            start_line: 1,
                            start_col: 0,
                            end_line: 1,
                            end_col: 0,
                            replacement: format!("{import_stmt}\n"),
                        }),
                    );
                }
            }
        }

        diagnostics
    }
}

fn is_python_builtin(name: &str) -> bool {
    BUILTINS_SET.contains(name)
}

static BUILTINS_SET: once_cell::sync::Lazy<std::collections::HashSet<&str>> =
    once_cell::sync::Lazy::new(|| PYTHON_BUILTINS.iter().copied().collect());

const PYTHON_BUILTINS: &[&str] = &[
    "abs", "all", "any", "ascii", "bin", "bool", "breakpoint", "bytearray",
    "bytes", "callable", "chr", "classmethod", "compile", "complex",
    "delattr", "dict", "dir", "divmod", "enumerate", "eval", "exec",
    "filter", "float", "format", "frozenset", "getattr", "globals",
    "hasattr", "hash", "help", "hex", "id", "input", "int", "isinstance",
    "issubclass", "iter", "len", "list", "locals", "map", "max",
    "memoryview", "min", "next", "object", "oct", "open", "ord", "pow",
    "print", "property", "range", "repr", "reversed", "round", "set",
    "setattr", "slice", "sorted", "staticmethod", "str", "sum", "super",
    "tuple", "type", "vars", "zip", "__import__",
    // common test/framework names that are not library imports
    "self", "cls", "app", "db", "client", "request", "response",
    "session", "config", "logger", "log",
];

fn suggest_correct_function(current_func: &str, filename: &str) -> Option<String> {
    let lower = filename.to_lowercase();
    let ext_map: &[(&str, &str)] = &[
        (".csv", "read_csv"),
        (".tsv", "read_csv"),
        (".xlsx", "read_excel"),
        (".xls", "read_excel"),
        (".json", "read_json"),
        (".jsonl", "read_json"),
        (".parquet", "read_parquet"),
        (".pq", "read_parquet"),
        (".feather", "read_feather"),
        (".h5", "read_hdf"),
        (".hdf5", "read_hdf"),
        (".xml", "read_xml"),
        (".html", "read_html"),
        (".htm", "read_html"),
        (".pkl", "read_pickle"),
        (".pickle", "read_pickle"),
        (".dta", "read_stata"),
        (".sav", "read_spss"),
    ];

    for (ext, func) in ext_map {
        if lower.ends_with(ext) && *func != current_func {
            return Some(func.to_string());
        }
    }
    None
}

fn common_alias_map() -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();
    m.insert("np", "import numpy as np");
    m.insert("pd", "import pandas as pd");
    m.insert("plt", "import matplotlib.pyplot as plt");
    m.insert("tf", "import tensorflow as tf");
    m.insert("sns", "import seaborn as sns");
    m.insert("cv2", "import cv2");
    m.insert("sk", "import sklearn as sk");
    m.insert("sp", "import scipy as sp");
    m
}

fn resolve_module(receiver: &str, alias_map: &HashMap<String, String>) -> String {
    // Try to resolve the top-level identifier through aliases
    let top = receiver.split('.').next().unwrap_or(receiver);
    if let Some(resolved) = alias_map.get(top) {
        if receiver.contains('.') {
            let rest = &receiver[top.len()..];
            format!("{resolved}{rest}")
        } else {
            resolved.clone()
        }
    } else {
        receiver.to_string()
    }
}

fn find_closest_param(
    name: &str,
    params: &[introspect::ParamInfo],
) -> Option<String> {
    let mut best: Option<(String, usize)> = None;
    for p in params {
        let dist = strsim::levenshtein(name, &p.name);
        if dist <= 3 {
            match &best {
                None => best = Some((p.name.clone(), dist)),
                Some((_, bd)) if dist < *bd => best = Some((p.name.clone(), dist)),
                _ => {}
            }
        }
    }
    best.map(|(n, _)| n)
}
