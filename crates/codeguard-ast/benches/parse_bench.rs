use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use std::path::PathBuf;

fn collect_py_files(dir: &str) -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();
    fn walk(dir: &std::path::Path, files: &mut Vec<(PathBuf, String)>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    if !name.starts_with('.') && name != "__pycache__" && name != "node_modules" {
                        walk(&path, files);
                    }
                } else if path.extension().is_some_and(|e| e == "py") {
                    if let Ok(source) = std::fs::read_to_string(&path) {
                        files.push((path, source));
                    }
                }
            }
        }
    }
    walk(std::path::Path::new(dir), &mut files);
    files
}

fn bench_parse(c: &mut Criterion) {
    // Use FastAPI if available, else use a small sample
    let fastapi_dir = "/tmp/fastapi";
    let files = if std::path::Path::new(fastapi_dir).exists() {
        collect_py_files(fastapi_dir)
    } else {
        // Fallback: use our own test fixtures
        collect_py_files("tests/fixtures")
    };

    let file_count = files.len();
    if file_count == 0 {
        eprintln!("No Python files found for benchmarking");
        return;
    }

    let mut group = c.benchmark_group("parse");

    group.bench_function(BenchmarkId::new("tree-sitter parse", file_count), |b| {
        b.iter(|| {
            for (_, source) in &files {
                black_box(codeguard_ast::parse_python(source));
            }
        });
    });

    // Pre-parse for extraction bench
    let parsed: Vec<_> = files
        .iter()
        .filter_map(|(path, source)| {
            codeguard_ast::parse_python(source).map(|tree| (path.clone(), source.clone(), tree))
        })
        .collect();

    group.bench_function(BenchmarkId::new("extract FileInfo", file_count), |b| {
        b.iter(|| {
            for (path, source, tree) in &parsed {
                black_box(codeguard_ast::extract_file_info(tree, source, path));
            }
        });
    });

    group.bench_function(BenchmarkId::new("build SymbolTable", file_count), |b| {
        b.iter(|| {
            for (_, source, tree) in &parsed {
                black_box(codeguard_ast::SymbolTable::build(tree, source));
            }
        });
    });

    group.bench_function(BenchmarkId::new("vibe lint", file_count), |b| {
        b.iter(|| {
            for (path, source, tree) in &parsed {
                black_box(codeguard_vibe::lint_vibe(tree, source, path));
            }
        });
    });

    group.bench_function(
        BenchmarkId::new("full pipeline (parse+extract+symtable+vibe)", file_count),
        |b| {
            b.iter(|| {
                for (path, source) in &files {
                    let tree = codeguard_ast::parse_python(source).unwrap();
                    let _info = codeguard_ast::extract_file_info(&tree, source, path);
                    let _sym = codeguard_ast::SymbolTable::build(&tree, source);
                    let _diags = codeguard_vibe::lint_vibe(&tree, source, path);
                    black_box(());
                }
            });
        },
    );

    group.finish();
}

criterion_group!(benches, bench_parse);
criterion_main!(benches);
