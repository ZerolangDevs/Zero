//! Compiler error type and source rendering.

use crate::diag::Span;

/// A compiler error carrying an optional source span.
#[derive(Debug)]
pub struct CompileError {
    pub message: String,
    pub span: Option<Span>,
    pub file: String,
}

impl CompileError {
    pub fn new(message: impl Into<String>, file: impl Into<String>) -> Self {
        CompileError {
            message: message.into(),
            span: None,
            file: file.into(),
        }
    }

    pub fn at(message: impl Into<String>, file: impl Into<String>, span: Span) -> Self {
        CompileError {
            message: message.into(),
            span: Some(span),
            file: file.into(),
        }
    }
}

/// Compute the 1-based line and column for a byte offset.
pub fn line_col(src: &str, offset: usize) -> (u32, u32) {
    let offset = offset.min(src.len());
    let mut line = 1u32;
    let mut col = 1u32;
    for (i, ch) in src.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Render an error with a source snippet and a caret, for the CLI.
pub fn render_error(err: &CompileError, src: &str) -> String {
    let mut out = format!("error: {}", err.message);
    if let Some(span) = err.span {
        let (line, col) = line_col(src, span.start);
        out.push_str(&format!("\n  --> {}:{}:{}", err.file, line, col));
        let line_text = src.lines().nth((line - 1) as usize).unwrap_or("");
        out.push_str(&format!("\n   | {line_text}"));
        let caret_pad = " ".repeat(col.saturating_sub(1) as usize);
        out.push_str(&format!("\n   | {caret_pad}^"));
    }
    out
}
