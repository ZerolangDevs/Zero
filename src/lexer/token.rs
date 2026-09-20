//! Token definitions for the Zero lexer.

use crate::diag::Span;

#[derive(Debug, Clone, PartialEq)]
pub enum TokKind {
    // Literals / identifiers
    Ident(String),
    Str(String),
    Int(i64),

    // Keywords
    Fn,
    Return,
    Import,
    Scope,
    HighLevel,
    LowLevel,
    Const,

    // Symbols
    LBrace,
    RBrace,
    LParen,
    RParen,
    Semi,
    Comma,
    Dot,
    Lt,
    Gt,
    Equal,

    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokKind,
    pub span: Span,
}
