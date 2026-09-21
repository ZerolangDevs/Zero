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
    Func,
    Return,
    Import,
    Scope,
    HighLevel,
    LowLevel,
    Const,
    True,
    False,
    And,
    Or,

    // Symbols
    LBrace,
    RBrace,
    LParen,
    RParen,
    Semi,
    Comma,
    Colon,
    Dot,
    Lt,
    Gt,
    Equal,

    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Bang,
    Arrow,
    EqEq,
    BangEq,
    Le,
    Ge,

    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokKind,
    pub span: Span,
}
