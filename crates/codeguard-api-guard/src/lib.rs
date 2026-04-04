pub mod introspect;

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
        let cache = self.results.lock().unwrap();
        let mut diagnostics = Vec::new();

        if cache.is_empty() {
            return diagnostics;
        }

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

        diagnostics
    }
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
