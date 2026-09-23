//! Rust code generation for the Zero language.
//!
//! The generated output is a single self-contained `.rs` file:
//! ZVal runtime -> `use` imports -> inlined raw headers -> runtime helpers
//! -> functions. Every Zero variable is a dynamic `ZVal`; every function
//! takes `ZVal` parameters and returns `ZVal`.

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::import::{Loader, BUILTIN_IMPORTS};
use crate::scope::{BUILTIN_CALL_RUST, BUILTIN_CALL_SYS};

/// Dynamic-value runtime injected at the top of every generated file.
mod emit;
mod runtime;

use emit::{ends_with_return, escape_rust_string, rust_block, INDENT};
use runtime::{preamble, ZVAL_RUNTIME};

pub struct Codegen<'a> {
    src_name: &'a str,
    imports: Vec<String>,
    body: String,
    indent: usize,
    /// True when the `io` standard header has been imported.
    io_imported: bool,
    used_call_sys: bool,
    emitted_builtins: HashSet<String>,
    /// Zero variables declared per active block: name -> immutable flag.
    /// Controls `let`/`let mut` on first declaration vs plain reassignment.
    blocks: Vec<HashMap<String, bool>>,
    /// Declared return type of the function being generated (None outside).
    current_ret: Option<ZType>,
}

/// Compile a program (plus everything loaded from its headers) to Rust.
pub fn generate(prog: &Program, loader: &Loader, src_name: &str) -> String {
    let mut cg = Codegen {
        src_name,
        imports: Vec::new(),
        body: String::new(),
        indent: 0,
        io_imported: loader.io_imported,
        used_call_sys: false,
        emitted_builtins: HashSet::new(),
        blocks: Vec::new(),
        current_ret: None,
    };

    // 1. builtin header aliases -> `use` statements
    for alias in &loader.builtins {
        if let Some(use_stmt) = BUILTIN_IMPORTS.iter().find(|(a, _)| a == alias) {
            if cg.emitted_builtins.insert(use_stmt.1.to_string()) {
                cg.imports.push(use_stmt.1.to_string());
            }
        }
    }

    // 2. raw Rust headers, inlined verbatim (std/stream* + user headers)
    for (display, content) in &loader.rust_headers {
        cg.imports
            .push(format!("// ---- import: {display} ----\n{content}"));
    }

    // 3. functions from Zero headers
    for (display, hprog) in &loader.zero_headers {
        cg.line(&format!("// ---- import: {display} ----"));
        for f in hprog.functions() {
            cg.gen_function(f);
        }
    }

    // 4. top-level statements form the implicit `fn main` entry point
    cg.gen_top_level(prog);

    // 5. functions from the main program
    for f in prog.functions() {
        cg.gen_function(f);
    }

    cg.assemble()
}

