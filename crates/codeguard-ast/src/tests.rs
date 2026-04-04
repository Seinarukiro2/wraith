use crate::{extract_file_info, parse_python, LineIndex};
use std::path::PathBuf;

fn parse_and_extract(source: &str) -> crate::extract::FileInfo {
    let tree = parse_python(source).unwrap();
    extract_file_info(&tree, source, &PathBuf::from("test.py"))
}

#[test]
fn test_simple_import() {
    let info = parse_and_extract("import os");
    assert_eq!(info.imports.len(), 1);
    assert_eq!(info.imports[0].module, "os");
    assert!(!info.imports[0].is_from);
}

#[test]
fn test_from_import() {
    let info = parse_and_extract("from os.path import join, exists");
    assert_eq!(info.imports.len(), 1);
    assert_eq!(info.imports[0].module, "os");
    assert!(info.imports[0].is_from);
    assert_eq!(info.imports[0].names.len(), 2);
    assert_eq!(info.imports[0].names[0].name, "join");
    assert_eq!(info.imports[0].names[1].name, "exists");
}

#[test]
fn test_aliased_import() {
    let info = parse_and_extract("import numpy as np");
    assert_eq!(info.imports.len(), 1);
    assert_eq!(info.imports[0].names[0].name, "numpy");
    assert_eq!(info.imports[0].names[0].alias, Some("np".to_string()));
}

#[test]
fn test_relative_import_skipped() {
    let info = parse_and_extract("from . import utils");
    assert_eq!(info.imports.len(), 0);
}

#[test]
fn test_simple_call() {
    let info = parse_and_extract("print('hello')");
    assert_eq!(info.calls.len(), 1);
    assert_eq!(info.calls[0].function, "print");
    assert!(info.calls[0].receiver.is_none());
    assert_eq!(info.calls[0].positional_count, 1);
}

#[test]
fn test_method_call() {
    let info = parse_and_extract("os.path.join('/tmp', 'file')");
    assert_eq!(info.calls.len(), 1);
    assert_eq!(info.calls[0].function, "join");
    assert_eq!(info.calls[0].receiver, Some("os.path".to_string()));
    assert_eq!(info.calls[0].full_name, "os.path.join");
}

#[test]
fn test_keyword_args() {
    let info = parse_and_extract("json.dumps(data, indent=2, sort_keys=True)");
    assert_eq!(info.calls.len(), 1);
    assert_eq!(info.calls[0].keyword_args.len(), 2);
    assert_eq!(info.calls[0].keyword_args[0].name, "indent");
    assert_eq!(info.calls[0].keyword_args[1].name, "sort_keys");
    assert_eq!(info.calls[0].positional_count, 1);
}

#[test]
fn test_call_in_assignment() {
    let info = parse_and_extract("result = os.path.join('/tmp', 'file')");
    assert!(info.calls.len() >= 1);
    let join_calls: Vec<_> = info.calls.iter().filter(|c| c.function == "join").collect();
    assert_eq!(join_calls.len(), 1);
    assert_eq!(join_calls[0].receiver, Some("os.path".to_string()));
}

#[test]
fn test_string_assignment() {
    let info = parse_and_extract(r#"API_KEY = "secret_value""#);
    assert_eq!(info.assignments.len(), 1);
    assert_eq!(info.assignments[0].target, "API_KEY");
    assert!(info.assignments[0].value_is_string);
}

#[test]
fn test_non_string_assignment() {
    let info = parse_and_extract("count = 42");
    assert_eq!(info.assignments.len(), 1);
    assert_eq!(info.assignments[0].target, "count");
    assert!(!info.assignments[0].value_is_string);
}

#[test]
fn test_comment_extraction() {
    let info = parse_and_extract("# This is a comment\nx = 1");
    assert_eq!(info.comments.len(), 1);
    assert!(info.comments[0].text.contains("This is a comment"));
}

#[test]
fn test_multiple_imports() {
    let info = parse_and_extract("import os\nimport sys\nimport json");
    assert_eq!(info.imports.len(), 3);
}

#[test]
fn test_line_index() {
    let source = "line1\nline2\nline3";
    let idx = LineIndex::new(source);
    assert_eq!(idx.line_col(0), (1, 0));   // start of line 1
    assert_eq!(idx.line_col(6), (2, 0));   // start of line 2
    assert_eq!(idx.line_col(8), (2, 2));   // 'n' in line2
    assert_eq!(idx.line_col(12), (3, 0));  // start of line 3
}

#[test]
fn test_nested_call_in_assignment() {
    let info = parse_and_extract("x = foo.bar(baz.qux(1))");
    // Should find both foo.bar() and baz.qux()
    assert!(info.calls.len() >= 2);
}

#[test]
fn test_star_import() {
    let info = parse_and_extract("from os.path import *");
    assert_eq!(info.imports.len(), 1);
    assert_eq!(info.imports[0].names[0].name, "*");
}

#[test]
fn test_kwarg_spans() {
    let source = "foo(bar=1, baz=2)";
    let info = parse_and_extract(source);
    assert_eq!(info.calls[0].keyword_args.len(), 2);
    // Verify spans point to the kwarg name, not the value
    let bar_span = &info.calls[0].keyword_args[0].name_span;
    assert_eq!(bar_span.start_col, 4); // 'bar' starts at col 4
    let baz_span = &info.calls[0].keyword_args[1].name_span;
    assert_eq!(baz_span.start_col, 11); // 'baz' starts at col 11
}
