use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::path::PathBuf;

#[pyfunction]
#[pyo3(signature = (path,))]
fn check(py: Python<'_>, path: String) -> PyResult<Vec<PyObject>> {
    let source = std::fs::read_to_string(&path)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyIOError, _>(e.to_string()))?;
    check_source_impl(py, &source, &path)
}

#[pyfunction]
#[pyo3(signature = (source, filename = "<stdin>"))]
fn check_source(py: Python<'_>, source: &str, filename: &str) -> PyResult<Vec<PyObject>> {
    check_source_impl(py, source, filename)
}

#[pyfunction]
#[pyo3(signature = (source, filename = "<stdin>"))]
fn fix(_py: Python<'_>, source: &str, filename: &str) -> PyResult<String> {
    let path = PathBuf::from(filename);
    let tree = codeguard_ast::parse_python(source)
        .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Failed to parse Python source"))?;

    let mut diagnostics = Vec::new();
    diagnostics.extend(codeguard_vibe::lint_vibe(&tree, source, &path));

    let fixes: Vec<_> = diagnostics
        .iter()
        .filter_map(|d| d.fix.as_ref())
        .collect();

    if fixes.is_empty() {
        return Ok(source.to_string());
    }

    let line_index = codeguard_ast::LineIndex::new(source);
    let mut sorted = fixes;
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

    Ok(result)
}

fn check_source_impl(py: Python<'_>, source: &str, filename: &str) -> PyResult<Vec<PyObject>> {
    let path = PathBuf::from(filename);
    let tree = codeguard_ast::parse_python(source)
        .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Failed to parse Python source"))?;

    let mut diagnostics = Vec::new();
    diagnostics.extend(codeguard_vibe::lint_vibe(&tree, source, &path));

    let results: Vec<PyObject> = diagnostics
        .iter()
        .map(|d| {
            let dict = PyDict::new_bound(py);
            dict.set_item("code", &d.code.0).unwrap();
            dict.set_item("severity", format!("{}", d.severity)).unwrap();
            dict.set_item("line", d.span.start_line).unwrap();
            dict.set_item("col", d.span.start_col).unwrap();
            dict.set_item("message", &d.message).unwrap();
            dict.set_item("suggestion", d.suggestion.as_deref()).unwrap();
            dict.set_item("fixable", d.fix.is_some()).unwrap();
            dict.into_py(py)
        })
        .collect();

    Ok(results)
}

#[pymodule]
fn codeguard(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(check, m)?)?;
    m.add_function(wrap_pyfunction!(check_source, m)?)?;
    m.add_function(wrap_pyfunction!(fix, m)?)?;
    Ok(())
}
