use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// A node in the import graph representing a Python module/package.
#[derive(Debug, Clone)]
pub struct ModuleNode {
    pub path: PathBuf,
    pub imports: Vec<String>,      // modules this file imports
    pub exported: HashSet<String>, // names defined at module level
}

/// Project-wide import graph.
/// Maps module name → file path and its dependencies.
#[derive(Debug, Default)]
pub struct ImportGraph {
    pub modules: HashMap<String, ModuleNode>,
    pub file_to_module: HashMap<PathBuf, String>,
}

impl ImportGraph {
    /// Build import graph from parsed files.
    /// `files` is a list of (path, source, tree) tuples.
    pub fn build(files: &[(PathBuf, String, tree_sitter::Tree)], project_root: &Path) -> Self {
        let mut graph = Self::default();

        for (path, source, tree) in files {
            let info = crate::extract_file_info(tree, source, path);

            // Derive module name from file path relative to project root
            let module_name = path_to_module(path, project_root);
            if module_name.is_empty() {
                continue;
            }

            // Collect imports
            let imports: Vec<String> = info.imports.iter().map(|i| i.module.clone()).collect();

            // Collect module-level exported names (from symbol table)
            let symtable = crate::symbols::SymbolTable::build(tree, source);
            let mut exported = HashSet::new();
            if let Some(bindings) = get_module_level_bindings(&symtable) {
                exported = bindings;
            }

            graph.modules.insert(
                module_name.clone(),
                ModuleNode {
                    path: path.clone(),
                    imports,
                    exported,
                },
            );
            graph.file_to_module.insert(path.clone(), module_name);
        }

        graph
    }

    /// Check if a module exists in the project.
    pub fn has_module(&self, name: &str) -> bool {
        self.modules.contains_key(name)
    }

    /// Get all modules that import a given module.
    pub fn dependents(&self, module: &str) -> Vec<String> {
        self.modules
            .iter()
            .filter(|(_, node)| node.imports.contains(&module.to_string()))
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Get all modules that a given module imports.
    pub fn dependencies(&self, module: &str) -> Option<&Vec<String>> {
        self.modules.get(module).map(|n| &n.imports)
    }
}

/// Convert file path to Python module name.
/// e.g. src/mypackage/utils.py → mypackage.utils
fn path_to_module(path: &Path, root: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    let mut parts: Vec<&str> = rel
        .components()
        .filter_map(|c| {
            if let std::path::Component::Normal(s) = c {
                s.to_str()
            } else {
                None
            }
        })
        .collect();

    // Remove .py extension from last part
    if let Some(last) = parts.last_mut() {
        if last.ends_with(".py") {
            *last = &last[..last.len() - 3];
        }
    }

    // Skip common source layout prefixes
    if !parts.is_empty() && (parts[0] == "src" || parts[0] == "lib") {
        parts.remove(0);
    }

    // Remove __init__ (package init files)
    if parts.last().is_some_and(|p| *p == "__init__") {
        parts.pop();
    }

    parts.join(".")
}

fn get_module_level_bindings(_symtable: &crate::symbols::SymbolTable) -> Option<HashSet<String>> {
    // This is a simplified version — returns all names bound at depth 0
    // Full implementation would inspect the symbol table's bindings directly
    // For now, we expose this through the existing API
    None // TODO: expose module-level name enumeration from SymbolTable
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_to_module() {
        let root = Path::new("/project");
        assert_eq!(
            path_to_module(Path::new("/project/src/mypackage/utils.py"), root),
            "mypackage.utils"
        );
        assert_eq!(
            path_to_module(Path::new("/project/mypackage/__init__.py"), root),
            "mypackage"
        );
        assert_eq!(path_to_module(Path::new("/project/main.py"), root), "main");
    }
}
