//! Recursive-descent parser for the Zero language.
//!
//! Grammar (v0.2):
//! ```text
//! program    := item*
//! item       := import | function
//! import     := "import" (IDENT ("." IDENT | INT)* | STRING) [";"]
//! function   := "fn" IDENT "(" [IDENT ("," IDENT)*] ")" block
//! block      := "{" stmt* "}"
//! stmt       := assign | return | scope | expr [终止符]
//! assign     := IDENT "=" expr
//! return     := "return" [expr]
//! scope      := "scope" ("highlevel" | "lowlevel") block
//! expr       := INT | STRING | IDENT | IDENT "(" [expr ("," expr)*] ")"
//! args       := expr ("," expr)*
//! ```

use crate::ast::*;
use crate::diag::{line_col, CompileError, Span};
use crate::lexer::{scan_raw_block, Lexer, TokKind, Token};

pub struct Parser<'a> {
    src: &'a str,
    file: String,
    lexer: Lexer<'a>,
    lookahead: Vec<Token>,
    /// End offset of the most recently consumed token.
    last_end: usize,
    /// True once `import control` has been seen: control flow keywords
    /// (`if`, `while`, `for`, `each`, `switch`, `try`) become available.
    control_enabled: bool,
}

/// Parse a source file into a program.
pub fn parse(src: &str, file: &str) -> Result<Program, CompileError> {
    Parser::new(src, file).parse_program()
}

impl<'a> Parser<'a> {
    fn new(src: &'a str, file: &str) -> Self {
        Parser {
            src,
            file: file.to_string(),
            lexer: Lexer::new(src),
            lookahead: Vec::new(),
            last_end: 0,
            control_enabled: false,
        }
    }

    /// Ensure at least `upto + 1` tokens are buffered. Lexer-level errors
    /// don't know the file name, so it is filled in here.
    fn fill(&mut self, upto: usize) -> Result<(), CompileError> {
        while self.lookahead.len() <= upto {
            let tok = self.lexer.next_token().map_err(|mut e| {
                if e.file == "<source>" {
                    e.file = self.file.clone();
                }
                e
            })?;
            self.lookahead.push(tok);
        }
        Ok(())
    }

    fn peek(&mut self) -> Result<&Token, CompileError> {
        self.fill(0)?;
        Ok(&self.lookahead[0])
    }

    fn peek2(&mut self) -> Result<&Token, CompileError> {
        self.fill(1)?;
        Ok(&self.lookahead[1])
    }

    fn next(&mut self) -> Result<Token, CompileError> {
        self.fill(0)?;
        let tok = self.lookahead.remove(0);
        if tok.kind != TokKind::Eof {
            self.last_end = tok.span.end;
        }
        Ok(tok)
    }

    fn at(&mut self, kind: &TokKind) -> Result<bool, CompileError> {
        Ok(&self.peek()?.kind == kind)
    }

    fn expect(&mut self, kind: &TokKind) -> Result<Token, CompileError> {
        let tok = self.next()?;
        if &tok.kind != kind {
            return Err(CompileError::at(
                format!("expected {:?}, found {:?}", kind, tok.kind),
                self.file.clone(),
                tok.span,
            ));
        }
        Ok(tok)
    }

    fn expect_ident(&mut self, what: &str) -> Result<(String, Span), CompileError> {
        let tok = self.next()?;
        match tok.kind {
            TokKind::Ident(name) => Ok((name, tok.span)),
            other => Err(CompileError::at(
                format!("expected {what}, found {other:?}"),
                self.file.clone(),
                tok.span,
            )),
        }
    }

    /// Jump the token stream to an absolute byte offset (after raw blocks).
    fn seek(&mut self, pos: usize) {
        self.lookahead.clear();
        self.lexer.seek(pos);
    }

    /// Consume a statement terminator. A statement may end with `;`, with a
    /// newline, or right before `}` / EOF (no terminator needed).
    fn end_stmt(&mut self) -> Result<(), CompileError> {
        if self.at(&TokKind::Semi)? {
            self.next()?;
            return Ok(());
        }
        if self.at(&TokKind::RBrace)? || self.at(&TokKind::Eof)? {
            return Ok(());
        }
        let tok = self.peek()?.clone();
        let (last_line, _) = line_col(self.src, self.last_end);
        let (next_line, _) = line_col(self.src, tok.span.start);
        if next_line > last_line {
            return Ok(()); // newline terminates the statement
        }
        Err(CompileError::at(
            format!("expected ';' or newline, found {:?}", tok.kind),
            self.file.clone(),
            tok.span,
        ))
    }

    fn parse_program(&mut self) -> Result<Program, CompileError> {
        let mut items = Vec::new();
        while !self.at(&TokKind::Eof)? {
            let tok = self.peek()?.clone();
            match tok.kind {
                TokKind::Import => items.push(Item::Import(self.parse_import()?)),
                TokKind::Fn | TokKind::Func => items.push(Item::Function(self.parse_function()?)),
                _ => {
                    // Top-level statements: they form the implicit entry point.
                    let stmt = self.parse_stmt()?;
                    items.push(Item::Top(stmt));
                }
            }
        }
        Ok(Program { items })
    }

