use std::collections::HashMap;
use tree_sitter::{Node, Tree};

/// Kind of name binding in Python
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingKind {
    /// `import X` or `import X as Y`
    Import,
    /// `from X import Y` or `from X import Y as Z`
    FromImport,
    /// `X = ...` or augmented assignment `X += ...`
    Assignment,
    /// Function/method parameter: `def f(X):`
    Parameter,
    /// For-loop target: `for X in ...`
    ForTarget,
    /// With-statement target: `with ... as X`
    WithTarget,
    /// Exception handler: `except E as X`
    ExceptTarget,
    /// Function definition: `def X():`
    FunctionDef,
    /// Class definition: `class X:`
    ClassDef,
    /// `global X`
    Global,
    /// `nonlocal X`
    Nonlocal,
    /// Comprehension variable: `[... for X in ...]`
    ComprehensionVar,
    /// Walrus operator: `(X := ...)`
    NamedExpr,
}

impl BindingKind {
    /// Is this binding an import of a module?
    pub fn is_import(self) -> bool {
        matches!(self, BindingKind::Import | BindingKind::FromImport)
    }
}

#[derive(Debug, Clone)]
pub struct Binding {
    pub kind: BindingKind,
    pub scope_depth: u32,
    pub scope_start: u32, // start line of enclosing scope
    pub scope_end: u32,   // end line of enclosing scope
    pub line: u32,
}

/// Symbol table with scope awareness (PEP 227 LEGB).
/// Each binding records its enclosing scope range, enabling
/// position-aware resolution: "is X bound in the scope containing line L?"
#[derive(Debug, Default)]
pub struct SymbolTable {
    bindings: HashMap<String, Vec<Binding>>,
}

impl SymbolTable {
    /// Build symbol table from tree-sitter parse tree (first pass).
    pub fn build(tree: &Tree, source: &str) -> Self {
        let mut table = Self::default();
        collect_bindings(tree.root_node(), source, &mut table, 0);
        table
    }

    pub fn add(
        &mut self,
        name: String,
        kind: BindingKind,
        line: u32,
        scope_depth: u32,
        scope_start: u32,
        scope_end: u32,
    ) {
        self.bindings.entry(name).or_default().push(Binding {
            kind,
            scope_depth,
            scope_start,
            scope_end,
            line,
        });
    }

    /// Check if a name was ever bound as an import in this file.
    pub fn is_import(&self, name: &str) -> bool {
        self.bindings
            .get(name)
            .is_some_and(|bs| bs.iter().any(|b| b.kind.is_import()))
    }

    /// Check if a name was ever bound as a non-import (local variable, parameter, etc.)
    pub fn is_local(&self, name: &str) -> bool {
        self.bindings
            .get(name)
            .is_some_and(|bs| bs.iter().any(|b| !b.kind.is_import()))
    }

    /// Check if a name is bound at all (any scope).
    pub fn is_bound(&self, name: &str) -> bool {
        self.bindings.contains_key(name)
    }

    /// Check if a name is visible at a specific line (PEP 227 LEGB).
    /// A binding is visible at line L if:
    /// - It's at module scope (depth 0) — always visible, OR
    /// - Its enclosing scope range contains line L
    pub fn is_visible_at(&self, name: &str, at_line: u32) -> bool {
        self.bindings.get(name).is_some_and(|bs| {
            bs.iter().any(|b| {
                // Module-level bindings are always visible
                b.scope_depth == 0
                // Same-scope bindings: the usage line is within the scope range
                || (at_line >= b.scope_start && at_line <= b.scope_end)
            })
        })
    }

    /// Check if a name is bound at module scope (depth 0).
    pub fn is_bound_at_module_scope(&self, name: &str) -> bool {
        self.bindings
            .get(name)
            .is_some_and(|bs| bs.iter().any(|b| b.scope_depth == 0))
    }

    /// Get all bindings for a name.
    pub fn get(&self, name: &str) -> Option<&Vec<Binding>> {
        self.bindings.get(name)
    }
}

/// First pass: walk AST, collect all name bindings.
/// scope_start/scope_end = line range of the enclosing scope.
fn collect_bindings(node: Node, source: &str, table: &mut SymbolTable, depth: u32) {
    let scope_start = node.start_position().row as u32 + 1;
    let scope_end = node.end_position().row as u32 + 1;
    collect_bindings_inner(node, source, table, depth, scope_start, scope_end);
}

