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
        }
    }

    /// Ensure at least `upto + 1` tokens are buffered.
    fn fill(&mut self, upto: usize) -> Result<(), CompileError> {
        while self.lookahead.len() <= upto {
            self.lookahead.push(self.lexer.next_token()?);
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
                TokKind::Fn => items.push(Item::Function(self.parse_function()?)),
                _ => {
                    return Err(CompileError::at(
                        format!(
                            "expected 'import' or 'fn' at top level, found {:?}",
                            tok.kind
                        ),
                        self.file.clone(),
                        tok.span,
                    ));
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
        Ok(Import {
            name,
            span: Span::new(kw.span.start, end),
        })
    }

    fn parse_function(&mut self) -> Result<Function, CompileError> {
        let kw = self.expect(&TokKind::Fn)?;
        let (name, name_span) = self.expect_ident("function name")?;
        self.expect(&TokKind::LParen)?;
        let mut params = Vec::new();
        if !self.at(&TokKind::RParen)? {
            loop {
                let (param, _) = self.expect_ident("parameter name")?;
                params.push(param);
                if self.at(&TokKind::Comma)? {
                    self.next()?;
                } else {
                    break;
                }
            }
        }
        self.expect(&TokKind::RParen)?;
        let block = self.parse_block()?;
        Ok(Function {
            name,
            params,
            body: block,
            span: Span::new(kw.span.start, name_span.end),
        })
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
        // `name = expr` is a dynamic variable assignment/creation.
        let is_assign = matches!(self.peek()?.kind, TokKind::Ident(_))
            && matches!(self.peek2()?.kind, TokKind::Equal);
        if is_assign {
            return self.parse_assign();
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
            _ => {
                let expr = self.parse_expr()?;
                self.end_stmt()?;
                Ok(Stmt::Expr(expr))
            }
        }
    }

    fn parse_assign(&mut self) -> Result<Stmt, CompileError> {
        let (name, name_span) = self.expect_ident("variable name")?;
        self.expect(&TokKind::Equal)?;
        let value = self.parse_expr()?;
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
        let tok = self.next()?;
        let expr = match tok.kind {
            TokKind::Int(v) => Expr::Int(v, tok.span),
            TokKind::Str(s) => Expr::Str(s, tok.span),
            TokKind::Ident(name) => {
                if self.at(&TokKind::LParen)? {
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
        assert_eq!(f.params, vec!["a".to_string(), "b".to_string()]);
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
    fn rejects_stray_top_level_statement() {
        let err = parse("call_sys(\"x\")", "test.zero").unwrap_err();
        assert!(err.message.contains("expected 'import' or 'fn'"));
    }
}
