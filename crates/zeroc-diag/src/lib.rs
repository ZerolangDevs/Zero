//! Compiler diagnostics: source spans and error reporting.

mod error;
mod span;

pub use error::{line_col, render_error, CompileError};
pub use span::Span;
