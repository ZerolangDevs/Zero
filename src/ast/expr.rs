//! Expressions in the Zero language.

use crate::diag::Span;

#[derive(Debug, Clone)]
pub enum Expr {
    Int(i64, Span),
    Str(String, Span),
    Ident(String, Span),
    Call {
        callee: String,
        args: Vec<Expr>,
        span: Span,
    },
}

impl Expr {
    pub(crate) fn span(&self) -> Span {
        match self {
            Expr::Int(_, s) | Expr::Str(_, s) | Expr::Ident(_, s) | Expr::Call { span: s, .. } => {
                *s
            }
        }
    }
}
