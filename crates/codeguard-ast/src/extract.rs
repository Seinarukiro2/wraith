use crate::line_index::LineIndex;
use codeguard_core::Span;
use std::path::Path;
use tree_sitter::{Node, Tree};

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub imports: Vec<ImportInfo>,
    pub calls: Vec<CallInfo>,
    pub assignments: Vec<AssignmentInfo>,
    pub comments: Vec<CommentInfo>,
    pub decorators: Vec<DecoratorInfo>,
}

#[derive(Debug, Clone)]
pub struct ImportInfo {
    pub module: String,
    pub names: Vec<ImportedName>,
    pub is_from: bool,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ImportedName {
    pub name: String,
    pub alias: Option<String>,
}

#[derive(Debug, Clone)]
pub struct KeywordArg {
    pub name: String,
    pub name_span: Span,
}

#[derive(Debug, Clone)]
pub struct CallInfo {
    pub receiver: Option<String>,
    pub function: String,
    pub full_name: String,
    pub function_span: Span,
    pub keyword_args: Vec<KeywordArg>,
    pub positional_count: usize,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct AssignmentInfo {
    pub target: String,
    pub value: Option<String>,
    pub value_is_string: bool,
    pub span: Span,
    pub value_span: Option<Span>,
}

#[derive(Debug, Clone)]
pub struct CommentInfo {
    pub text: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct DecoratorInfo {
    pub name: String,
    pub arguments: Vec<String>,
    pub span: Span,
    pub function_span: Span,
    pub function_decorators: Vec<String>,
}

pub fn extract_file_info(tree: &Tree, source: &str, path: &Path) -> FileInfo {
    let line_index = LineIndex::new(source);
    let root = tree.root_node();

    let mut info = FileInfo {
        imports: Vec::new(),
        calls: Vec::new(),
        assignments: Vec::new(),
        comments: Vec::new(),
        decorators: Vec::new(),
    };

    extract_recursive(root, source, path, &line_index, &mut info);
    info
}

fn extract_recursive(
    node: Node,
    source: &str,
    path: &Path,
    idx: &LineIndex,
    info: &mut FileInfo,
) {
    match node.kind() {
        "import_statement" => {
            extract_import(node, source, path, idx, info);
        }
        "import_from_statement" => {
            extract_from_import(node, source, path, idx, info);
        }
        "assignment" => {
            extract_assignment(node, source, path, idx, info);
        }
        "call" => {
            extract_call(node, source, path, idx, info);
        }
        "comment" => {
            let text = node_text(node, source);
            info.comments.push(CommentInfo {
                text,
                span: idx.span_from_node(node, path),
            });
        }
        "decorated_definition" => {
            extract_decorated_definition(node, source, path, idx, info);
        }
        _ => {}
    }

    let child_count = node.child_count();
    for i in 0..child_count {
        if let Some(child) = node.child(i) {
            extract_recursive(child, source, path, idx, info);
        }
    }
}

fn extract_import(node: Node, source: &str, path: &Path, idx: &LineIndex, info: &mut FileInfo) {
    // import X, import X as Y, import X.Y.Z
    let mut cursor = node.walk();
    let mut names = Vec::new();

    for child in node.children(&mut cursor) {
        match child.kind() {
            "dotted_name" => {
                let module = node_text(child, source);
                names.push(ImportedName {
                    name: module,
                    alias: None,
                });
            }
            "aliased_import" => {
                let name_node = child.child_by_field_name("name");
                let alias_node = child.child_by_field_name("alias");
                if let Some(name_node) = name_node {
                    names.push(ImportedName {
                        name: node_text(name_node, source),
                        alias: alias_node.map(|n| node_text(n, source)),
                    });
                }
            }
            _ => {}
        }
    }

    for name in &names {
        let top_module = name
            .name
            .split('.')
            .next()
            .unwrap_or(&name.name)
            .to_string();
        info.imports.push(ImportInfo {
            module: top_module,
            names: vec![name.clone()],
            is_from: false,
            span: idx.span_from_node(node, path),
        });
    }
}

fn extract_from_import(
    node: Node,
    source: &str,
    path: &Path,
    idx: &LineIndex,
    info: &mut FileInfo,
) {
    let module_node = node.child_by_field_name("module_name");
    let module = module_node.map(|n| node_text(n, source)).unwrap_or_default();

    // Relative imports (from . import X) — skip package validation
    if module.is_empty() || module.starts_with('.') {
        return;
    }

    let mut names = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "dotted_name" if Some(child.id()) != module_node.map(|n| n.id()) => {
                names.push(ImportedName {
                    name: node_text(child, source),
                    alias: None,
                });
            }
            "aliased_import" => {
                let name_node = child.child_by_field_name("name");
                let alias_node = child.child_by_field_name("alias");
                if let Some(name_node) = name_node {
                    names.push(ImportedName {
                        name: node_text(name_node, source),
                        alias: alias_node.map(|n| node_text(n, source)),
                    });
                }
            }
            "wildcard_import" => {
                names.push(ImportedName {
                    name: "*".to_string(),
                    alias: None,
                });
            }
            _ => {}
        }
    }

