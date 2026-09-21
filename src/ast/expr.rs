//! Expressions in the Zero language.

use crate::diag::Span;

/// Binary operators, ordered by precedence in the parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
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
    Str(String, Span),
    Bool(bool, Span),
    Ident(String, Span),
    Call {
        callee: String,
        args: Vec<Expr>,
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
            | Expr::Str(_, s)
            | Expr::Bool(_, s)
            | Expr::Ident(_, s)
            | Expr::Call { span: s, .. }
            | Expr::Binary { span: s, .. }
            | Expr::Unary { span: s, .. } => *s,
        }
    }
}
