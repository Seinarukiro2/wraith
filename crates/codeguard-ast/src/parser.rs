use once_cell::sync::Lazy;
use tree_sitter::{Language, Parser, Tree};

static PYTHON_LANGUAGE: Lazy<Language> = Lazy::new(|| tree_sitter_python::LANGUAGE.into());

pub fn parse_python(source: &str) -> Option<Tree> {
    let mut parser = Parser::new();
    parser
        .set_language(&PYTHON_LANGUAGE)
        .expect("failed to load Python grammar");
    parser.parse(source, None)
}