impl<'a> Codegen<'a> {
    /// Top-level statements become `fn main() -> ZVal { ... }`.
    fn gen_top_level(&mut self, prog: &Program) {
        let tops: Vec<Stmt> = prog
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Top(stmt) => Some(stmt.clone()),
                _ => None,
            })
            .collect();
        if tops.is_empty() {
            return;
        }
        self.line("fn main() -> ZVal {");
        self.indent += 1;
        self.blocks.push(HashMap::new());
        self.gen_stmts(&tops);
        if !ends_with_return(&tops) {
            self.line("return ZVal::Nil;");
        }
        self.blocks.pop();
        self.indent -= 1;
        self.line("}");
    }
    fn line(&mut self, text: &str) {
        for _ in 0..self.indent {
            self.body.push_str(INDENT);
        }
        self.body.push_str(text);
        self.body.push('\n');
    }

    fn gen_function(&mut self, f: &Function) {
        let params: Vec<String> = f
            .params
            .iter()
            .map(|p| format!("mut {}: ZVal", p.name))
            .collect();
        self.line(&format!("fn {}({}) -> ZVal {{", f.name, params.join(", ")));
        self.indent += 1;
        self.blocks
            .push(f.params.iter().map(|p| (p.name.clone(), false)).collect());
        // Runtime type checks for annotated parameters.
        for p in &f.params {
            if let Some(ty) = &p.ty {
                self.line(&format!(
                    "__zero_check_type(&{}, {});",
                    p.name,
                    ztype_code(ty)
                ));
            }
        }
        self.current_ret = f.ret;
        if let Block::Stmts(stmts) = &f.body {
            self.gen_stmts(stmts);
            if !ends_with_return(stmts) {
                self.line(&self.checked_nil_return());
            }
        }
        self.current_ret = None;
        self.blocks.pop();
        self.indent -= 1;
        self.line("}");
    }

    fn gen_stmts(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            match stmt {
                Stmt::Assign {
                    name,
                    value,
                    is_const,
                    ..
                } => {
                    let expr = self.gen_expr(value);
                    // Update the nearest existing binding (across blocks), or
                    // declare a new variable in the current block.
                    let declared = self.blocks.iter().rev().any(|b| b.contains_key(name));
                    if declared {
                        self.line(&format!("{name} = {expr};"));
                    } else {
                        let kw = if *is_const { "let" } else { "let mut" };
                        self.blocks
                            .last_mut()
                            .expect("block stack")
                            .insert(name.clone(), *is_const);
                        self.line(&format!("{kw} {name}: ZVal = {expr};"));
                    }
                }
                Stmt::Expr(expr) => {
                    let code = self.gen_expr(expr);
                    self.line(&format!("{code};"));
                }
                Stmt::Return { value } => {
                    let expr = match value {
                        Some(e) => self.gen_expr(e),
                        None => "ZVal::Nil".to_string(),
                    };
                    match &self.current_ret {
                        Some(ty) => {
                            let code =
                                format!("return __zero_check_type(&{expr}, {});", ztype_code(ty));
                            self.line(&code);
                        }
                        _ => self.line(&format!("return {expr};")),
                    }
                }
                Stmt::Break { .. } => {
                    self.line("break;");
                }
                Stmt::Continue { .. } => {
                    self.line("continue;");
                }
                Stmt::Scope { kind, block, .. } => {
                    self.line("{");
                    self.indent += 1;
                    self.blocks.push(HashMap::new());
                    match (kind, block) {
                        (ScopeKind::HighLevel, Block::Stmts(stmts)) => {
                            self.gen_stmts(stmts);
                        }
                        (ScopeKind::LowLevel, Block::Raw(text)) => {
                            for raw_line in text.lines() {
                                let trimmed = raw_line.trim_end();
                                if trimmed.is_empty() {
                                    self.body.push('\n');
                                } else {
                                    self.line(trimmed);
                                }
                            }
                        }
                        _ => {
                            self.line("// unreachable: malformed scope block");
                        }
                    }
                    self.blocks.pop();
                    self.indent -= 1;
                    self.line("}");
                }
                Stmt::If {
                    cond,
                    then,
                    else_ifs,
                    else_branch,
                } => {
                    let c = self.gen_expr(cond);
                    self.line(&format!("if {c}.to_bool() {{"));
                    self.indent += 1;
                    self.gen_block_body(then, None);
                    self.indent -= 1;
                    for (econd, ebody) in else_ifs {
                        let ec = self.gen_expr(econd);
                        self.line(&format!("}} else if {ec}.to_bool() {{"));
                        self.indent += 1;
                        self.gen_block_body(ebody, None);
                        self.indent -= 1;
                    }
                    if let Some(ebody) = else_branch {
                        self.line("} else {");
                        self.indent += 1;
                        self.gen_block_body(ebody, None);
                        self.indent -= 1;
                    }
                    self.line("}");
                }
                Stmt::While { cond, body } => {
                    let c = self.gen_expr(cond);
                    self.line(&format!("while {c}.to_bool() {{"));
                    self.indent += 1;
                    self.gen_block_body(body, None);
                    self.indent -= 1;
                    self.line("}");
                }
                Stmt::For {
                    var,
                    start,
                    end,
                    inclusive,
                    body,
                } => {
                    let s = self.gen_expr(start);
                    let e = self.gen_expr(end);
                    let adj = if *inclusive { " + 1" } else { "" };
                    self.line(&format!(
                        "let __zero_loop_start = {s}.as_int().unwrap_or(0);"
                    ));
                    self.line(&format!(
                        "let __zero_loop_end = {e}.as_int().unwrap_or(0){adj};"
                    ));
                    self.line("for __zero_i in __zero_loop_start..__zero_loop_end {");
                    self.indent += 1;
                    self.gen_block_body(body, Some((var.as_str(), "ZVal::Int(__zero_i)")));
                    self.indent -= 1;
                    self.line("}");
                }
                Stmt::Each { var, iter, body } => {
                    let it = self.gen_expr(iter);
                    self.line(&format!("let __zero_each = {it};"));
                    self.line("match &__zero_each {");
                    self.indent += 1;
                    self.line("ZVal::Int(__zero_n) => {");
                    self.indent += 1;
                    self.line("for __zero_i in 0..*__zero_n {");
                    self.indent += 1;
                    self.gen_block_body(body, Some((var.as_str(), "ZVal::Int(__zero_i)")));
                    self.indent -= 1;
                    self.line("}");
                    self.indent -= 1;
                    self.line("}");
                    self.line("ZVal::Str(__zero_s) => {");
                    self.indent += 1;
                    self.line("for __zero_c in __zero_s.chars() {");
                    self.indent += 1;
                    self.gen_block_body(
                        body,
                        Some((var.as_str(), "ZVal::Str(__zero_c.to_string())")),
                    );
                    self.indent -= 1;
                    self.line("}");
                    self.indent -= 1;
                    self.line("}");
                    self.line("_ => {}");
                    self.indent -= 1;
                    self.line("}");
                }
                Stmt::Switch {
                    value,
                    arms,
                    default,
                } => {
                    let v = self.gen_expr(value);
                    self.line(&format!("let __zero_sw = {v};"));
                    for (i, (av, abody)) in arms.iter().enumerate() {
                        let a = self.gen_expr(av);
                        let head = if i == 0 {
                            format!("if __zero_sw == {a} {{")
                        } else {
                            format!("}} else if __zero_sw == {a} {{")
                        };
                        self.line(&head);
                        self.indent += 1;
                        self.gen_block_body(abody, None);
                        self.indent -= 1;
                    }
                    if let Some(dbody) = default {
                        let head = if arms.is_empty() {
                            "else {".to_string()
                        } else {
                            "} else {".to_string()
                        };
                        self.line(&head);
                        self.indent += 1;
                        self.gen_block_body(dbody, None);
                        self.indent -= 1;
                    }
                    if !arms.is_empty() || default.is_some() {
                        self.line("}");
                    }
                }
                Stmt::Try {
                    body,
                    catch_var,
                    catch_body,
                } => {
                    self.line("let __zero_try = std::panic::catch_unwind(|| -> ZVal {");
                    self.indent += 1;
                    // Inside the closure a `return` ends the try block, not
                    // the function, so no function return-type checks apply.
                    let saved_ret = self.current_ret;
                    self.current_ret = None;
                    self.blocks.push(HashMap::new());
                    match body {
                        Block::Stmts(stmts) => {
                            self.gen_stmts(stmts);
                            if !ends_with_return(stmts) {
                                self.line("return ZVal::Nil;");
                            }
                        }
                        Block::Raw(text) => {
                            for raw_line in text.lines() {
                                let trimmed = raw_line.trim_end();
                                if trimmed.is_empty() {
                                    self.body.push('\n');
                                } else {
                                    self.line(trimmed);
                                }
                            }
                        }
                    }
                    self.blocks.pop();
                    self.current_ret = saved_ret;
                    self.indent -= 1;
                    self.line("});");
                    if let Some(cbody) = catch_body {
                        self.line("if let Err(__zero_payload) = __zero_try {");
                        self.indent += 1;
                        self.line("let __zero_msg = __zero_payload");
                        self.indent += 1;
                        self.line(".downcast_ref::<&str>().map(|s| s.to_string())");
                        self.line(".or_else(|| __zero_payload.downcast_ref::<String>().cloned())");
                        self.line(".unwrap_or_else(|| \"panic\".to_string());");
                        self.indent -= 1;
                        let cextra = catch_var
                            .as_deref()
                            .map(|v| (v, "ZVal::Str(__zero_msg.clone())"));
                        self.gen_block_body(cbody, cextra);
                        self.indent -= 1;
                        self.line("}");
                    }
                }
            }
        }
    }

    /// Emit a block body inside a fresh Rust block. `extra` is a
    /// `(name, init_expr)` pair for a variable pre-declared in that block
    /// (loop / catch variables).
    fn gen_block_body(&mut self, block: &Block, extra: Option<(&str, &str)>) {
        self.blocks.push(HashMap::new());
        if let Some((v, init)) = extra {
            self.blocks
                .last_mut()
                .expect("block stack")
                .insert(v.to_string(), false);
            self.line(&format!("let {v}: ZVal = {init};"));
        }
        match block {
            Block::Stmts(stmts) => self.gen_stmts(stmts),
            Block::Raw(text) => {
                for raw_line in text.lines() {
                    let trimmed = raw_line.trim_end();
                    if trimmed.is_empty() {
                        self.body.push('\n');
                    } else {
                        self.line(trimmed);
                    }
                }
            }
        }
        self.blocks.pop();
    }

    fn gen_expr(&mut self, expr: &Expr) -> String {
        match expr {
            Expr::Int(v, _) => format!("ZVal::Int({v})"),
            Expr::Float(v, _) => format!("ZVal::Float({})", rust_float_literal(*v)),
            Expr::Str(s, _) => format!("ZVal::Str({}.into())", escape_rust_string(s)),
            Expr::Bool(b, _) => format!("ZVal::Bool({b})"),
            Expr::Nil(..) => "ZVal::Nil".to_string(),
            Expr::Ident(name, _) => format!("{name}.clone()"),
            Expr::TypeTo { ty, value, .. } => {
                let arg = self.gen_expr(value);
                let helper = match ty {
                    ZType::Int => "__zero_to_int",
                    ZType::Float => "__zero_to_float",
                    ZType::Str => "__zero_to_str",
                    ZType::Bool => "__zero_to_bool",
                };
                format!("{helper}(&{arg})")
            }
            Expr::Call {
                callee,
                args,
                span: _,
            } => match callee.as_str() {
                BUILTIN_CALL_RUST => {
                    // The analyzer guarantees args[0] is a string literal.
                    let Expr::Str(raw, _) = &args[0] else {
                        return "/* invalid call_rust */".to_string();
                    };
                    rust_block(raw, self.indent)
                }
                BUILTIN_CALL_SYS => {
                    self.used_call_sys = true;
                    // The command must reach Rust as a `&str`, not a ZVal.
                    let arg = self.gen_io_str_arg(&args[0]);
                    format!("__zero_sys({arg})")
                }
                "print" | "input" if self.io_imported => {
                    let fmt = self.gen_io_str_arg(&args[0]);
                    let rest: Vec<String> = args[1..].iter().map(|a| self.gen_expr(a)).collect();
                    format!("{callee}({fmt}, &[{}])", rest.join(", "))
                }
                "input_s" if self.io_imported => "input_s()".to_string(),
                "set_stream" if self.io_imported => {
                    let kind = self.gen_io_str_arg(&args[0]);
                    let name = if args.len() >= 2 {
                        format!("Some({})", self.gen_io_str_arg(&args[1]))
                    } else {
                        "None".to_string()
                    };
                    format!("set_stream({kind}, {name})")
                }
                _ => {
                    let args_code: Vec<String> = args.iter().map(|a| self.gen_expr(a)).collect();
                    format!("{callee}({})", args_code.join(", "))
                }
            },
            Expr::Binary { op, lhs, rhs, .. } => {
                let l = self.gen_expr(lhs);
                let r = self.gen_expr(rhs);
                match op {
                    BinOp::Add => format!("{l}.add(&{r})"),
                    BinOp::Sub => format!("{l}.sub(&{r})"),
                    BinOp::Mul => format!("{l}.mul(&{r})"),
                    BinOp::Div => format!("{l}.div(&{r})"),
                    BinOp::Rem => format!("{l}.rem(&{r})"),
                    BinOp::Eq => format!("ZVal::Bool({l} == {r})"),
                    BinOp::Ne => format!("ZVal::Bool({l} != {r})"),
                    BinOp::Lt => format!("{l}.lt(&{r})"),
                    BinOp::Le => format!("{l}.le(&{r})"),
                    BinOp::Gt => format!("{l}.gt(&{r})"),
                    BinOp::Ge => format!("{l}.ge(&{r})"),
                    // && and || keep Rust's short-circuit semantics.
                    BinOp::And => format!("ZVal::Bool({l}.to_bool() && {r}.to_bool())"),
                    BinOp::Or => format!("ZVal::Bool({l}.to_bool() || {r}.to_bool())"),
                }
            }
            Expr::Unary { op, operand, .. } => {
                let o = self.gen_expr(operand);
                match op {
                    UnOp::Neg => format!("{o}.neg()"),
                    UnOp::Not => format!("ZVal::Bool(!{o}.to_bool())"),
                }
            }
        }
    }

    /// Render an argument that must reach Rust as a `&str`: string literals
    /// stay literals, everything else is stringified via the ZVal runtime.
    fn gen_io_str_arg(&mut self, expr: &Expr) -> String {
        match expr {
            Expr::Str(s, _) => escape_rust_string(s),
            other => format!("&{}.to_rust_string()", self.gen_expr(other)),
        }
    }

    fn assemble(&self) -> String {
        let mut out = String::new();
        out.push_str("#![allow(unused, dead_code, nonstandard_style)]\n");
        out.push_str("// Generated by Zero compiler (zeroc v0.3.0)\n");
        out.push_str(&format!("// Source: {}\n", self.src_name));
        out.push('\n');
        out.push_str(ZVAL_RUNTIME);
        out.push('\n');
        if !self.imports.is_empty() {
            for imp in &self.imports {
                out.push_str(imp);
                out.push_str("\n\n");
            }
        }
        if self.used_call_sys {
            out.push_str(&preamble());
            out.push('\n');
        }
        out.push_str(&self.body);
        out
    }
}