    fn parse_import(&mut self) -> Result<Import, CompileError> {
        let kw = self.expect(&TokKind::Import)?;
        let tok = self.next()?;
        let mut end = tok.span.end;
        let name = match tok.kind {
            TokKind::Ident(first) => {
                let mut name = first;
                // Dotted header names: `import std.io`, `import v1.0`
                while self.at(&TokKind::Dot)? {
                    self.next()?;
                    let seg = self.next()?;
                    end = seg.span.end;
                    match seg.kind {
                        TokKind::Ident(s) => {
                            name.push('.');
                            name.push_str(&s);
                        }
                        TokKind::Int(n) => {
                            name.push('.');
                            name.push_str(&n.to_string());
                        }
                        other => {
                            return Err(CompileError::at(
                                format!("expected header name segment, found {other:?}"),
                                self.file.clone(),
                                seg.span,
                            ));
                        }
                    }
                }
                name
            }
            TokKind::Str(s) => s,
            other => {
                return Err(CompileError::at(
                    format!("expected header name, found {other:?}"),
                    self.file.clone(),
                    tok.span,
                ));
            }
        };
        // A top-level import may omit its trailing semicolon.
        if self.at(&TokKind::Semi)? {
            end = self.next()?.span.end;
        }
        if name == "control" {
            self.control_enabled = true;
        }
        Ok(Import {
            name,
            span: Span::new(kw.span.start, end),
        })
    }

    /// 1-based line number of a byte offset.
    fn line_of(&self, offset: usize) -> usize {
        self.src[..offset.min(self.src.len())].matches('\n').count() + 1
    }

    /// Leading whitespace width of the line containing `offset`.
    fn line_indent(&self, offset: usize) -> usize {
        let offset = offset.min(self.src.len());
        let line_start = self.src[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0);
        self.src[line_start..]
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .count()
    }

    /// Body of a control flow statement, after its `:`. Accepts a `{ ... }`
    /// block, a single statement on the same line, or an indented block on
    /// the following lines (no braces needed).
    fn parse_control_body(&mut self) -> Result<Block, CompileError> {
        // The ':' was just consumed, so last_end points at the end of the
        // control flow header line.
        if self.at(&TokKind::LBrace)? {
            let tok = self.peek()?.clone();
            return Err(CompileError::at(
                "control flow bodies are Python-style: use indentation after ':', not { }",
                self.file.clone(),
                tok.span,
            ));
        }
        let colon_line = self.line_of(self.last_end);
        let next = self.peek()?.clone();
        if self.line_of(next.span.start) == colon_line {
            // Single statement on the same line.
            let stmt = self.parse_stmt()?;
            return Ok(Block::Stmts(vec![stmt]));
        }
        // Indented block: statements more indented than the header line.
        let base_indent = self.line_indent(self.last_end);
        let mut stmts = Vec::new();
        loop {
            let tok = self.peek()?.clone();
            if matches!(tok.kind, TokKind::Eof | TokKind::RBrace) {
                break;
            }
            if self.line_indent(tok.span.start) <= base_indent {
                break;
            }
            stmts.push(self.parse_stmt()?);
        }
        Ok(Block::Stmts(stmts))
    }

    fn parse_if(&mut self) -> Result<Stmt, CompileError> {
        self.next()?; // 'if'
        let cond = self.parse_expr()?;
        self.expect(&TokKind::Colon)?;
        let then = self.parse_control_body()?;
        let mut else_ifs = Vec::new();
        let mut else_branch = None;
        loop {
            if self.at(&TokKind::ElseIf)? {
                self.next()?;
                let econd = self.parse_expr()?;
                self.expect(&TokKind::Colon)?;
                let ebody = self.parse_control_body()?;
                else_ifs.push((econd, ebody));
            } else if self.at(&TokKind::Else)? {
                self.next()?;
                self.expect(&TokKind::Colon)?;
                let ebody = self.parse_control_body()?;
                else_branch = Some(ebody);
                break;
            } else {
                break;
            }
        }
        Ok(Stmt::If {
            cond,
            then,
            else_ifs,
            else_branch,
        })
    }

    fn parse_while(&mut self) -> Result<Stmt, CompileError> {
        self.next()?; // 'while'
        let cond = self.parse_expr()?;
        self.expect(&TokKind::Colon)?;
        let body = self.parse_control_body()?;
        Ok(Stmt::While { cond, body })
    }

