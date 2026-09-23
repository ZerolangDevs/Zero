//! Token definitions for the Zero lexer.

use crate::diag::Span;

#[derive(Debug, Clone, PartialEq)]
pub enum TokKind {
    // Literals / identifiers
    Ident(String),
    Str(String),
    Int(i64),
    Float(f64),

    // Keywords
    Fn,
    Func,
    Return,
    Break,
    Continue,
    If,
    Else,
    ElseIf,
    While,
    For,
    Each,
    In,
    Switch,
    Case,
    Default,
    Try,
    Catch,
    Import,
    Scope,
    HighLevel,
    LowLevel,
    Const,
    True,
    False,
    Null,
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
    DotDot,
    DotDotEq,
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
    PlusEq,
    MinusEq,
    StarEq,
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
