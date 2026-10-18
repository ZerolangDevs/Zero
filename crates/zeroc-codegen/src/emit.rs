//! Low-level emission helpers for Rust code generation.

use crate::ast::Stmt;

pub(crate) const INDENT: &str = "    ";

pub(crate) fn ends_with_return(stmts: &[Stmt]) -> bool {
    matches!(stmts.last(), Some(Stmt::Return { .. }))
}

/// Format a `call_rust` snippet as a Rust block expression.
pub(crate) fn rust_block(raw: &str, indent: usize) -> String {
    if raw.contains('\n') {
        let pad = INDENT.repeat(indent + 1);
        let indented: Vec<String> = raw.lines().map(|l| format!("{pad}{l}")).collect();
        format!("{{\n{}\n{}}}", indented.join("\n"), INDENT.repeat(indent))
    } else {
        format!("{{ {raw} }}")
    }
}

/// Escape a Zero string as a Rust string literal.
pub(crate) fn escape_rust_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