fn collect_bindings_inner(
    node: Node,
    source: &str,
    table: &mut SymbolTable,
    depth: u32,
    scope_start: u32,
    scope_end: u32,
) {
    match node.kind() {
        // import X, import X as Y, import X.Y.Z
        "import_statement" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "dotted_name" => {
                        // `import os.path` — binds `os`
                        let full = text(child, source);
                        let top = full.split('.').next().unwrap_or(&full);
                        table.add(
                            top.to_string(),
                            BindingKind::Import,
                            line(child),
                            depth,
                            scope_start,
                            scope_end,
                        );
                    }
                    "aliased_import" => {
                        // `import numpy as np` — binds `np`
                        if let Some(alias) = child.child_by_field_name("alias") {
                            table.add(
                                text(alias, source),
                                BindingKind::Import,
                                line(child),
                                depth,
                                scope_start,
                                scope_end,
                            );
                        } else if let Some(name) = child.child_by_field_name("name") {
                            let full = text(name, source);
                            let top = full.split('.').next().unwrap_or(&full);
                            table.add(
                                top.to_string(),
                                BindingKind::Import,
                                line(child),
                                depth,
                                scope_start,
                                scope_end,
                            );
                        }
                    }
                    _ => {}
                }
            }
        }

        // from X import Y, from X import Y as Z
        "import_from_statement" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "dotted_name" => {
                        // Skip the module name (first dotted_name is the source module)
                        let module_node = node.child_by_field_name("module_name");
                        if module_node.map(|m| m.id()) == Some(child.id()) {
                            continue;
                        }
                        table.add(
                            text(child, source),
                            BindingKind::FromImport,
                            line(child),
                            depth,
                            scope_start,
                            scope_end,
                        );
                    }
                    "aliased_import" => {
                        if let Some(alias) = child.child_by_field_name("alias") {
                            table.add(
                                text(alias, source),
                                BindingKind::FromImport,
                                line(child),
                                depth,
                                scope_start,
                                scope_end,
                            );
                        } else if let Some(name) = child.child_by_field_name("name") {
                            table.add(
                                text(name, source),
                                BindingKind::FromImport,
                                line(child),
                                depth,
                                scope_start,
                                scope_end,
                            );
                        }
                    }
                    "wildcard_import" => {
                        // from X import * — can't track individual names
                    }
                    _ => {}
                }
            }
        }

        // X = ..., X += ..., X: type = ...
        "assignment" => {
            if let Some(left) = node.child_by_field_name("left") {
                collect_target_names(
                    left,
                    source,
                    BindingKind::Assignment,
                    table,
                    depth,
                    scope_start,
                    scope_end,
                );
            }
        }
        "augmented_assignment" => {
            if let Some(left) = node.child_by_field_name("left") {
                collect_target_names(
                    left,
                    source,
                    BindingKind::Assignment,
                    table,
                    depth,
                    scope_start,
                    scope_end,
                );
            }
        }

        // def f(X, Y=1, *args, **kwargs):
        "function_definition" => {
            // Bind the function name
            if let Some(name) = node.child_by_field_name("name") {
                table.add(
                    text(name, source),
                    BindingKind::FunctionDef,
                    line(name),
                    depth,
                    scope_start,
                    scope_end,
                );
            }
            // Bind parameters (scope = the function body)
            let fn_start = node.start_position().row as u32 + 1;
            let fn_end = node.end_position().row as u32 + 1;
            if let Some(params) = node.child_by_field_name("parameters") {
                collect_parameters(params, source, table, depth + 1, fn_start, fn_end);
            }
        }

        // class X:
        "class_definition" => {
            if let Some(name) = node.child_by_field_name("name") {
                table.add(
                    text(name, source),
                    BindingKind::ClassDef,
                    line(name),
                    depth,
                    scope_start,
                    scope_end,
                );
            }
        }

        // for X in ...:
        "for_statement" => {
            if let Some(left) = node.child_by_field_name("left") {
                collect_target_names(
                    left,
                    source,
                    BindingKind::ForTarget,
                    table,
                    depth,
                    scope_start,
                    scope_end,
                );
            }
        }

        // async for X in ...:
        // tree-sitter-python uses "for_statement" inside "async" context too

        // with ... as X:
        "with_statement" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "with_clause" || child.kind() == "with_item" {
                    // with_item has alias field
                    collect_with_items(child, source, table, depth, scope_start, scope_end);
                }
            }
        }

        // except E as X:
        "except_clause" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "as_pattern" {
                    // Look for as_pattern_target (same pattern as with_statement)
                    let mut inner = child.walk();
                    for c in child.children(&mut inner) {
                        if c.kind() == "as_pattern_target" {
                            collect_target_names(
                                c,
                                source,
                                BindingKind::ExceptTarget,
                                table,
                                depth,
                                scope_start,
                                scope_end,
                            );
                        }
                    }
                    // Fallback: try field name
                    if let Some(alias) = child.child_by_field_name("alias") {
                        collect_target_names(
                            alias,
                            source,
                            BindingKind::ExceptTarget,
                            table,
                            depth,
                            scope_start,
                            scope_end,
                        );
                    }
                }
            }
        }

        // global X, Y
        "global_statement" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "identifier" {
                    table.add(
                        text(child, source),
                        BindingKind::Global,
                        line(child),
                        depth,
                        scope_start,
                        scope_end,
                    );
                }
            }
        }

        // nonlocal X, Y
        "nonlocal_statement" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "identifier" {
                    table.add(
                        text(child, source),
                        BindingKind::Nonlocal,
                        line(child),
                        depth,
                        scope_start,
                        scope_end,
                    );
                }
            }
        }

        // (X := expr)
        "named_expression" => {
            if let Some(name) = node.child_by_field_name("name") {
                if name.kind() == "identifier" {
                    table.add(
                        text(name, source),
                        BindingKind::NamedExpr,
                        line(name),
                        depth,
                        scope_start,
                        scope_end,
                    );
                }
            }
        }

        // List/dict/set comprehension: [... for X in ...]
        "list_comprehension"
        | "set_comprehension"
        | "dictionary_comprehension"
        | "generator_expression" => {
            collect_comprehension_vars(node, source, table, depth, scope_start, scope_end);
        }

        _ => {}
    }

    // Recurse into children — increase depth and update scope range for function/class bodies
    let (child_depth, child_scope_start, child_scope_end) = match node.kind() {
        "function_definition" | "class_definition" => {
            let ns = node.start_position().row as u32 + 1;
            let ne = node.end_position().row as u32 + 1;
            (depth + 1, ns, ne)
        }
        _ => (depth, scope_start, scope_end),
    };
    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i) {
            collect_bindings_inner(
                child,
                source,
                table,
                child_depth,
                child_scope_start,
                child_scope_end,
            );
        }
    }
}

