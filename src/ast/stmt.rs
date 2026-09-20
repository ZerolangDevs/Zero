//! Statements in the Zero language.

use crate::ast::{Block, Expr, ScopeKind};
use crate::diag::Span;

#[derive(Debug, Clone)]
pub enum Stmt {
    /// Dynamic variable: `name = expr` creates or overwrites a variable of
    /// any type. No type annotations. `name = expr<const>` is immutable.
    Assign {
        name: String,
        value: Expr,
        is_const: bool,
        span: Span,
    },
    Expr(Expr),
    Return {
        value: Option<Expr>,
    },
    Scope {
        kind: ScopeKind,
        block: Block,
        span: Span,
    },
}
