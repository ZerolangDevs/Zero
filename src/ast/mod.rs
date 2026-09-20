//! Abstract syntax tree for the Zero language (v0.2).

mod expr;
mod stmt;

pub use expr::Expr;
pub use stmt::Stmt;

use crate::diag::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    /// Zero-level code: normal language statements.
    HighLevel,
    /// Raw Rust code, pasted into the generated output verbatim.
    LowLevel,
}

#[derive(Debug, Clone)]
pub struct Program {
    pub items: Vec<Item>,
}

#[derive(Debug, Clone)]
pub enum Item {
    Import(Import),
    Function(Function),
}

#[derive(Debug, Clone)]
pub struct Import {
    /// Header name as written: either a bare name (`stdio`) or a path
    /// (`headers/helpers`, `extra.rs`).
    pub name: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    /// Un-typed parameters: any parameter is a dynamic `ZVal`.
    pub params: Vec<String>,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Block {
    /// A normal highlevel block: a list of statements.
    Stmts(Vec<Stmt>),
    /// Raw text inside a `scope lowlevel { ... }` block.
    Raw(String),
}

impl Program {
    pub fn has_main(&self) -> bool {
        self.items.iter().any(|item| match item {
            Item::Function(f) => f.name == "main",
            _ => false,
        })
    }

    pub fn functions(&self) -> impl Iterator<Item = &Function> {
        self.items.iter().filter_map(|item| match item {
            Item::Function(f) => Some(f),
            _ => None,
        })
    }
}
