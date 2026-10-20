//! Scope analysis for the Zero language.
//!
//! Every Zero program starts in the **highlevel** scope. A `scope highlevel
//! { ... }` block opens a nested variable scope; a `scope lowlevel { ... }`
//! block switches to raw Rust and is not analysed. Variables are dynamic:
//! `name = expr` creates or overwrites a variable, and any read must resolve
//! against the scope chain. Calls must target a builtin, an imported `io`
//! function, or a known user function with a matching arity.

use std::collections::{HashMap, HashSet};
pub use zeroc_ast as ast;
pub use zeroc_diag as diag;
pub use zeroc_parser as parser;

use crate::ast::*;
use crate::diag::{CompileError, Span};

/// Builtin function names understood by the compiler.
mod callable;

pub mod int_fast;

pub use callable::{FnInfo, BUILTIN_CALL_RUST, BUILTIN_CALL_SYS, IO_FUNCTIONS};

struct Scope {
    /// Variable name -> immutable flag. Assignment both creates and
    /// overwrites a variable in the current scope.
    symbols: HashMap<String, bool>,
}

pub struct ScopeAnalyzer<'a> {
    /// All callable functions (user + io) known to the whole unit.
    functions: &'a HashMap<String, FnInfo>,
    /// True when `import io` is active: `print`/`input_s`/`input`/`set_stream`
    /// are then treated as the standard IO API.
    io_imported: bool,
    /// True when `import sys` is active: the `+<path>` path-join operator
    /// is then available.
    sys_imported: bool,
    scopes: Vec<Scope>,
    /// Declared return type of the function being analysed (None outside).
    current_ret: Option<ZType>,
    /// Nesting depth of `while` / `for` / `each` loops, for validating
    /// `break` / `continue`.
    loop_depth: usize,
    /// Variables created with `name<win> = { ... }`.
    win_vars: HashSet<String>,
    file: String,
}

impl<'a> ScopeAnalyzer<'a> {
    pub fn new(
        functions: &'a HashMap<String, FnInfo>,
        file: &str,
        io_imported: bool,
        sys_imported: bool,
    ) -> Self {
        let mut scopes = Vec::new();
        scopes.push(Scope {
            symbols: HashMap::new(),
        });
        ScopeAnalyzer {
            functions,
            io_imported,
            sys_imported,
            scopes,
            current_ret: None,
            loop_depth: 0,
            win_vars: HashSet::new(),
            file: file.to_string(),
        }
    }

    fn err(&self, message: impl Into<String>, span: Span) -> CompileError {
        CompileError::at(message, self.file.clone(), span)
    }