    fn parse_for(&mut self) -> Result<Stmt, CompileError> {
        self.next()?; // 'for'
        let (var, _) = self.expect_ident("loop variable")?;
        self.expect(&TokKind::In)?;
        let start = self.parse_expr()?;
        let inclusive = if self.at(&TokKind::DotDotEq)? {
            self.next()?;
            true
        } else if self.at(&TokKind::DotDot)? {
            self.next()?;
            false
        } else {
            let tok = self.peek()?.clone();
            return Err(CompileError::at(
                format!("expected '..' or '..=' in for loop, found {:?}", tok.kind),
                self.file.clone(),
                tok.span,
            ));
        };
        let end = self.parse_expr()?;
        self.expect(&TokKind::Colon)?;
        let body = self.parse_control_body()?;
        Ok(Stmt::For {
            var,
            start,
            end,
            inclusive,
            body,
        })
    }

    fn parse_each(&mut self) -> Result<Stmt, CompileError> {
        self.next()?; // 'each'
        let (var, _) = self.expect_ident("loop variable")?;
        self.expect(&TokKind::In)?;
        let iter = self.parse_expr()?;
        self.expect(&TokKind::Colon)?;
        let body = self.parse_control_body()?;
        Ok(Stmt::Each { var, iter, body })
    }

    fn parse_switch(&mut self) -> Result<Stmt, CompileError> {
        let kw = self.next()?; // 'switch'
        let base_indent = self.line_indent(kw.span.start);
        let value = self.parse_expr()?;
        self.expect(&TokKind::Colon)?;
        let mut arms = Vec::new();
        let mut default = None;

        let in_braces = self.at(&TokKind::LBrace)?;
        if in_braces {
            self.next()?; // '{'
        }
        loop {
            if self.at(&TokKind::Case)? {
                let ck = self.next()?;
                let cbase = self.line_indent(ck.span.start);
                let arm_val = self.parse_expr()?;
                self.expect(&TokKind::Colon)?;
                let arm_body = self.parse_control_body_with_base(cbase)?;
                arms.push((arm_val, arm_body));
            } else if self.at(&TokKind::Default)? {
                let dk = self.next()?;
                let dbase = self.line_indent(dk.span.start);
                self.expect(&TokKind::Colon)?;
                let dbody = self.parse_control_body_with_base(dbase)?;
                default = Some(dbody);
                break;
            } else {
                break;
            }
            if in_braces && self.at(&TokKind::RBrace)? {
                break;
            }
            if !in_braces {
                let p = self.peek()?.clone();
                if self.line_indent(p.span.start) <= base_indent {
                    break;
                }
            }
        }
        if in_braces {
            self.expect(&TokKind::RBrace)?;
        }
        Ok(Stmt::Switch {
            value,
            arms,
            default,
        })
    }

    fn parse_try(&mut self) -> Result<Stmt, CompileError> {
        self.next()?; // 'try'
        self.expect(&TokKind::Colon)?;
        let body = self.parse_control_body()?;
        let mut catch_var = None;
        let mut catch_body = None;
        if self.at(&TokKind::Catch)? {
            self.next()?;
            if matches!(&self.peek()?.kind, TokKind::Ident(_))
                && matches!(&self.peek2()?.kind, TokKind::Colon)
            {
                catch_var = Some(self.expect_ident("catch variable")?.0);
            }
            self.expect(&TokKind::Colon)?;
            catch_body = Some(self.parse_control_body()?);
        }
        Ok(Stmt::Try {
            body,
            catch_var,
            catch_body,
        })
    }

    /// Like `parse_control_body` but with an explicit base indentation (for
    /// `case` / `default` arms, whose header sits at their own indent level).
    fn parse_control_body_with_base(&mut self, base_indent: usize) -> Result<Block, CompileError> {
        if self.at(&TokKind::LBrace)? {
            let tok = self.peek()?.clone();
            return Err(CompileError::at(
                "control flow bodies are Python-style: use indentation after ':', not { }",
                self.file.clone(),
                tok.span,
            ));
        }
        let colon_line = self.line_of(self.last_end);
        let next = self.peek()?.clone();
        if self.line_of(next.span.start) == colon_line {
            let stmt = self.parse_stmt()?;
            return Ok(Block::Stmts(vec![stmt]));
        }
        let mut stmts = Vec::new();
        loop {
            let tok = self.peek()?.clone();
            if matches!(tok.kind, TokKind::Eof | TokKind::RBrace) {
                break;
            }
            if self.line_indent(tok.span.start) <= base_indent {
                break;
            }
            stmts.push(self.parse_stmt()?);
        }
        Ok(Block::Stmts(stmts))
    }

