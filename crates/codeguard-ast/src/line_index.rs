use codeguard_core::Span;
use std::path::Path;

pub struct LineIndex {
    line_starts: Vec<usize>,
}

impl LineIndex {
    pub fn new(source: &str) -> Self {
        let mut line_starts = vec![0];
        for (i, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(i + 1);
            }
        }
        Self { line_starts }
    }

    pub fn line_col(&self, byte_offset: usize) -> (u32, u32) {
        let line = self
            .line_starts
            .partition_point(|&start| start <= byte_offset)
            .saturating_sub(1);
        let col = byte_offset - self.line_starts[line];
        (line as u32 + 1, col as u32)
    }

    pub fn span_from_node(&self, node: tree_sitter::Node, path: &Path) -> Span {
        let (start_line, start_col) = self.line_col(node.start_byte());
        let (end_line, end_col) = self.line_col(node.end_byte());
        Span::new(path.to_path_buf(), start_line, start_col, end_line, end_col)
    }

    pub fn byte_offset(&self, line: u32, col: u32) -> usize {
        let line_idx = (line as usize).saturating_sub(1);
        if line_idx < self.line_starts.len() {
            self.line_starts[line_idx] + col as usize
        } else {
            self.line_starts.last().copied().unwrap_or(0) + col as usize
        }
    }
}