    pub fn analyze_program(&mut self, prog: &Program) -> Result<(), CompileError> {
        // Top-level statements form the implicit entry point: analyse them
        // inside a fresh scope, as if they were one function body.
        let tops: Vec<Stmt> = prog
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Top(stmt) => Some(stmt.clone()),
                _ => None,
            })
            .collect();
        if !tops.is_empty() {
            self.analyze_block(&Block::Stmts(tops), None)?;
        }
        for item in &prog.items {
            if let Item::Function(f) = item {
                self.analyze_function(f)?;
            }
        }
        Ok(())
    }

    fn analyze_function(&mut self, f: &Function) -> Result<(), CompileError> {
        let Block::Stmts(stmts) = &f.body else {
            return Err(self.err("function body must be a highlevel block", f.span));
        };
        // Function bodies open a fresh highlevel scope; parameters are in
        // scope and may be reassigned.
        let mut scope = Scope {
            symbols: HashMap::new(),
        };
        for p in &f.params {
            scope.symbols.insert(p.name.clone(), false);
        }
        self.scopes.push(scope);
        self.current_ret = f.ret;
        let result = self.analyze_stmts(stmts);
        self.current_ret = None;
        self.scopes.pop();
        result
    }

    fn analyze_stmts(&mut self, stmts: &[Stmt]) -> Result<(), CompileError> {
        for stmt in stmts {
            match stmt {
                Stmt::Assign {
                    name,
                    value,
                    is_const,
                    span,
                } => {
                    self.analyze_expr(value)?;
                    // Dynamic semantics: update the nearest existing binding
                    // (across scopes, so loops can accumulate), otherwise
                    // create a new variable in the current scope.
                    let existing = self
                        .scopes
                        .iter()
                        .rev()
                        .find_map(|sc| sc.symbols.get(name).copied());
                    let msg = match existing {
                        Some(_) if *is_const => Some(format!(
                            "cannot declare immutable variable '{name}': it already exists"
                        )),
                        Some(true) => Some(format!("cannot assign to immutable variable '{name}'")),
                        Some(false) | None => None,
                    };
                    if let Some(m) = msg {
                        return Err(self.err(m, *span));
                    }
                    if existing.is_none() {
                        self.current_mut().symbols.insert(name.clone(), *is_const);
                    }
                }
                Stmt::Expr(expr) => {
                    // Bare identifiers as statements are allowed (no-op).
                    self.analyze_expr(expr)?;
                }
                Stmt::Return { value } => {
                    if let Some(e) = value {
                        self.analyze_expr(e)?;
                        if let Some(declared) = self.current_ret {
                            if let Some(found) = literal_type(e) {
                                if declared != found {
                                    return Err(self.err(
                                        format!(
                                            "function returns {}, but declared to return {}",
                                            found.name(),
                                            declared.name()
                                        ),
                                        e.span(),
                                    ));
                                }
                            }
                        }
                    }
                }
                Stmt::Scope { kind, block, span } => {
                    let _ = span;
                    match kind {
                        ScopeKind::HighLevel => {
                            let Block::Stmts(stmts) = block else {
                                return Err(
                                    self.err("highlevel scope cannot contain raw code", *span)
                                );
                            };
                            self.scopes.push(Scope {
                                symbols: HashMap::new(),
                            });
                            let result = self.analyze_stmts(stmts);
                            self.scopes.pop();
                            result?;
                        }
                        ScopeKind::LowLevel => {
                            // Raw Rust: nothing to analyse at Zero level.
                        }
                    }
                }
                Stmt::If {
                    cond,
                    then,
                    else_ifs,
                    else_branch,
                } => {
                    self.analyze_expr(cond)?;
                    self.analyze_block(then, None)?;
                    for (econd, ebody) in else_ifs {
                        self.analyze_expr(econd)?;
                        self.analyze_block(ebody, None)?;
                    }
                    if let Some(ebody) = else_branch {
                        self.analyze_block(ebody, None)?;
                    }
                }
                Stmt::WinCreate { name, args, span } => {
                    self.win_vars.insert(name.clone());
                    self.current_mut().symbols.insert(name.clone(), false);
                    for (arg_name, value) in args {
                        match arg_name.as_str() {
                            "x" | "y" | "size" => self.analyze_expr(value)?,
                            other => {
                                return Err(self.err(
                                    format!("unknown window option '{other}' (x, y, size)"),
                                    *span,
                                ));
                            }
                        }
                    }
                }
                Stmt::Break { span } => {
                    if self.loop_depth == 0 {
                        return Err(self.err("'break' outside of a loop", *span));
                    }
                }
                Stmt::Continue { span } => {
                    if self.loop_depth == 0 {
                        return Err(self.err("'continue' outside of a loop", *span));
                    }
                }
                Stmt::While { cond, body } => {
                    self.analyze_expr(cond)?;
                    self.loop_depth += 1;
                    let result = self.analyze_block(body, None);
                    self.loop_depth -= 1;
                    result?;
                }
                Stmt::For {
                    var,
                    start,
                    end,
                    step,
                    body,
                    ..
                } => {
                    self.analyze_expr(start)?;
                    self.analyze_expr(end)?;
                    self.analyze_expr(step)?;
                    self.loop_depth += 1;
                    let result = self.analyze_block(body, Some(var));
                    self.loop_depth -= 1;
                    result?;
                }
                Stmt::Each { var, iter, body } => {
                    self.analyze_expr(iter)?;
                    self.loop_depth += 1;
                    let result = self.analyze_block(body, Some(var));
                    self.loop_depth -= 1;
                    result?;
                }
                Stmt::Switch {
                    value,
                    arms,
                    default,
                } => {
                    self.analyze_expr(value)?;
                    for (av, abody) in arms {
                        self.analyze_expr(av)?;
                        self.analyze_block(abody, None)?;
                    }
                    if let Some(dbody) = default {
                        self.analyze_block(dbody, None)?;
                    }
                }
                Stmt::Try {
                    body,
                    catch_var,
                    catch_body,
                } => {
                    self.analyze_block(body, None)?;
                    if let Some(cbody) = catch_body {
                        self.analyze_block(cbody, catch_var.as_deref())?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Analyse a block in a fresh scope; `extra` names a variable that is
    /// pre-declared in that scope (loop/catch variables).
    fn analyze_block(&mut self, block: &Block, extra: Option<&str>) -> Result<(), CompileError> {
        match block {
            Block::Stmts(stmts) => {
                let mut scope = Scope {
                    symbols: HashMap::new(),
                };
                if let Some(v) = extra {
                    scope.symbols.insert(v.to_string(), false);
                }
                self.scopes.push(scope);
                let result = self.analyze_stmts(stmts);
                self.scopes.pop();
                result
            }
            Block::Raw(_) => Ok(()),
        }
    }

    fn analyze_expr(&mut self, expr: &Expr) -> Result<(), CompileError> {
        match expr {
            Expr::Int(..) | Expr::Float(..) | Expr::Str(..) | Expr::Bool(..) | Expr::Nil(..) => {
                Ok(())
            }
            Expr::TypeTo { value, .. } => self.analyze_expr(value),
            Expr::Ident(name, span) => {
                if self.resolve(name).is_none() {
                    Err(self.err(format!("undefined variable '{name}'"), *span))
                } else {
                    Ok(())
                }
            }
            Expr::Call { callee, args, span } => {
                for arg in args {
                    self.analyze_expr(arg)?;
                }
                self.check_call(callee, args, span)
            }
            Expr::NetCall { member, args, span } => {
                let ok = match member.as_str() {
                    "connect" | "post" | "put" | "download" => args.len() == 2,
                    "send" | "get" | "delete" => args.len() == 1,
                    "recv" | "close" => args.is_empty(),
                    "httpRequest" => args.len() == 3,
                    other => {
                        return Err(self.err(format!("unknown net method '{other}'"), *span));
                    }
                };
                if !ok {
                    return Err(self.err(
                        format!("net.{member} takes a fixed number of arguments"),
                        *span,
                    ));
                }
                for a in args {
                    self.analyze_expr(a)?;
                }
                Ok(())
            }
            Expr::ServerCall { member, args, span } => {
                self.check_server_call(member, args, span)?;
                // route 类的最后一个参数是 handler 函数名，不作为变量分析。
                let is_route = matches!(
                    member.as_str(),
                    "get" | "post" | "put" | "delete" | "request"
                );
                for (i, a) in args.iter().enumerate() {
                    if is_route && i + 1 == args.len() {
                        continue;
                    }
                    self.analyze_expr(a)?;
                }
                Ok(())
            }
            Expr::WinOp {
                win,
                path,
                args,
                span,
            } => self.check_win_op(win, path, args, span),
            Expr::Binary { op, lhs, rhs, span } => {
                if *op == BinOp::PathJoin && !self.sys_imported {
                    return Err(self.err("'+<path>' requires 'import sys'", *span));
                }
                self.analyze_expr(lhs)?;
                self.analyze_expr(rhs)
            }
            Expr::Unary { operand, .. } => self.analyze_expr(operand),
        }
    }

    fn check_call(&self, callee: &str, args: &[Expr], span: &Span) -> Result<(), CompileError> {
        match callee {
            BUILTIN_CALL_RUST => {
                if args.len() != 1 {
                    return Err(self.err(
                        "call_rust takes exactly 1 argument (a Rust code string)",
                        *span,
                    ));
                }
                if !matches!(args[0], Expr::Str(..)) {
                    return Err(self.err(
                        "call_rust requires a string literal of Rust code",
                        args[0].span(),
                    ));
                }
                Ok(())
            }
            BUILTIN_CALL_SYS => {
                if args.len() != 1 {
                    return Err(self.err(
                        "call_sys takes exactly 1 argument (a command string)",
                        *span,
                    ));
                }
                Ok(())
            }
            "print" | "input" if self.io_imported => {
                if args.is_empty() {
                    return Err(self.err(
                        format!("{callee} takes a format string plus optional values"),
                        *span,
                    ));
                }
                Ok(())
            }
            "input_s" if self.io_imported => {
                if !args.is_empty() {
                    return Err(self.err("input_s takes no arguments", *span));
                }
                Ok(())
            }
            "set_stream" if self.io_imported => {
                if args.len() < 1 || args.len() > 2 {
                    return Err(self.err(
                        "set_stream takes a stream type ('shell' or 'file') and an optional filename",
                        *span,
                    ));
                }
                Ok(())
            }
            other => match self.functions.get(other) {
                Some(info) => {
                    if let Some(k) = info.arity {
                        if args.len() != k {
                            return Err(self.err(
                                format!(
                                    "function '{other}' takes {k} argument(s), found {}",
                                    args.len()
                                ),
                                *span,
                            ));
                        }
                    }
                    self.check_arg_types(other, args, info, span)
                }
                None => Err(self.err(format!("unknown function '{other}'"), *span)),
            },
        }
    }

    /// Statically check literal arguments against declared parameter types.
    /// Non-literal (dynamic) arguments are left to runtime checks.
    fn check_arg_types(
        &self,
        callee: &str,
        args: &[Expr],
        info: &FnInfo,
        span: &Span,
    ) -> Result<(), CompileError> {
        for (i, (arg, declared)) in args.iter().zip(info.params.iter()).enumerate() {
            let Some(declared) = declared else { continue };
            if let Some(found) = literal_type(arg) {
                if *declared != found {
                    return Err(self.err(
                        format!(
                            "argument {} of '{callee}' expects {}, found {}",
                            i + 1,
                            declared.name(),
                            found.name(),
                        ),
                        *span,
                    ));
                }
            }
        }
        Ok(())
    }

    /// Validate a window operation and its named arguments.
    /// Validate a `server.*` call. Route handlers must reference a known
    /// function with 0 or 3 parameters (`func h()` or `func h(method, path,
    /// body)`); other members follow fixed arities.
    fn check_server_call(
        &self,
        member: &str,
        args: &[Expr],
        span: &Span,
    ) -> Result<(), CompileError> {
        let route_arity = match member {
            "get" | "post" | "put" | "delete" => Some(2),
            "request" => Some(3),
            "start" => Some(1),
            "query" | "header" | "param" => Some(1),
            other => {
                return Err(self.err(format!("unknown server method '{other}'"), *span));
            }
        };
        if let Some(n) = route_arity {
            if args.len() != n {
                return Err(self.err(format!("server.{member} takes {n} argument(s)"), *span));
            }
        }
        // route 类：最后一个参数是 handler 函数名。
        if matches!(member, "get" | "post" | "put" | "delete" | "request") {
            let Some(Expr::Ident(handler, _)) = args.last() else {
                return Err(self.err(
                    format!("server.{member}: handler must be a function name"),
                    *span,
                ));
            };
            match self.functions.get(handler) {
                Some(info) if info.params.len() == 0 || info.params.len() == 3 => {}
                Some(info) => {
                    return Err(self.err(
                        format!(
                            "server handler '{handler}' takes {} argument(s); use 0 or 3",
                            info.params.len()
                        ),
                        *span,
                    ));
                }
                None => {
                    return Err(self.err(
                        format!("unknown server handler function '{handler}'"),
                        *span,
                    ));
                }
            }
        }
        Ok(())
    }

    fn check_win_op(
        &mut self,
        win: &str,
        path: &[String],
        args: &[(String, Expr)],
        span: &Span,
    ) -> Result<(), CompileError> {
        if !self.win_vars.contains(win) {
            return Err(self.err(
                format!("'{win}' is not a window (create it with '{win}<win> = {{ ... }}')"),
                *span,
            ));
        }
        let segs: Vec<&str> = path.iter().map(|s| s.as_str()).collect();
        let (kind, ctrl_kind) = match segs.as_slice() {
            ["update"] => ("update", None),
            ["add", k]
                if matches!(
                    *k,
                    "button"
                        | "text"
                        | "select"
                        | "selects"
                        | "input"
                        | "switch"
                        | "slider"
                        | "checkbox"
                        | "spinbox"
                        | "textedit"
                        | "progress"
                        | "group"
                ) =>
            {
                ("add", Some(*k))
            }
            [_, op] if *op == "del" || *op == "update" => ("update_control", None),
            _ => {
                return Err(self.err(
                    "unknown window operation (update, add.<control>, <id>.del, <id>.update)",
                    *span,
                ));
            }
        };
        for (arg_name, value) in args {
            match arg_name.as_str() {
                "x" | "y" | "size" | "items" => self.analyze_expr(value)?,
                "id" => {
                    if !matches!(value, Expr::Str(..)) {
                        return Err(self.err("'id' must be a string literal", *span));
                    }
                }
                "onclick" => {
                    let Expr::Ident(fname, _) = value else {
                        return Err(self.err("'onclick' must be a function name", *span));
                    };
                    match self.functions.get(fname) {
                        Some(info) => {
                            if let Some(k) = info.arity {
                                if k != 0 {
                                    return Err(self.err(
                                        format!(
                                            "'onclick' function '{fname}' must take no arguments"
                                        ),
                                        *span,
                                    ));
                                }
                            }
                        }
                        None => {
                            return Err(self.err(
                                format!("unknown function '{fname}' used as onclick"),
                                *span,
                            ));
                        }
                    }
                }
                "into" => {
                    if kind == "update" {
                        return Err(self.err("'into' is only valid on controls", *span));
                    }
                    if kind == "add" && !matches!(ctrl_kind, Some("input") | Some("textedit")) {
                        return Err(self.err(
                            format!("'into' is only valid for input controls, not '{ctrl_kind:?}'"),
                            *span,
                        ));
                    }
                    let Expr::Ident(v, _) = value else {
                        return Err(self.err("'into' must be a variable name", *span));
                    };
                    // `into=(var)` declares/updates the target variable.
                    self.current_mut().symbols.insert(v.clone(), false);
                }
                other => {
                    return Err(self.err(format!("unknown argument '{other}'"), *span));
                }
            }
        }
        if kind == "add" && self.named_arg(args, "id").is_none() {
            return Err(self.err("'id' is required when adding a control", *span));
        }
        Ok(())
    }

    fn named_arg<'e>(&self, args: &'e [(String, Expr)], name: &str) -> Option<&'e Expr> {
        args.iter().find(|(n, _)| n == name).map(|(_, e)| e)
    }

    fn current_mut(&mut self) -> &mut Scope {
        self.scopes.last_mut().expect("scope stack never empty")
    }

    /// Resolve a variable name against the scope chain (innermost first).
    fn resolve(&self, name: &str) -> Option<&Scope> {
        self.scopes
            .iter()
            .rev()
            .find(|s| s.symbols.contains_key(name))
    }
}

/// The static type of a literal expression, if it has one.
fn literal_type(expr: &Expr) -> Option<ZType> {
    match expr {
        Expr::Int(..) => Some(ZType::Int),
        Expr::Float(..) => Some(ZType::Float),
        Expr::Str(..) => Some(ZType::Str),
        Expr::Bool(..) => Some(ZType::Bool),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn functions_of(prog: &Program) -> HashMap<String, FnInfo> {
        prog.functions()
            .map(|f| {
                (
                    f.name.clone(),
                    FnInfo {
                        arity: Some(f.params.len()),
                        params: f.params.iter().map(|p| p.ty).collect(),
                    },
                )
            })
            .collect()
    }

    fn analyze(src: &str, io_imported: bool) -> Result<(), CompileError> {
        let prog = parse(src, "test.zero").unwrap();
        let fns = functions_of(&prog);
        ScopeAnalyzer::new(&fns, "test.zero", io_imported, false).analyze_program(&prog)
    }

    #[test]
    fn window_ops_require_win_variable() {
        let err = analyze("import win\nfn main() { w.update() }", false).unwrap_err();
        assert!(err.message.contains("is not a window"));
    }

    #[test]
    fn window_create_and_ops_analyze() {
        let src = "import win\nfn cb() { return 1 }\nfn main() { w<win> = {x=10}\nw.update()\nw.add.button(id=\"b\", onclick=cb)\nw.b.del()\nw.b.update(x=5)\nw.add.input(id=\"i\", into=(v)) }";
        let prog = parse(src, "test.zero").unwrap();
        let fns = functions_of(&prog);
        ScopeAnalyzer::new(&fns, "test.zero", false, false)
            .analyze_program(&prog)
            .unwrap();
    }

    #[test]
    fn window_onclick_must_be_zero_arg_fn() {
        let src = "import win\nfn cb(a) { return a }\nfn main() { w<win> = {}\nw.add.button(id=\"b\", onclick=cb) }";
        let prog = parse(src, "test.zero").unwrap();
        let fns = functions_of(&prog);
        let err = ScopeAnalyzer::new(&fns, "test.zero", false, false)
            .analyze_program(&prog)
            .unwrap_err();
        assert!(err.message.contains("must take no arguments"));
    }

    #[test]
    fn path_join_requires_sys_import() {
        let err = analyze("fn main() { p = a +<path> b }", false).unwrap_err();
        assert!(err.message.contains("requires 'import sys'"));
    }

    #[test]
    fn path_join_works_when_sys_imported() {
        let prog = parse(
            "import sys\nfn main() { a = \"x\"\nb = \"y\"\np = a +<path> b }",
            "test.zero",
        )
        .unwrap();
        let fns = functions_of(&prog);
        ScopeAnalyzer::new(&fns, "test.zero", false, true)
            .analyze_program(&prog)
            .unwrap();
    }

    #[test]
    fn resolves_variables_across_scopes() {
        analyze("fn main() { x = 1\nscope highlevel { y = x } }", false).unwrap();
    }

    #[test]
    fn rejects_undefined_variable() {
        let err = analyze("fn main() { y = missing }", false).unwrap_err();
        assert!(err.message.contains("undefined variable 'missing'"));
    }

    #[test]
    fn rejects_unknown_function() {
        let err = analyze("fn main() { nope() }", false).unwrap_err();
        assert!(err.message.contains("unknown function 'nope'"));
    }

    #[test]
    fn call_rust_requires_literal_string() {
        let err = analyze("fn main() { code = \"x\"\ncall_rust(code) }", false).unwrap_err();
        assert!(err.message.contains("string literal"));
    }

    #[test]
    fn checks_arity() {
        let err = analyze("fn add(a, b) { return a }\nfn main() { add(1) }", false).unwrap_err();
        assert!(err.message.contains("takes 2 argument(s)"));
    }

    #[test]
    fn io_functions_require_import() {
        let err = analyze("fn main() { print(\"hi\") }", false).unwrap_err();
        assert!(err.message.contains("unknown function 'print'"));
    }

    #[test]
    fn io_functions_work_when_imported() {
        analyze(
            "fn main() { print(\"hi {}\", 1)\ninput_s()\nset_stream(\"shell\") }",
            true,
        )
        .unwrap();
    }

    #[test]
    fn set_stream_arity() {
        let err = analyze("fn main() { set_stream(\"a\", \"b\", \"c\") }", true).unwrap_err();
        assert!(err.message.contains("optional filename"));
    }

    #[test]
    fn const_cannot_be_reassigned() {
        let err = analyze("fn main() { x = 1<const>\nx = 2 }", false).unwrap_err();
        assert!(err
            .message
            .contains("cannot assign to immutable variable 'x'"));
    }

    #[test]
    fn mutable_can_be_reassigned() {
        analyze("fn main() { x = 1\nx = 2 }", false).unwrap();
    }

    #[test]
    fn cannot_redeclare_const_with_marker() {
        let err = analyze("fn main() { x = 1\nx = 2<const> }", false).unwrap_err();
        assert!(err.message.contains("it already exists"));
    }

    #[test]
    fn const_cannot_be_reassigned_from_inner_scope() {
        // Assignment updates the nearest binding, so an inner scope cannot
        // overwrite an outer immutable variable.
        let err = analyze(
            "fn main() { x = 1<const>\nscope highlevel { x = 2 } }",
            false,
        )
        .unwrap_err();
        assert!(err
            .message
            .contains("cannot assign to immutable variable 'x'"));
    }

    #[test]
    fn inner_scope_updates_outer_mutable() {
        // Dynamic semantics: assignment updates the nearest existing binding.
        analyze(
            "fn main() { x = 1\nscope highlevel { x = 2 }\nscope highlevel { y = x } }",
            false,
        )
        .unwrap();
    }

    #[test]
    fn checks_argument_literal_types() {
        // int 参数收到 string 字面量 -> 编译期报错
        let err = analyze("func add(a<int>): { }\nfn main() { add(\"x\") }", false).unwrap_err();
        assert!(err
            .message
            .contains("argument 1 of 'add' expects int, found string"));
        // 正确类型通过；变量参数不静态检查
        analyze(
            "func add(a<int>): { }\nfn main() { x = 5\nadd(1)\nadd(x) }",
            false,
        )
        .unwrap();
    }

    #[test]
    fn checks_return_literal_types() {
        let err = analyze("func f() -> int: \"abc\"", false).unwrap_err();
        assert!(err
            .message
            .contains("returns string, but declared to return int"));
        analyze("func f() -> int: 1\nfunc g() -> string: \"ok\"", false).unwrap();
    }

    #[test]
    fn operator_operands_are_checked() {
        // Undefined variable inside an operator expression is an error.
        let err = analyze("fn main() { x = a + 1 }", false).unwrap_err();
        assert!(err.message.contains("undefined variable 'a'"));
        // Valid operator expressions pass.
        analyze("fn main() { x = 1 + 2 * 3\ny = x > 1\nz = !y }", false).unwrap();
    }

    #[test]
    fn loop_accumulation_works() {
        analyze(
            "import control\nfn main() { sum = 0\nfor i in 0..5:\n    sum = sum + i }",
            false,
        )
        .unwrap();
    }
}