    fn parse_function(&mut self) -> Result<Function, CompileError> {
        let kw = self.next()?;
        let is_func = kw.kind == TokKind::Func;
        let (name, name_span) = self.expect_ident("function name")?;
        self.expect(&TokKind::LParen)?;
        let mut params = Vec::new();
        if !self.at(&TokKind::RParen)? {
            loop {
                let (pname, _) = self.expect_ident("parameter name")?;
                let ty = self.parse_opt_type()?;
                params.push(Param { name: pname, ty });
                if self.at(&TokKind::Comma)? {
                    self.next()?;
                } else {
                    break;
                }
            }
        }
        self.expect(&TokKind::RParen)?;
        let ret = self.parse_opt_return_type()?;
        let body = if is_func {
            // `func name(...): code` - colon body, either a block or a
            // single expression that is implicitly returned.
            self.expect(&TokKind::Colon)?;
            if self.at(&TokKind::LBrace)? {
                self.parse_block()?
            } else {
                let expr = self.parse_expr()?;
                Block::Stmts(vec![Stmt::Return { value: Some(expr) }])
            }
        } else {
            self.parse_block()?
        };
        Ok(Function {
            name,
            params,
            ret,
            body,
            span: Span::new(kw.span.start, name_span.end),
        })
    }

    /// Optional parameter type: `x<int>` -> `Some(ZType::Int)`. The
    /// explicit `any` type is dynamic and normalised to `None`.
    fn parse_opt_type(&mut self) -> Result<Option<ZType>, CompileError> {
        if self.at(&TokKind::Lt)? {
            self.next()?;
            let ty = self.parse_type_name()?;
            self.expect(&TokKind::Gt)?;
            Ok(ty)
        } else {
            Ok(None)
        }
    }

    /// Optional return type: `-> int` (or `-> any`, same as omitted).
    fn parse_opt_return_type(&mut self) -> Result<Option<ZType>, CompileError> {
        if self.at(&TokKind::Arrow)? {
            self.next()?;
            self.parse_type_name()
        } else {
            Ok(None)
        }
    }

    fn parse_type_name(&mut self) -> Result<Option<ZType>, CompileError> {
        let tok = self.next()?;
        match tok.kind {
            TokKind::Ident(name) => match name.as_str() {
                "any" => Ok(None),
                _ => ZType::from_name(&name).map(Some).ok_or_else(|| {
                    CompileError::at(
                        format!("unknown type '{name}' (expected int, float, string, bool or any)"),
                        self.file.clone(),
                        tok.span,
                    )
                }),
            },
            other => Err(CompileError::at(
                format!("expected type name, found {other:?}"),
                self.file.clone(),
                tok.span,
            )),
        }
    }

    fn parse_block(&mut self) -> Result<Block, CompileError> {
        let open = self.expect(&TokKind::LBrace)?;
        let block = self.parse_block_rest(open.span.end)?;
        Ok(block)
    }

    /// Continue parsing a block whose opening `{` has already been consumed.
    fn parse_block_rest(&mut self, _open_end: usize) -> Result<Block, CompileError> {
        let mut stmts = Vec::new();
        while !self.at(&TokKind::RBrace)? && !self.at(&TokKind::Eof)? {
            stmts.push(self.parse_stmt()?);
        }
        self.expect(&TokKind::RBrace)?;
        Ok(Block::Stmts(stmts))
    }

    fn parse_stmt(&mut self) -> Result<Stmt, CompileError> {
        // `name = expr` (or `+=` / `-=` / `*=`) is a dynamic variable
        // assignment/creation. Compound forms desugar in parse_assign.
        let is_assign = matches!(self.peek()?.kind, TokKind::Ident(_))
            && matches!(
                self.peek2()?.kind,
                TokKind::Equal | TokKind::PlusEq | TokKind::MinusEq | TokKind::StarEq
            );
        if is_assign {
            return self.parse_assign();
        }

        // `break` / `continue` are only meaningful inside loops.
        if matches!(self.peek()?.kind, TokKind::Break | TokKind::Continue) {
            if !self.control_enabled {
                return Err(CompileError::at(
                    "'break' / 'continue' require 'import control'",
                    self.file.clone(),
                    self.peek()?.span,
                ));
            }
            let tok = self.next()?;
            self.end_stmt()?;
            return Ok(match tok.kind {
                TokKind::Break => Stmt::Break { span: tok.span },
                _ => Stmt::Continue { span: tok.span },
            });
        }

        let tok = self.peek()?.clone();
        match tok.kind {
            TokKind::Return => {
                self.next()?;
                let value = if self.at(&TokKind::Semi)? || self.at(&TokKind::RBrace)? {
                    None
                } else {
                    Some(self.parse_expr()?)
                };
                self.end_stmt()?;
                Ok(Stmt::Return { value })
            }
            TokKind::Scope => {
                let kw = self.next()?;
                let scope_tok = self.next()?;
                let kind = match scope_tok.kind {
                    TokKind::HighLevel => ScopeKind::HighLevel,
                    TokKind::LowLevel => ScopeKind::LowLevel,
                    other => {
                        return Err(CompileError::at(
                            format!("expected 'highlevel' or 'lowlevel', found {other:?}"),
                            self.file.clone(),
                            scope_tok.span,
                        ));
                    }
                };
                let open = self.expect(&TokKind::LBrace)?;
                let block = match kind {
                    ScopeKind::HighLevel => self.parse_block_rest(open.span.end)?,
                    ScopeKind::LowLevel => {
                        let (raw, end) =
                            scan_raw_block(self.src, open.span.start).map_err(|e| {
                                CompileError {
                                    message: e.message,
                                    span: e.span,
                                    file: self.file.clone(),
                                }
                            })?;
                        self.seek(end);
                        Block::Raw(raw)
                    }
                };
                Ok(Stmt::Scope {
                    kind,
                    block,
                    span: Span::new(kw.span.start, open.span.end),
                })
            }
            TokKind::If
            | TokKind::While
            | TokKind::For
            | TokKind::Each
            | TokKind::Switch
            | TokKind::Try => {
                if !self.control_enabled {
                    return Err(CompileError::at(
                        "control flow requires 'import control' at the top of the file",
                        self.file.clone(),
                        tok.span,
                    ));
                }
                match tok.kind {
                    TokKind::If => self.parse_if(),
                    TokKind::While => self.parse_while(),
                    TokKind::For => self.parse_for(),
                    TokKind::Each => self.parse_each(),
                    TokKind::Switch => self.parse_switch(),
                    TokKind::Try => self.parse_try(),
                    _ => unreachable!(),
                }
            }
            TokKind::Else | TokKind::ElseIf => Err(CompileError::at(
                "unexpected 'else' / 'else_if' without a matching 'if'",
                self.file.clone(),
                tok.span,
            )),
            TokKind::Case | TokKind::Default => Err(CompileError::at(
                "unexpected 'case' / 'default' outside a 'switch'",
                self.file.clone(),
                tok.span,
            )),
            TokKind::Catch => Err(CompileError::at(
                "unexpected 'catch' without a matching 'try'",
                self.file.clone(),
                tok.span,
            )),
            _ => {
                let expr = self.parse_expr()?;
                self.end_stmt()?;
                Ok(Stmt::Expr(expr))
            }
        }
    }