    let top_module = module.split('.').next().unwrap_or(&module).to_string();

    info.imports.push(ImportInfo {
        module: top_module,
        names,
        is_from: true,
        span: idx.span_from_node(node, path),
    });
}

fn extract_call(node: Node, source: &str, path: &Path, idx: &LineIndex, info: &mut FileInfo) {
    let func_node = node.child_by_field_name("function");
    let args_node = node.child_by_field_name("arguments");

    let (receiver, function, full_name, func_span) = match func_node {
        Some(f) if f.kind() == "attribute" => {
            let obj = f.child_by_field_name("object").map(|n| node_text(n, source));
            let attr_node = f.child_by_field_name("attribute");
            let attr = attr_node
                .map(|n| node_text(n, source))
                .unwrap_or_default();
            let full = node_text(f, source);
            let aspan = attr_node
                .map(|n| idx.span_from_node(n, path))
                .unwrap_or_else(|| idx.span_from_node(f, path));
            (obj, attr, full, aspan)
        }
        Some(f) if f.kind() == "identifier" => {
            let name = node_text(f, source);
            let sp = idx.span_from_node(f, path);
            (None, name.clone(), name, sp)
        }
        _ => return,
    };

    let mut keyword_args = Vec::new();
    let mut positional_count = 0;

    if let Some(args) = args_node {
        let mut cursor = args.walk();
        for child in args.children(&mut cursor) {
            match child.kind() {
                "keyword_argument" => {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        keyword_args.push(KeywordArg {
                            name: node_text(name_node, source),
                            name_span: idx.span_from_node(name_node, path),
                        });
                    }
                }
                "(" | ")" | "," => {}
                _ => {
                    positional_count += 1;
                }
            }
        }
    }

    info.calls.push(CallInfo {
        receiver,
        function,
        full_name,
        function_span: func_span,
        keyword_args,
        positional_count,
        span: idx.span_from_node(node, path),
    });
}

fn extract_assignment(
    node: Node,
    source: &str,
    path: &Path,
    idx: &LineIndex,
    info: &mut FileInfo,
) {
    let left = node.child_by_field_name("left");
    let right = node.child_by_field_name("right");

    let target = match left {
        Some(n) => node_text(n, source),
        None => return,
    };

    let (value, value_is_string, value_span) = match right {
        Some(n) => {
            let is_str = n.kind() == "string" || n.kind() == "concatenated_string";
            (
                Some(node_text(n, source)),
                is_str,
                Some(idx.span_from_node(n, path)),
            )
        }
        None => (None, false, None),
    };

    info.assignments.push(AssignmentInfo {
        target,
        value,
        value_is_string,
        span: idx.span_from_node(node, path),
        value_span,
    });
}

fn extract_decorated_definition(
    node: Node,
    source: &str,
    path: &Path,
    idx: &LineIndex,
    info: &mut FileInfo,
) {
    let mut decorators_names = Vec::new();
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        if child.kind() == "decorator" {
            let dec_text = node_text(child, source);
            let dec_name = dec_text.trim_start_matches('@').trim();
            decorators_names.push(dec_name.to_string());

            // Extract route decorators with their arguments
            if let Some(call_node) = child.child(1) {
                if call_node.kind() == "call" {
                    let func_text = call_node
                        .child_by_field_name("function")
                        .map(|n| node_text(n, source))
                        .unwrap_or_default();

                    let mut args = Vec::new();
                    if let Some(args_node) = call_node.child_by_field_name("arguments") {
                        let mut ac = args_node.walk();
                        for arg_child in args_node.children(&mut ac) {
                            if arg_child.kind() == "string" {
                                let s = node_text(arg_child, source);
                                let unquoted = s
                                    .trim_start_matches(|c| c == '\'' || c == '"')
                                    .trim_end_matches(|c| c == '\'' || c == '"');
                                args.push(unquoted.to_string());
                            }
                        }
                    }

                    // Find the function def that follows
                    let func_def = node.children(&mut node.walk()).find(|c| {
                        c.kind() == "function_definition"
                    });
                    let function_span = func_def
                        .map(|f| idx.span_from_node(f, path))
                        .unwrap_or_else(|| idx.span_from_node(node, path));

                    info.decorators.push(DecoratorInfo {
                        name: func_text,
                        arguments: args,
                        span: idx.span_from_node(child, path),
                        function_span,
                        function_decorators: decorators_names.clone(),
                    });
                }
            }
        }
    }
}

fn node_text(node: Node, source: &str) -> String {
    source[node.start_byte()..node.end_byte()].to_string()
}