/// Extract names from assignment targets (handles tuples, lists, starred).
fn collect_target_names(
    node: Node,
    source: &str,
    kind: BindingKind,
    table: &mut SymbolTable,
    depth: u32,
    scope_start: u32,
    scope_end: u32,
) {
    match node.kind() {
        "identifier" => {
            table.add(
                text(node, source),
                kind,
                line(node),
                depth,
                scope_start,
                scope_end,
            );
        }
        // Wrapper nodes that contain an identifier child
        "as_pattern_target" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_target_names(child, source, kind, table, depth, scope_start, scope_end);
            }
        }
        "pattern_list" | "tuple_pattern" | "list_pattern" | "tuple" | "list" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_target_names(child, source, kind, table, depth, scope_start, scope_end);
            }
        }
        "list_splat_pattern" | "starred_expression" => {
            if let Some(child) = node.child(0).or_else(|| node.child(1)) {
                collect_target_names(child, source, kind, table, depth, scope_start, scope_end);
            }
        }
        "attribute" | "subscript" => {
            // x.attr = ... or x[i] = ... — doesn't bind a new name
        }
        _ => {}
    }
}

/// Extract parameter names from function parameters.
fn collect_parameters(
    node: Node,
    source: &str,
    table: &mut SymbolTable,
    depth: u32,
    scope_start: u32,
    scope_end: u32,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "identifier" => {
                table.add(
                    text(child, source),
                    BindingKind::Parameter,
                    line(child),
                    depth,
                    scope_start,
                    scope_end,
                );
            }
            "default_parameter" | "typed_default_parameter" | "typed_parameter" => {
                // Try field "name" first
                if let Some(name) = child.child_by_field_name("name") {
                    if name.kind() == "identifier" {
                        table.add(
                            text(name, source),
                            BindingKind::Parameter,
                            line(name),
                            depth,
                            scope_start,
                            scope_end,
                        );
                    }
                } else {
                    // Fallback: find identifier — may be nested in splat patterns
                    // e.g. typed_parameter > dictionary_splat_pattern > identifier
                    let found = find_first_identifier(child, source);
                    if let Some((name_str, name_line)) = found {
                        table.add(
                            name_str,
                            BindingKind::Parameter,
                            name_line,
                            depth,
                            scope_start,
                            scope_end,
                        );
                    }
                }
            }
            "list_splat_pattern" | "dictionary_splat_pattern" => {
                // *args, **kwargs
                let mut inner = child.walk();
                for c in child.children(&mut inner) {
                    if c.kind() == "identifier" {
                        table.add(
                            text(c, source),
                            BindingKind::Parameter,
                            line(c),
                            depth,
                            scope_start,
                            scope_end,
                        );
                    }
                }
            }
            "tuple_pattern" | "list_pattern" => {
                collect_target_names(
                    child,
                    source,
                    BindingKind::Parameter,
                    table,
                    depth,
                    scope_start,
                    scope_end,
                );
            }
            _ => {}
        }
    }
}