    fn parse_assign(&mut self) -> Result<Stmt, CompileError> {
        let (name, name_span) = self.expect_ident("variable name")?;
        let op_tok = self.next()?;
        let op = match op_tok.kind {
            TokKind::Equal => None,
            TokKind::PlusEq => Some(BinOp::Add),
            TokKind::MinusEq => Some(BinOp::Sub),
            TokKind::StarEq => Some(BinOp::Mul),
            other => {
                return Err(CompileError::at(
                    format!("expected assignment operator, found {other:?}"),
                    self.file.clone(),
                    op_tok.span,
                ));
            }
        };
        let rhs = self.parse_expr()?;
        // Compound forms desugar to `name = name op rhs`, so the scope
        // analyzer and codegen handle them with no extra machinery.
        let value = match op {
            None => rhs,
            Some(bop) => {
                let span = Span::new(name_span.start, rhs.span().end);
                Expr::Binary {
                    op: bop,
                    lhs: Box::new(Expr::Ident(name.clone(), name_span)),
                    rhs: Box::new(rhs),
                    span,
                }
            }
        };
        // Optional immutability marker: `name = expr<const>`
        let (is_const, span) = if self.at(&TokKind::Lt)? {
            self.next()?;
            self.expect(&TokKind::Const)?;
            let gt = self.expect(&TokKind::Gt)?;
            (true, Span::new(name_span.start, gt.span.end))
        } else {
            (false, name_span)
        };
        self.end_stmt()?;
        Ok(Stmt::Assign {
            name,
            value,
            is_const,
            span,
        })
    }

    fn parse_expr(&mut self) -> Result<Expr, CompileError> {
        self.parse_binary(0)
    }