impl<'a> Codegen<'a> {
    /// The implicit `return nil` at the end of a function body, wrapped in a
    /// runtime type check when the function declares a concrete return type.
    fn checked_nil_return(&self) -> String {
        match &self.current_ret {
            Some(ty) => {
                format!("return __zero_check_type(&ZVal::Nil, {});", ztype_code(ty))
            }
            _ => "return ZVal::Nil;".to_string(),
        }
    }
}

/// Rust source for a declared Zero type.
fn ztype_code(ty: &ZType) -> &'static str {
    match ty {
        ZType::Int => "ZValType::Int",
        ZType::Float => "ZValType::Float",
        ZType::Str => "ZValType::Str",
        ZType::Bool => "ZValType::Bool",
    }
}

/// Format an `f64` as a valid Rust float literal (`2.0` stays `2.0`, not
/// `2`, so the generated code is unambiguously a float).
fn rust_float_literal(v: f64) -> String {
    let s = format!("{v}");
    if s.contains('.') || s.contains('e') || s.contains('E') || s.contains("inf") || s == "NaN" {
        s
    } else {
        format!("{s}.0")
    }
}

/// True when the function body already ends with an explicit `return`.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::Loader;
    use crate::parser::parse;
    use std::path::Path;

    fn generate_for(src: &str) -> String {
        let prog = parse(src, "test.zero").unwrap();
        let mut loader = Loader::new(Path::new("."));
        loader.register_main(&prog, "test.zero").unwrap();
        loader.load(&prog, "test.zero").unwrap();
        generate(&prog, &loader, "test.zero")
    }

    #[test]
    fn always_injects_zval_runtime() {
        let out = generate_for("fn main() { call_sys(\"echo hi\") }");
        assert!(out.contains("pub enum ZVal"));
        assert!(out.contains("fn __zero_sys(command: &str) -> i32"));
        assert!(out.contains("fn main() -> ZVal {"));
        assert!(out.contains("return ZVal::Nil;"));
    }

    #[test]
    fn inlines_call_rust() {
        let out = generate_for(r#"fn main() { call_rust("println!(\"hi\");") }"#);
        assert!(out.contains("{ println!(\"hi\"); };"));
    }

    #[test]
    fn variables_declare_then_reassign() {
        let out = generate_for("fn main() { x = 1\nx = 2 }");
        assert!(out.contains("let mut x: ZVal = ZVal::Int(1);"));
        assert!(out.contains("x = ZVal::Int(2);"));
    }

    #[test]
    fn const_emits_immutable_let() {
        let out = generate_for("fn main() { x = 1<const>\nx = 2 }");
        // The analyzer would reject the reassignment; codegen itself only
        // decides let vs let mut from the marker.
        assert!(out.contains("let x: ZVal = ZVal::Int(1);"));
        assert!(!out.contains("let mut x: ZVal"));
    }

    #[test]
    fn function_params_and_return() {
        let out = generate_for("fn add(a, b) { return a }\nfn main() { add(1, 2) }");
        assert!(out.contains("fn add(mut a: ZVal, mut b: ZVal) -> ZVal {"));
        assert!(out.contains("add(ZVal::Int(1), ZVal::Int(2));"));
        // no trailing auto-return because the body already returns
        assert!(!out.contains(
            "fn add(mut a: ZVal, mut b: ZVal) -> ZVal {\n    return a;\n    return ZVal::Nil;"
        ));
    }

    #[test]
    fn io_functions_emit_std_calls() {
        let out = generate_for(
            "import io\nfn main() { print(\"hi {}\", 1)\ninput_s()\nset_stream(\"file\", \"a.txt\") }",
        );
        assert!(out.contains("print(\"hi {}\", &[ZVal::Int(1)]);"));
        assert!(out.contains("input_s();"));
        assert!(out.contains("set_stream(\"file\", Some(\"a.txt\"));"));
        assert!(out.contains("std/stream.rs"));
        assert!(out.contains("std/stream_io.rs"));
        assert!(out.contains("std/io.rs"));
    }

    #[test]
    fn io_not_imported_keeps_user_fn() {
        let out = generate_for("fn print(a) { return a }\nfn main() { print(1) }");
        assert!(out.contains("fn print(mut a: ZVal) -> ZVal {"));
        assert!(out.contains("print(ZVal::Int(1));"));
    }

    #[test]
    fn scopes_become_rust_blocks() {
        let out = generate_for(
            "fn main() { x = 1\nscope highlevel { x = 2 }\nscope lowlevel { let y = 3; } }",
        );
        // 内层赋值更新外层绑定，不重复 let
        assert_eq!(out.matches("let mut x: ZVal =").count(), 1);
        assert!(out.contains("x = ZVal::Int(2);"));
        assert!(out.contains("let y = 3;"));
    }

    #[test]
    fn loop_accumulation_updates_outer() {
        let out = generate_for(
            "import control\nfn main() { sum = 0\nfor i in 0..5:\n    sum = sum + i }",
        );
        assert!(out.contains("let mut sum: ZVal = ZVal::Int(0);"));
        assert!(out.contains("sum = sum.clone().add(&i.clone());"));
    }

    #[test]
    fn top_level_statements_become_main() {
        let out = generate_for("x = 1\nprint(\"hi\")\nfn helper() { return 1 }");
        assert!(out.contains("fn main() -> ZVal {"));
        assert!(out.contains("let mut x: ZVal = ZVal::Int(1);"));
        assert!(out.contains("fn helper() -> ZVal {"));
    }

    #[test]
    fn operators_emit_zval_calls() {
        let out = generate_for("fn main() { x = 1 + 2 * 3\ny = a == b\nz = a and b\nn = -x }");
        assert!(
            out.contains("let mut x: ZVal = ZVal::Int(1).add(&ZVal::Int(2).mul(&ZVal::Int(3)));")
        );
        assert!(out.contains("let mut y: ZVal = ZVal::Bool(a.clone() == b.clone());"));
        assert!(out
            .contains("let mut z: ZVal = ZVal::Bool(a.clone().to_bool() && b.clone().to_bool());"));
        assert!(out.contains("let mut n: ZVal = x.clone().neg();"));
    }

    #[test]
    fn func_types_emit_runtime_checks() {
        let out = generate_for("func add(a<int>, b) -> int: a + b\nfn main() { add(1, 2) }");
        assert!(out.contains("fn add(mut a: ZVal, mut b: ZVal) -> ZVal {"));
        assert!(out.contains("__zero_check_type(&a, ZValType::Int);"));
        // 单表达式体 -> 隐式 return，且带返回类型校验
        assert!(
            out.contains("return __zero_check_type(&a.clone().add(&b.clone()), ZValType::Int);")
        );
    }

    #[test]
    fn bool_literal_emits_zval() {
        let out = generate_for("fn main() { x = true }");
        assert!(out.contains("let mut x: ZVal = ZVal::Bool(true);"));
    }

    #[test]
    fn escapes_strings() {
        assert_eq!(escape_rust_string("a\"b\\c"), "\"a\\\"b\\\\c\"");
    }

    #[test]
    fn math_std_header_inlines_lowlevel_and_zero() {
        let out = generate_for("import math\nfn main() { x = pow(2, 3)\ny = add(1, 2) }");
        // lowlevel_math is inlined as raw Rust.
        assert!(out.contains("std/lowlevel_math.rs"));
        assert!(out.contains("pub fn zadd(a: ZVal, b: ZVal) -> ZVal {"));
        assert!(out.contains("pub fn zneg(a: ZVal) -> ZVal {"));
        // math is compiled from Zero and builds on lowlevel_math.
        assert!(out.contains("std/math.zh"));
        assert!(out.contains("fn add(mut a: ZVal, mut b: ZVal) -> ZVal {"));
        assert!(out.contains("return zadd(a.clone(), b.clone());"));
        assert!(out.contains("fn pow(mut base: ZVal, mut exp: ZVal) -> ZVal {"));
        assert!(out.contains("fn gcd(mut a: ZVal, mut b: ZVal) -> ZVal {"));
        assert!(out.contains("fn digit_sum(mut n: ZVal) -> ZVal {"));
    }

    #[test]
    fn lowlevel_math_works_without_math() {
        let out = generate_for("import lowlevel_math\nfn main() { x = zadd(1, 2) }");
        assert!(out.contains("std/lowlevel_math.rs"));
        assert!(out.contains("pub fn zsub(a: ZVal, b: ZVal) -> ZVal {"));
        assert!(!out.contains("std/math.zh"));
    }

    #[test]
    fn float_and_null_and_type_to_emit() {
        let out = generate_for(
            "fn main() { x = 3.14\ny = NULL\nz = type_to<int>(\"42\")\nf = type_to<float>(1)\ns = type_to<string>(2)\nb = type_to<bool>(x) }",
        );
        assert!(out.contains("let mut x: ZVal = ZVal::Float(3.14);"));
        assert!(out.contains("let mut y: ZVal = ZVal::Nil;"));
        assert!(out.contains("let mut z: ZVal = __zero_to_int(&ZVal::Str(\"42\".into()));"));
        assert!(out.contains("let mut f: ZVal = __zero_to_float(&ZVal::Int(1));"));
        assert!(out.contains("let mut s: ZVal = __zero_to_str(&ZVal::Int(2));"));
        assert!(out.contains("let mut b: ZVal = __zero_to_bool(&x.clone());"));
    }

    #[test]
    fn break_continue_emit() {
        let out = generate_for(
            "import control\nfn main() { for i in 0..5:\n    if i == 1:\n        continue\n    if i == 3:\n        break }",
        );
        assert!(out.contains("continue;"));
        assert!(out.contains("break;"));
    }

    #[test]
    fn strings_std_header_inlines_lowlevel_and_zero() {
        let out =
            generate_for("import strings\nfn main() { x = len(\"abc\")\ny = upper(\"abc\") }");
        assert!(out.contains("std/lowlevel_strings.rs"));
        assert!(out.contains("pub fn zlen(a: ZVal) -> ZVal {"));
        assert!(out.contains("std/strings.zh"));
        assert!(out.contains("fn len(mut s: ZVal) -> ZVal {"));
        assert!(out.contains("return zlen(s.clone());"));
        assert!(out.contains("fn capitalize(mut s: ZVal) -> ZVal {"));
    }
}
