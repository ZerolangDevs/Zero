//! Local type inference for the native `i64` fast path.
//!
//! A Zero function is **int-fast** when every parameter is declared `int`
//! and its body only uses integer operations, integer control flow, and
//! calls to other int-fast functions or low-level functions that expose a
//! native `_i64` variant (e.g. `zpow` has `zpow_i64`). Such functions are
//! compiled to raw `i64` code (no `ZVal` boxing).

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::callable::FnInfo;

/// True when a low-level Rust function exposes a native `_i64` variant.
fn has_native_variant(callee: &str, callable: &HashMap<String, FnInfo>) -> bool {
    callable.contains_key(&format!("{callee}_i64"))
}

/// Mark every int-fast function in the unit (fixed-point over call graph).
pub fn analyze(
    fns: &HashMap<String, &Function>,
    callable: &HashMap<String, FnInfo>,
) -> HashSet<String> {
    let names: Vec<String> = fns.keys().cloned().collect();
    let mut memo: HashMap<String, bool> = HashMap::new();
    loop {
        let mut changed = false;
        for name in &names {
            let prev = memo.get(name).copied().unwrap_or(false);
            // 先插入临时值以打断调用环（自递归等）。
            memo.insert(name.clone(), false);
            let now = fns
                .get(name)
                .map(|f| is_int_fast(f, fns, callable, &mut memo))
                .unwrap_or(false);
            memo.insert(name.clone(), now);
            if now && !prev {
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    memo.into_iter()
        .filter(|(_, v)| *v)
        .map(|(k, _)| k)
        .collect()
}

fn is_int_fast(
    f: &Function,
    fns: &HashMap<String, &Function>,
    callable: &HashMap<String, FnInfo>,
    memo: &mut HashMap<String, bool>,
) -> bool {
    if !f.params.iter().all(|p| p.ty == Some(ZType::Int)) {
        return false;
    }
    // 显式声明 `-> int` 才走快速路径（否则返回类型未知，可能改变语义）。
    if f.ret != Some(ZType::Int) {
        return false;
    }
    let Block::Stmts(stmts) = &f.body else {
        return false;
    };
    stmts_int(stmts, fns, callable, memo, &f.name)
}

fn block_int(
    b: &Block,
    fns: &HashMap<String, &Function>,
    callable: &HashMap<String, FnInfo>,
    memo: &mut HashMap<String, bool>,
    current: &str,
) -> bool {
    match b {
        Block::Stmts(stmts) => stmts_int(stmts, fns, callable, memo, current),
        Block::Raw(_) => false,
    }
}

fn stmts_int(
    stmts: &[Stmt],
    fns: &HashMap<String, &Function>,
    callable: &HashMap<String, FnInfo>,
    memo: &mut HashMap<String, bool>,
    current: &str,
) -> bool {
    stmts
        .iter()
        .all(|s| stmt_int(s, fns, callable, memo, current))
}

fn stmt_int(
    s: &Stmt,
    fns: &HashMap<String, &Function>,
    callable: &HashMap<String, FnInfo>,
    memo: &mut HashMap<String, bool>,
    current: &str,
) -> bool {
    match s {
        Stmt::Assign { value, .. } => expr_int(value, fns, callable, memo, current),
        Stmt::Expr(e) => expr_int(e, fns, callable, memo, current),
        Stmt::Return { value } => value
            .as_ref()
            .map(|e| expr_int(e, fns, callable, memo, current))
            .unwrap_or(true),
        Stmt::Break { .. } | Stmt::Continue { .. } => true,
        Stmt::If {
            cond,
            then,
            else_ifs,
            else_branch,
        } => {
            expr_int(cond, fns, callable, memo, current)
                && block_int(then, fns, callable, memo, current)
                && else_ifs.iter().all(|(c, b)| {
                    expr_int(c, fns, callable, memo, current)
                        && block_int(b, fns, callable, memo, current)
                })
                && else_branch
                    .as_ref()
                    .map(|b| block_int(b, fns, callable, memo, current))
                    .unwrap_or(true)
        }
        Stmt::While { cond, body } => {
            expr_int(cond, fns, callable, memo, current)
                && block_int(body, fns, callable, memo, current)
        }
        Stmt::For {
            start,
            end,
            step,
            body,
            ..
        } => {
            expr_int(start, fns, callable, memo, current)
                && expr_int(end, fns, callable, memo, current)
                && expr_int(step, fns, callable, memo, current)
                && block_int(body, fns, callable, memo, current)
        }
        Stmt::Each { iter, body, .. } => {
            expr_int(iter, fns, callable, memo, current)
                && block_int(body, fns, callable, memo, current)
        }
        Stmt::Switch {
            value,
            arms,
            default,
        } => {
            expr_int(value, fns, callable, memo, current)
                && arms.iter().all(|(av, ab)| {
                    expr_int(av, fns, callable, memo, current)
                        && block_int(ab, fns, callable, memo, current)
                })
                && default
                    .as_ref()
                    .map(|b| block_int(b, fns, callable, memo, current))
                    .unwrap_or(true)
        }
        Stmt::Scope {
            kind: ScopeKind::HighLevel,
            block,
            ..
        } => block_int(block, fns, callable, memo, current),
        Stmt::Scope {
            kind: ScopeKind::LowLevel,
            ..
        }
        | Stmt::WinCreate { .. } => false,
        Stmt::Try { .. } => false,
    }
}

fn expr_int(
    e: &Expr,
    fns: &HashMap<String, &Function>,
    callable: &HashMap<String, FnInfo>,
    memo: &mut HashMap<String, bool>,
    current: &str,
) -> bool {
    match e {
        Expr::Int(..) | Expr::Ident(..) => true,
        Expr::Binary { op, lhs, rhs, .. } => {
            matches!(
                op,
                BinOp::Add
                    | BinOp::Sub
                    | BinOp::Mul
                    | BinOp::Div
                    | BinOp::Rem
                    | BinOp::Eq
                    | BinOp::Ne
                    | BinOp::Lt
                    | BinOp::Le
                    | BinOp::Gt
                    | BinOp::Ge
                    | BinOp::And
                    | BinOp::Or
            ) && expr_int(lhs, fns, callable, memo, current)
                && expr_int(rhs, fns, callable, memo, current)
        }
        Expr::Unary { op, operand, .. } => {
            matches!(op, UnOp::Neg | UnOp::Not) && expr_int(operand, fns, callable, memo, current)
        }
        Expr::Call { callee, args, .. } => {
            args.iter()
                .all(|a| expr_int(a, fns, callable, memo, current))
                && (has_native_variant(callee, callable)
                    || callee == current // 直接自递归：_i64 版本递归自身即可
                    || match fns.get(callee) {
                        Some(cf) => match memo.get(callee) {
                            Some(&v) => v,
                            None => is_int_fast(cf, fns, callable, memo),
                        },
                        None => false,
                    })
        }
        _ => false,
    }
}