fn collect_with_items(
    node: Node,
    source: &str,
    table: &mut SymbolTable,
    depth: u32,
    scope_start: u32,
    scope_end: u32,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "with_item" | "with_clause" => {
                collect_with_items(child, source, table, depth, scope_start, scope_end);
            }
            "as_pattern" => {
                // as_pattern has as_pattern_target child containing the identifier
                let mut inner = child.walk();
                for c in child.children(&mut inner) {
                    if c.kind() == "as_pattern_target" {
                        collect_target_names(
                            c,
                            source,
                            BindingKind::WithTarget,
                            table,
                            depth,
                            scope_start,
                            scope_end,
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

fn collect_comprehension_vars(
    node: Node,
    source: &str,
    table: &mut SymbolTable,
    depth: u32,
    scope_start: u32,
    scope_end: u32,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "for_in_clause" {
            if let Some(left) = child.child_by_field_name("left") {
                collect_target_names(
                    left,
                    source,
                    BindingKind::ComprehensionVar,
                    table,
                    depth,
                    scope_start,
                    scope_end,
                );
            }
        }
        // Recurse for nested comprehensions
        if child.kind() == "for_in_clause" || child.kind() == "if_clause" {
            collect_comprehension_vars(child, source, table, depth, scope_start, scope_end);
        }
    }
}

/// Recursively find the first identifier in a node tree (DFS).
fn find_first_identifier(node: Node, source: &str) -> Option<(String, u32)> {
    if node.kind() == "identifier" {
        return Some((text(node, source), line(node)));
    }
    let count = node.child_count();
    for i in 0..count {
        if let Some(child) = node.child(i) {
            if let Some(result) = find_first_identifier(child, source) {
                return Some(result);
            }
        }
    }
    None
}

fn text(node: Node, source: &str) -> String {
    source[node.start_byte()..node.end_byte()].to_string()
}

fn line(node: Node) -> u32 {
    node.start_position().row as u32 + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_python;

    fn build(source: &str) -> SymbolTable {
        let tree = parse_python(source).unwrap();
        SymbolTable::build(&tree, source)
    }

    #[test]
    fn test_import_binding() {
        let st = build("import os\nimport numpy as np");
        assert!(st.is_import("os"));
        assert!(st.is_import("np"));
        assert!(!st.is_import("numpy")); // aliased away
    }

    #[test]
    fn test_from_import_binding() {
        let st = build("from os.path import join, exists");
        assert!(st.is_import("join"));
        assert!(st.is_import("exists"));
    }

    #[test]
    fn test_assignment_binding() {
        let st = build("x = 1\ny, z = 2, 3");
        assert!(st.is_local("x"));
        assert!(st.is_local("y"));
        assert!(st.is_local("z"));
        assert!(!st.is_import("x"));
    }

    #[test]
    fn test_function_params() {
        let st = build("def foo(a, b, *args, **kwargs):\n    pass");
        assert!(st.is_local("foo"));
        assert!(st.is_local("a"));
        assert!(st.is_local("b"));
        assert!(st.is_local("args"));
        assert!(st.is_local("kwargs"));
    }

    #[test]
    fn test_for_target() {
        let st = build("for item in items:\n    pass");
        assert!(st.is_local("item"));
    }

    #[test]
    fn test_with_target() {
        let st = build("with open('f') as fp:\n    pass");
        assert!(st.is_local("fp"));
    }

    #[test]
    fn test_class_def() {
        let st = build("class MyClass:\n    pass");
        assert!(st.is_local("MyClass"));
    }

    #[test]
    fn test_comprehension_var() {
        let st = build("result = [x for x in range(10)]");
        assert!(st.is_local("x"));
        assert!(st.is_local("result"));
    }

    #[test]
    fn test_import_vs_local() {
        let st = build("import os\npath = os.path.join('/a', '/b')");
        assert!(st.is_import("os"));
        assert!(st.is_local("path"));
        // path is local, not an import
        assert!(!st.is_import("path"));
    }

    #[test]
    fn test_walrus() {
        let st = build("if (n := len(data)) > 0:\n    pass");
        assert!(st.is_local("n"));
    }

    #[test]
    fn test_tuple_unpack() {
        let st = build("a, b, c = 1, 2, 3");
        assert!(st.is_local("a"));
        assert!(st.is_local("b"));
        assert!(st.is_local("c"));
    }

    #[test]
    fn test_typed_params() {
        let st = build("def process(self, task: Task, count: int = 0):\n    pass");
        assert!(st.is_local("self"));
        assert!(st.is_local("task"));
        assert!(st.is_local("count"));
        assert!(st.is_local("process"));
    }

    #[test]
    fn test_static_method_params() {
        let st = build("@staticmethod\ndef resolve(locales: dict, locale: str) -> str:\n    pass");
        assert!(st.is_local("locales"));
        assert!(st.is_local("locale"));
    }
}
