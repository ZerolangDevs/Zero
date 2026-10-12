//! Expressions in the Zero language.

use crate::ast::ZType;
use crate::diag::Span;

/// Binary operators, ordered by precedence in the parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    /// Path join: `path1 +<path> path2` (requires `import sys`).
    PathJoin,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

/// Unary (prefix) operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    /// Arithmetic negation: `-x`
    Neg,
    /// Logical negation: `!x`
    Not,
}

#[derive(Debug, Clone)]
pub enum Expr {
    Int(i64, Span),
    Float(f64, Span),
    Str(String, Span),
    Bool(bool, Span),
    /// The `NULL` literal.
    Nil(Span),
    Ident(String, Span),
    Call {
        callee: String,
        args: Vec<Expr>,
        span: Span,
    },
    /// Type conversion: `type_to<int>(expr)`.
    TypeTo {
        ty: ZType,
        value: Box<Expr>,
        span: Span,
    },
    /// Network call: `net.get(...)`, `net.connect(...)`, ... .
    /// Requires `import network`.
    NetCall {
        member: String,
        args: Vec<Expr>,
        span: Span,
    },
    /// Window operation: `win.update()`, `win.add.button(...)`,
    /// `win.<id>.del()` / `win.<id>.update(...)`. Requires `import win`.
    WinOp {
        win: String,
        path: Vec<String>,
        args: Vec<(String, Expr)>,
        span: Span,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        span: Span,
    },
    Unary {
        op: UnOp,
        operand: Box<Expr>,
        span: Span,
    },
}

impl Expr {
    pub(crate) fn span(&self) -> Span {
        match self {
            Expr::Int(_, s)
            | Expr::Float(_, s)
            | Expr::Str(_, s)
            | Expr::Bool(_, s)
            | Expr::Nil(s)
            | Expr::Ident(_, s)
            | Expr::Call { span: s, .. }
            | Expr::TypeTo { span: s, .. }
            | Expr::NetCall { span: s, .. }
            | Expr::WinOp { span: s, .. }
            | Expr::Binary { span: s, .. }
            | Expr::Unary { span: s, .. } => *s,
        }
    }
}