    /// Pratt parser: left-associative binary operators with precedence.
    fn parse_binary(&mut self, min_prec: u8) -> Result<Expr, CompileError> {
        let mut lhs = self.parse_unary()?;
        while let Some((op, prec)) = self.peek_binop()? {
            if prec < min_prec {
                break;
            }
            self.next()?;
            let rhs = self.parse_binary(prec + 1)?;
            let span = Span::new(lhs.span().start, rhs.span().end);
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }

    /// Look at the next operator and its precedence, if any. `<const>` is
    /// a variable marker, not a comparison, so `<` right before the
    /// keyword `const` is not treated as an operator.
    fn peek_binop(&mut self) -> Result<Option<(BinOp, u8)>, CompileError> {
        let kind = self.peek()?.kind.clone();
        let (op, prec) = match &kind {
            TokKind::Plus => (BinOp::Add, 5),
            TokKind::Minus => (BinOp::Sub, 5),
            TokKind::Star => (BinOp::Mul, 6),
            TokKind::Slash => (BinOp::Div, 6),
            TokKind::Percent => (BinOp::Rem, 6),
            TokKind::EqEq => (BinOp::Eq, 3),
            TokKind::BangEq => (BinOp::Ne, 3),
            TokKind::Lt => {
                if matches!(&self.peek2()?.kind, TokKind::Const) {
                    return Ok(None);
                }
                (BinOp::Lt, 4)
            }
            TokKind::Gt => (BinOp::Gt, 4),
            TokKind::Le => (BinOp::Le, 4),
            TokKind::Ge => (BinOp::Ge, 4),
            TokKind::And => (BinOp::And, 2),
            TokKind::Or => (BinOp::Or, 1),
            _ => return Ok(None),
        };
        Ok(Some((op, prec)))
    }

    fn parse_unary(&mut self) -> Result<Expr, CompileError> {
        let tok = self.peek()?.clone();
        let (op, operand) = match tok.kind {
            TokKind::Minus => {
                self.next()?;
                (UnOp::Neg, self.parse_unary()?)
            }
            TokKind::Bang => {
                self.next()?;
                (UnOp::Not, self.parse_unary()?)
            }
            _ => return self.parse_primary(),
        };
        let end = operand.span().end;
        Ok(Expr::Unary {
            op,
            operand: Box::new(operand),
            span: Span::new(tok.span.start, end),
        })
    }

    fn parse_primary(&mut self) -> Result<Expr, CompileError> {
        let tok = self.next()?;
        let expr = match tok.kind {
            TokKind::Int(v) => Expr::Int(v, tok.span),
            TokKind::Float(v) => Expr::Float(v, tok.span),
            TokKind::Str(s) => Expr::Str(s, tok.span),
            TokKind::True => Expr::Bool(true, tok.span),
            TokKind::False => Expr::Bool(false, tok.span),
            TokKind::Null => Expr::Nil(tok.span),
            TokKind::Ident(name) => {
                if name == "type_to" && self.at(&TokKind::Lt)? {
                    // `type_to<int>(expr)` — explicit type conversion.
                    self.next()?; // '<'
                    let ty = self.parse_type_name()?.ok_or_else(|| {
                        CompileError::at(
                            "type_to requires a concrete type (int, float, string or bool)",
                            self.file.clone(),
                            tok.span,
                        )
                    })?;
                    self.expect(&TokKind::Gt)?;
                    self.expect(&TokKind::LParen)?;
                    let value = self.parse_expr()?;
                    let close = self.expect(&TokKind::RParen)?;
                    Expr::TypeTo {
                        ty,
                        value: Box::new(value),
                        span: Span::new(tok.span.start, close.span.end),
                    }
                } else if self.at(&TokKind::LParen)? {
                    self.next()?;
                    let mut args = Vec::new();
                    if !self.at(&TokKind::RParen)? {
                        loop {
                            args.push(self.parse_expr()?);
                            if self.at(&TokKind::Comma)? {
                                self.next()?;
                            } else {
                                break;
                            }
                        }
                    }
                    let close = self.expect(&TokKind::RParen)?;
                    Expr::Call {
                        callee: name,
                        args,
                        span: Span::new(tok.span.start, close.span.end),
                    }
                } else {
                    Expr::Ident(name, tok.span)
                }
            }
            TokKind::LParen => {
                let inner = self.parse_expr()?;
                self.expect(&TokKind::RParen)?;
                inner
            }
            other => {
                return Err(CompileError::at(
                    format!("expected expression, found {other:?}"),
                    self.file.clone(),
                    tok.span,
                ));
            }
        };
        Ok(expr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_minimal_program() {
        let prog = parse(
            "import stdio\nfn main() { call_sys(\"echo hi\") }",
            "test.zero",
        )
        .unwrap();
        assert!(prog.has_main());
        assert_eq!(prog.items.len(), 2);
    }

    #[test]
    fn parses_function_parameters() {
        let prog = parse("fn add(a, b) { return a } fn main() {}", "test.zero").unwrap();
        let Item::Function(f) = &prog.items[0] else {
            panic!("expected function");
        };
        assert_eq!(f.params.len(), 2);
        assert_eq!(f.params[0].name, "a");
        assert_eq!(f.params[1].name, "b");
        assert!(f.params.iter().all(|p| p.ty.is_none()));
    }

    #[test]
    fn parses_func_with_types() {
        let prog = parse(
            "func add(a<int>, b) -> int: a + b\nfunc main(): { }",
            "test.zero",
        )
        .unwrap();
        let Item::Function(f) = &prog.items[0] else {
            panic!("expected function");
        };
        assert_eq!(f.params[0].name, "a");
        assert_eq!(f.params[0].ty, Some(ZType::Int));
        assert_eq!(f.params[1].ty, None);
        assert_eq!(f.ret, Some(ZType::Int));
        // 单表达式体被包装成隐式 return
        let Block::Stmts(stmts) = &f.body else {
            panic!("expected stmts");
        };
        assert!(matches!(&stmts[0], Stmt::Return { value: Some(..) }));
    }

    #[test]
    fn any_type_normalised_to_dynamic() {
        let prog = parse("func f(a<any>, b) -> any: a", "test.zero").unwrap();
        let Item::Function(f) = &prog.items[0] else {
            panic!("expected function");
        };
        assert!(f.params[0].ty.is_none());
        assert!(f.params[1].ty.is_none());
        assert!(f.ret.is_none());
    }

    #[test]
    fn rejects_unknown_type() {
        let err = parse("func f(x<floats>): { }", "test.zero").unwrap_err();
        assert!(err.message.contains("unknown type 'floats'"));
    }

    #[test]
    fn parses_variable_assignment() {
        let prog = parse("fn main() { x = 1\ny = \"hi\" }", "test.zero").unwrap();
        let Item::Function(f) = &prog.items[0] else {
            panic!("expected function");
        };
        let Block::Stmts(stmts) = &f.body else {
            panic!("expected stmts");
        };
        assert!(matches!(&stmts[0], Stmt::Assign { name, .. } if name == "x"));
        assert!(matches!(&stmts[1], Stmt::Assign { name, .. } if name == "y"));
    }

    #[test]
    fn parses_const_marker() {
        let prog = parse("fn main() { pi = 3<const> }", "test.zero").unwrap();
        let Item::Function(f) = &prog.items[0] else {
            panic!("expected function");
        };
        let Block::Stmts(stmts) = &f.body else {
            panic!("expected stmts");
        };
        let Stmt::Assign { name, is_const, .. } = &stmts[0] else {
            panic!("expected assign");
        };
        assert_eq!(name, "pi");
        assert!(is_const);
    }

    #[test]
    fn parses_lowlevel_raw_block() {
        let prog = parse(
            "fn main() { scope lowlevel { let x = 1; println!(\"{{}}\", x); } }",
            "test.zero",
        )
        .unwrap();
        let Item::Function(f) = &prog.items[0] else {
            panic!("expected function");
        };
        let Block::Stmts(stmts) = &f.body else {
            panic!("expected stmts");
        };
        let Stmt::Scope { kind, block, .. } = &stmts[0] else {
            panic!("expected scope");
        };
        assert_eq!(*kind, ScopeKind::LowLevel);
        let Block::Raw(text) = block else {
            panic!("expected raw");
        };
        assert!(text.contains("println!"));
    }

    #[test]
    fn parses_operator_precedence() {
        let prog = parse("fn main() { x = 1 + 2 * 3 }", "test.zero").unwrap();
        let Item::Function(f) = &prog.items[0] else {
            panic!("expected function");
        };
        let Block::Stmts(stmts) = &f.body else {
            panic!("expected stmts");
        };
        let Stmt::Assign { value, .. } = &stmts[0] else {
            panic!("expected assign");
        };
        let Expr::Binary { op, lhs, rhs, .. } = value else {
            panic!("expected binary");
        };
        assert_eq!(*op, BinOp::Add);
        assert!(matches!(&**lhs, Expr::Int(1, _)));
        assert!(matches!(
            &**rhs,
            Expr::Binary { op: mop, .. } if *mop == BinOp::Mul
        ));
    }

    #[test]
    fn parses_parens_and_unary() {
        let prog = parse("fn main() { x = (1 + 2) * -3\ny = !flag }", "test.zero").unwrap();
        let Item::Function(f) = &prog.items[0] else {
            panic!("expected function");
        };
        let Block::Stmts(stmts) = &f.body else {
            panic!("expected stmts");
        };
        let Stmt::Assign { value, .. } = &stmts[0] else {
            panic!("expected assign");
        };
        let Expr::Binary { op, lhs, .. } = value else {
            panic!("expected binary");
        };
        assert_eq!(*op, BinOp::Mul);
        let Expr::Binary { op: aop, .. } = &**lhs else {
            panic!("expected inner add");
        };
        assert_eq!(*aop, BinOp::Add);
        let Stmt::Assign { value: v2, .. } = &stmts[1] else {
            panic!("expected assign 2");
        };
        let Expr::Unary { op: uop, .. } = v2 else {
            panic!("expected unary");
        };
        assert_eq!(*uop, UnOp::Not);
    }

    #[test]
    fn const_marker_not_a_comparison() {
        let prog = parse("fn main() { x = 1<const> }", "test.zero").unwrap();
        let Item::Function(f) = &prog.items[0] else {
            panic!("expected function");
        };
        let Block::Stmts(stmts) = &f.body else {
            panic!("expected stmts");
        };
        let Stmt::Assign {
            value, is_const, ..
        } = &stmts[0]
        else {
            panic!("expected assign");
        };
        assert!(*is_const);
        assert!(matches!(value, Expr::Int(1, _)));
    }

    #[test]
    fn comparison_inside_expression() {
        let prog = parse("fn main() { x = a < b }", "test.zero").unwrap();
        let Item::Function(f) = &prog.items[0] else {
            panic!("expected function");
        };
        let Block::Stmts(stmts) = &f.body else {
            panic!("expected stmts");
        };
        let Stmt::Assign {
            value, is_const, ..
        } = &stmts[0]
        else {
            panic!("expected assign");
        };
        assert!(!is_const);
        let Expr::Binary { op, .. } = value else {
            panic!("expected comparison");
        };
        assert_eq!(*op, BinOp::Lt);
    }

    #[test]
    fn parses_bool_literals() {
        let prog = parse("fn main() { t = true\nf = false }", "test.zero").unwrap();
        let Item::Function(f) = &prog.items[0] else {
            panic!("expected function");
        };
        let Block::Stmts(stmts) = &f.body else {
            panic!("expected stmts");
        };
        let Stmt::Assign { value, .. } = &stmts[0] else {
            panic!("expected assign");
        };
        assert!(matches!(value, Expr::Bool(true, _)));
        let Stmt::Assign { value: v2, .. } = &stmts[1] else {
            panic!("expected assign 2");
        };
        assert!(matches!(v2, Expr::Bool(false, _)));
    }

    #[test]
    fn allows_top_level_statements() {
        // Top-level statements form the implicit entry point (no main).
        let prog = parse("call_sys(\"x\")\nx = 1", "test.zero").unwrap();
        assert!(matches!(&prog.items[0], Item::Top(_)));
        assert!(matches!(&prog.items[1], Item::Top(_)));
    }

    #[test]
    fn rejects_brace_control_flow() {
        // Python-style: control flow uses ':' + indentation, not { }.
        let err = parse("import control\nfn main() { if 1: { x = 1 } }", "test.zero").unwrap_err();
        assert!(err.message.contains("Python-style"));
    }

    #[test]
    fn parses_float_literal() {
        let prog = parse("fn main() { x = 3.14 }", "test.zero").unwrap();
        let Item::Function(f) = &prog.items[0] else {
            panic!("expected function")
        };
        let Block::Stmts(stmts) = &f.body else {
            panic!("expected stmts")
        };
        let Stmt::Assign { value, .. } = &stmts[0] else {
            panic!("expected assign")
        };
        assert!(matches!(value, Expr::Float(v, _) if (*v - 3.14).abs() < 1e-9));
    }

    #[test]
    fn parses_null_literal() {
        let prog = parse("fn main() { x = NULL }", "test.zero").unwrap();
        let Item::Function(f) = &prog.items[0] else {
            panic!("expected function")
        };
        let Block::Stmts(stmts) = &f.body else {
            panic!("expected stmts")
        };
        let Stmt::Assign { value, .. } = &stmts[0] else {
            panic!("expected assign")
        };
        assert!(matches!(value, Expr::Nil(_)));
    }

    #[test]
    fn compound_assignment_desugars() {
        let prog = parse("fn main() { x = 1\nx += 2\nx -= 1\nx *= 3 }", "test.zero").unwrap();
        let Item::Function(f) = &prog.items[0] else {
            panic!("expected function")
        };
        let Block::Stmts(stmts) = &f.body else {
            panic!("expected stmts")
        };
        let ops: Vec<BinOp> = stmts[1..]
            .iter()
            .filter_map(|s| match s {
                Stmt::Assign {
                    value: Expr::Binary { op, lhs, .. },
                    ..
                } => {
                    // lhs must re-reference the same variable
                    let Expr::Ident(name, _) = &**lhs else {
                        panic!("expected ident lhs")
                    };
                    assert_eq!(name, "x");
                    Some(*op)
                }
                _ => None,
            })
            .collect();
        assert_eq!(ops, vec![BinOp::Add, BinOp::Sub, BinOp::Mul]);
    }

    #[test]
    fn parses_type_to() {
        let prog = parse("fn main() { x = type_to<int>(\"42\") }", "test.zero").unwrap();
        let Item::Function(f) = &prog.items[0] else {
            panic!("expected function")
        };
        let Block::Stmts(stmts) = &f.body else {
            panic!("expected stmts")
        };
        let Stmt::Assign { value, .. } = &stmts[0] else {
            panic!("expected assign")
        };
        let Expr::TypeTo {
            ty, value: inner, ..
        } = value
        else {
            panic!("expected type_to")
        };
        assert_eq!(*ty, ZType::Int);
        assert!(matches!(&**inner, Expr::Str(s, _) if s == "42"));
    }

    #[test]
    fn parses_break_continue() {
        let prog = parse(
            "import control\nfn main() { while 1:\n    break\n    continue }",
            "test.zero",
        )
        .unwrap();
        let Item::Function(f) = prog
            .items
            .iter()
            .find(|i| matches!(i, Item::Function(_)))
            .expect("expected function")
        else {
            panic!()
        };
        let Block::Stmts(stmts) = &f.body else {
            panic!("expected stmts")
        };
        let Stmt::While { body, .. } = &stmts[0] else {
            panic!("expected while")
        };
        let Block::Stmts(inner) = body else {
            panic!("expected stmts")
        };
        assert!(matches!(inner[0], Stmt::Break { .. }));
        assert!(matches!(inner[1], Stmt::Continue { .. }));
    }
}
