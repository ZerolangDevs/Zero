//! Pretty-printer for the Zero language: parse source and regenerate it in
//! a canonical format (`zero <file.zero> fmt`).
//!
//! Formatting rules:
//! - 4-space indentation; one statement per line.
//! - Function bodies use `{ ... }`; control-flow bodies use Python-style
//!   indented blocks.
//! - Binary expressions are parenthesised by operator precedence so
//!   formatting never changes evaluation order.
//! - Compound assignments (`+=`/`-=`/`*=`), `x = x op rhs`, are restored
//!   to their compact form when the AST came from one.
//! - `//` comments are not preserved (the AST does not carry them).
//!
//! Note: `scope lowlevel { ... }` raw Rust blocks are emitted verbatim.

pub use zeroc_ast as ast;
pub use zeroc_diag as diag;
pub use zeroc_parser as parser;

use crate::ast::{BinOp, Block, Expr, Item, Program, ScopeKind, Stmt, UnOp};
use crate::diag::CompileError;

/// Format a Zero source file into canonical form.
pub fn format(src: &str, file: &str) -> Result<String, CompileError> {
    let prog = parser::parse(src, file)?;
    let mut f = Fmt::new();
    f.program(&prog);
    if !f.out.is_empty() && !f.out.ends_with('\n') {
        f.out.push('\n');
    }
    Ok(f.out)
}

/// Binary operator precedence, matching the parser (higher binds tighter).
fn op_prec(op: BinOp) -> u8 {
    match op {
        BinOp::Or => 1,
        BinOp::And => 2,
        BinOp::Eq | BinOp::Ne => 3,
        BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => 4,
        BinOp::Add | BinOp::Sub | BinOp::PathJoin => 5,
        BinOp::Mul | BinOp::Div | BinOp::Rem => 6,
    }
}

fn op_str(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Rem => "%",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::And => "and",
        BinOp::Or => "or",
        BinOp::PathJoin => "+<path>",
    }
}

/// Escape a string value back into a Zero string literal body.
fn escape_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            _ => out.push(c),
        }
    }
    out
}

/// Format a float so it always reads as a float literal (never `1`).
fn float_str(f: f64) -> String {
    let s = format!("{f}");
    if s.contains('.') || s.contains('e') || s.contains("inf") || s.contains("NaN") {
        s
    } else {
        format!("{s}.0")
    }
}

struct Fmt {
    out: String,
    indent: usize,
}

impl Fmt {
    fn new() -> Self {
        Fmt {
            out: String::new(),
            indent: 0,
        }
    }

    /// Write a line at the current indent, followed by a newline.
    fn line(&mut self, s: &str) {
        if !self.out.is_empty() {
            self.out.push('\n');
        }
        for _ in 0..self.indent {
            self.out.push_str("    ");
        }
        self.out.push_str(s);
    }

    /// Blank line between top-level items (not after the last one).
    fn blank(&mut self) {
        if !self.out.is_empty() && !self.out.ends_with("\n\n") {
            self.out.push('\n');
        }
    }

    // ---- program / items ----

    fn program(&mut self, prog: &Program) {
        let mut items = prog.items.iter().peekable();
        while let Some(item) = items.next() {
            match item {
                Item::Import(i) => self.line(&format!("import {}", i.name)),
                Item::Function(f) => self.function(f),
                Item::Top(stmt) => self.stmt(stmt),
            }
            // 顶层语句与连续 import 不空行；其他 item 之间空一行。
            let next_is_top = matches!(items.peek(), Some(Item::Top(_)));
            let cur_is_top = matches!(item, Item::Top(_));
            let next_is_import = matches!(items.peek(), Some(Item::Import(_)));
            let cur_is_import = matches!(item, Item::Import(_));
            let no_blank = (cur_is_top && next_is_top) || (cur_is_import && next_is_import);
            if items.peek().is_some() && !no_blank {
                self.blank();
            }
        }
    }

