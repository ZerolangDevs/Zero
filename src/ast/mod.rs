//! Abstract syntax tree for the Zero language (v0.2).

mod expr;
mod stmt;

pub use expr::{BinOp, Expr, UnOp};
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

/// A declared type for parameters and return values. `None` in a function
/// signature means dynamic (the default; the explicit type `any` is
/// normalised to `None` by the parser).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZType {
    Int,
    Str,
    Bool,
}

impl ZType {
    pub fn from_name(s: &str) -> Option<ZType> {
        match s {
            "int" => Some(ZType::Int),
            "string" => Some(ZType::Str),
            "bool" => Some(ZType::Bool),
            _ => None,
        }
    }

    /// Diagnostic name.
    pub fn name(&self) -> &'static str {
        match self {
            ZType::Int => "int",
            ZType::Str => "string",
            ZType::Bool => "bool",
        }
    }
}

/// A function parameter with an optional type annotation: `x<int>`.
#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: Option<ZType>,
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub params: Vec<Param>,
    /// Declared return type (None = dynamic).
    pub ret: Option<ZType>,
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
