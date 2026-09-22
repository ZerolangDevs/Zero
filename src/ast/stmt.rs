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
    /// `if cond: body else_if cond: body else: body`
    If {
        cond: Expr,
        then: Block,
        else_ifs: Vec<(Expr, Block)>,
        else_branch: Option<Block>,
    },
    /// `while cond: body`
    While {
        cond: Expr,
        body: Block,
    },
    /// `for var in start..end: body` (or `..=` for inclusive)
    For {
        var: String,
        start: Expr,
        end: Expr,
        inclusive: bool,
        body: Block,
    },
    /// `each var in iter: body` - iterates over a range (Int) or chars (Str)
    Each {
        var: String,
        iter: Expr,
        body: Block,
    },
    /// `switch value: { case v: body ... default: body }`
    Switch {
        value: Expr,
        arms: Vec<(Expr, Block)>,
        default: Option<Block>,
    },
    /// `try: body catch var: body`
    Try {
        body: Block,
        catch_var: Option<String>,
        catch_body: Option<Block>,
    },
}