    fn function(&mut self, f: &crate::ast::Function) {
        let params: Vec<String> = f
            .params
            .iter()
            .map(|p| match &p.ty {
                Some(t) => format!("{}<{}>", p.name, t.name()),
                None => p.name.clone(),
            })
            .collect();
        let ret = match f.ret {
            Some(t) => format!(" -> {}", t.name()),
            None => String::new(),
        };
        self.line(&format!(
            "func {}({}){}: {{",
            f.name,
            params.join(", "),
            ret
        ));
        self.indent += 1;
        match &f.body {
            Block::Stmts(stmts) => self.stmts(stmts),
            Block::Raw(_) => {} // 函数体不会是 raw
        }
        self.indent -= 1;
        self.line("}");
    }

    // ---- statements ----

    fn stmts(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            self.stmt(stmt);
        }
    }

    fn stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Assign {
                name,
                value,
                is_const,
                ..
            } => self.assign(name, value, *is_const),
            Stmt::Expr(e) => self.line(&self.expr(e, 0)),
            Stmt::Return { value } => match value {
                Some(e) => self.line(&format!("return {}", self.expr(e, 0))),
                None => self.line("return"),
            },
            Stmt::WinCreate { name, args, .. } => {
                let arg_list: Vec<String> = args
                    .iter()
                    .map(|(k, v)| format!("{k}={}", self.expr(v, 0)))
                    .collect();
                self.line(&format!("{}<win> = {{ {} }}", name, arg_list.join(", ")));
            }
            Stmt::Break { .. } => self.line("break"),
            Stmt::Continue { .. } => self.line("continue"),
            Stmt::Scope { kind, block, .. } => match kind {
                ScopeKind::HighLevel => {
                    self.line("scope highlevel {");
                    self.indent += 1;
                    self.block(block);
                    self.indent -= 1;
                    self.line("}");
                }
                ScopeKind::LowLevel => {
                    self.line("scope lowlevel {");
                    self.raw_block(block);
                    self.line("}");
                }
            },
            Stmt::If {
                cond,
                then,
                else_ifs,
                else_branch,
            } => {
                self.control("if", &self.expr(cond, 0), then);
                for (econd, ebody) in else_ifs {
                    self.control("else_if", &self.expr(econd, 0), ebody);
                }
                if let Some(ebody) = else_branch {
                    self.control("else", "", ebody);
                }
            }
            Stmt::While { cond, body } => self.control("while", &self.expr(cond, 0), body),
            Stmt::For {
                var,
                start,
                end,
                inclusive,
                step,
                body,
            } => {
                let range = if *inclusive {
                    format!("{}..={}", self.expr(start, 0), self.expr(end, 0))
                } else {
                    format!("{}..{}", self.expr(start, 0), self.expr(end, 0))
                };
                // 非默认 step 恢复为流式形式。
                let is_default_step = matches!(step, Expr::Int(1, _));
                let header = if is_default_step {
                    format!("for {var} in {range}:")
                } else {
                    format!(
                        "for {var} in from({}).to({}).step({}):",
                        self.expr(start, 0),
                        self.expr(end, 0),
                        self.expr(step, 0)
                    )
                };
                self.control_body(&header, body);
            }
            Stmt::Each { var, iter, body } => {
                self.control_body(&format!("each {var} in {}:", self.expr(iter, 0)), body);
            }
            Stmt::Switch {
                value,
                arms,
                default,
            } => {
                self.line(&format!("switch {}:", self.expr(value, 0)));
                self.indent += 1;
                for (av, abody) in arms {
                    self.line(&format!("case {}:", self.expr(av, 0)));
                    self.indent += 1;
                    self.block(abody);
                    self.indent -= 1;
                }
                if let Some(dbody) = default {
                    self.line("default:");
                    self.indent += 1;
                    self.block(dbody);
                    self.indent -= 1;
                }
                self.indent -= 1;
            }
            Stmt::Try {
                body,
                catch_var,
                catch_body,
            } => {
                self.control_body("try:", body);
                if let Some(cbody) = catch_body {
                    let header = match catch_var {
                        Some(v) => format!("catch {v}:"),
                        None => "catch:".to_string(),
                    };
                    self.control_body(&header, cbody);
                }
            }
        }
    }

    /// `name = expr` / `name = expr<const>`; restore compact `+=`/`-=`/`*=`.
    fn assign(&mut self, name: &str, value: &Expr, is_const: bool) {
        if let Expr::Binary { op, lhs, rhs, .. } = value {
            if let Expr::Ident(l, _) = lhs.as_ref() {
                if l == name {
                    let compound = match op {
                        BinOp::Add => Some("+="),
                        BinOp::Sub => Some("-="),
                        BinOp::Mul => Some("*="),
                        _ => None,
                    };
                    if let Some(c) = compound {
                        self.line(&format!("{name} {c} {}", self.expr(rhs, 0)));
                        return;
                    }
                }
            }
        }
        let v = self.expr(value, 0);
        let s = if is_const {
            format!("{name} = {v}<const>")
        } else {
            format!("{name} = {v}")
        };
        self.line(&s);
    }

    /// A block (function/scope/raw) as a child of a `{ ... }` container.
    fn block(&mut self, block: &Block) {
        match block {
            Block::Stmts(stmts) => self.stmts(stmts),
            Block::Raw(_) => self.raw_block(block),
        }
    }

    /// Raw Rust inside `scope lowlevel { ... }`, emitted verbatim (no
    /// re-indent: the original text is kept byte-for-byte).
    fn raw_block(&mut self, block: &Block) {
        if let Block::Raw(raw) = block {
            if !raw.is_empty() {
                // line("scope lowlevel {") 已带尾随换行，raw 原样输出。
                self.out.push_str(raw.trim_end_matches('\n'));
            }
        }
    }

    /// `keyword expr:` followed by a Python-style body.
    fn control(&mut self, keyword: &str, expr: &str, body: &Block) {
        let header = if expr.is_empty() {
            format!("{keyword}:")
        } else {
            format!("{keyword} {expr}:")
        };
        self.control_body(&header, body);
    }

    fn control_body(&mut self, header: &str, body: &Block) {
        self.line(header);
        self.indent += 1;
        self.block(body);
        self.indent -= 1;
    }

    // ---- expressions ----

    /// Render an expression; `parent_prec` is the precedence of the
    /// enclosing operator (0 at top level). Parens are added when this
    /// node binds looser than its parent, preserving evaluation order.
    fn expr(&self, e: &Expr, parent_prec: u8) -> String {
        match e {
            Expr::Int(n, _) => n.to_string(),
            Expr::Float(f, _) => float_str(*f),
            Expr::Str(s, _) => format!("\"{}\"", escape_str(s)),
            Expr::Bool(b, _) => b.to_string(),
            Expr::Nil(_) => "NULL".to_string(),
            Expr::Ident(name, _) => name.clone(),
            Expr::Call { callee, args, .. } => {
                let a: Vec<String> = args.iter().map(|x| self.expr(x, 0)).collect();
                format!("{callee}({})", a.join(", "))
            }
            Expr::TypeTo { ty, value, .. } => {
                format!("type_to<{}>({})", ty.name(), self.expr(value, 0))
            }
            Expr::NetCall { member, args, .. } => {
                let a: Vec<String> = args.iter().map(|x| self.expr(x, 0)).collect();
                format!("net.{member}({})", a.join(", "))
            }
            Expr::ServerCall { member, args, .. } => {
                let a: Vec<String> = args.iter().map(|x| self.expr(x, 0)).collect();
                format!("server.{member}({})", a.join(", "))
            }
            Expr::WinOp {
                win, path, args, ..
            } => self.win_op(win, path, args),
            Expr::Binary { op, lhs, rhs, .. } => {
                let prec = op_prec(*op);
                let s = format!(
                    "{} {} {}",
                    self.expr(lhs, prec),
                    op_str(*op),
                    // 左结合：右侧同优先级需要括号。
                    self.expr(rhs, prec + 1)
                );
                if prec < parent_prec {
                    format!("({s})")
                } else {
                    s
                }
            }
            Expr::Unary { op, operand, .. } => {
                let (prefix, prec) = match op {
                    UnOp::Neg => ("-", 7),
                    UnOp::Not => ("!", 7),
                };
                let s = format!("{prefix}{}", self.expr(operand, prec));
                if prec < parent_prec {
                    format!("({s})")
                } else {
                    s
                }
            }
        }
    }

    /// `win.update()` / `win.add.button(x=.., ...)` / `win.<id>.del()` ...
    fn win_op(&self, win: &str, path: &[String], args: &[(String, Expr)]) -> String {
        let segs: Vec<&str> = path.iter().map(|s| s.as_str()).collect();
        let named: Vec<String> = args
            .iter()
            .map(|(k, v)| format!("{k}={}", self.expr(v, 0)))
            .collect();
        let arg_str = named.join(", ");
        let call = |name: &str| format!("{win}.{name}({arg_str})");
        match segs.as_slice() {
            ["update"] => format!("{win}.update()"),
            ["add", kind] => format!("{win}.add.{kind}({arg_str})"),
            [id, "del"] => format!("{win}.{id}.del()"),
            [id, "update"] => format!("{win}.{id}.update({arg_str})"),
            _ => call(&segs.join(".")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(src: &str) -> String {
        format(src, "test.zero").unwrap()
    }

    #[test]
    fn formats_hello_like_program() {
        let out = fmt(
            "import io\nfunc greetUser(name<string>): {\n    print(\"Hello, {}!\\n\", name)\n}\nversion = \"0.3\"\ngreetUser(\"Zero\")\nprint(\"Zero v{}\\n\", version)\n",
        );
        assert!(out.starts_with("import io\n\nfunc greetUser(name<string>): {\n"));
        assert!(out.contains("print(\"Hello, {}!\\n\", name)\n"));
        assert!(out.contains("version = \"0.3\"\n"));
        assert!(out.contains("greetUser(\"Zero\")\n"));
    }

    #[test]
    fn precedence_is_preserved_with_parens() {
        let out = fmt("x = (1 + 2) * 3\ny = 1 + 2 * 3\n");
        assert!(out.contains("x = (1 + 2) * 3\n"));
        assert!(out.contains("y = 1 + 2 * 3\n"));
    }

    #[test]
    fn compound_assign_is_restored() {
        let out = fmt("x = 1\nx += 2\nx -= 1\nx *= 3\n");
        assert!(out.contains("x += 2\n"));
        assert!(out.contains("x -= 1\n"));
        assert!(out.contains("x *= 3\n"));
    }

    #[test]
    fn control_flow_formats() {
        let out = fmt(
            "import control\nx = 5\nif x > 0:\n    print(\"pos\\n\")\nelse_if x == 0:\n    print(\"zero\\n\")\nelse:\n    print(\"neg\\n\")\nfor i in from(0).to(3).step(2):\n    print(i)\nswitch x:\n    case 1:\n        print(\"one\\n\")\n    default:\n        print(\"other\\n\")\ntry:\n    call_rust(\"panic!(\\\"boom\\\");\")\ncatch e:\n    print(\"caught {}\\n\", e)\n",
        );
        assert!(out.contains("if x > 0:\n"));
        assert!(out.contains("else_if x == 0:\n"));
        assert!(out.contains("else:\n"));
        assert!(out.contains("for i in from(0).to(3).step(2):\n"));
        assert!(out.contains("switch x:\n"));
        assert!(out.contains("case 1:\n"));
        assert!(out.contains("default:\n"));
        assert!(out.contains("try:\n"));
        assert!(out.contains("catch e:\n"));
    }

    #[test]
    fn win_net_server_ops_format() {
        let out = fmt(
            "import win\nimport network\nimport server\nfunc cb(): {\n    return 1\n}\nfn main() { w<win> = {x=10, y=20, size=\"400x300\"}\nw.update()\nw.add.button(x=5, y=5, size=\"80x30\", id=\"b\", onclick=cb)\nw.b.del()\nw.b.update(x=9, id=\"b2\")\nnet.get(\"http://x\")\nserver.get(\"/\", cb)\nserver.start(8080) }",
        );
        assert!(out.contains("w<win> = { x=10, y=20, size=\"400x300\" }\n"));
        assert!(out.contains("w.update()\n"));
        assert!(out.contains("w.add.button(x=5, y=5, size=\"80x30\", id=\"b\", onclick=cb)\n"));
        assert!(out.contains("w.b.del()\n"));
        assert!(out.contains("w.b.update(x=9, id=\"b2\")\n"));
        assert!(out.contains("net.get(\"http://x\")\n"));
        assert!(out.contains("server.get(\"/\", cb)\n"));
        assert!(out.contains("server.start(8080)\n"));
    }

    #[test]
    fn lowlevel_raw_block_is_kept() {
        let out = fmt("scope lowlevel {\n    let x = 1;\n}\n");
        assert!(out.contains("scope lowlevel {\n    let x = 1;\n}\n"));
    }
}
