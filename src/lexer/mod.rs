//! Tokenizer for the Zero language.
//!
//! The lexer is pull-based so the parser can jump over raw `lowlevel`
//! blocks (which contain unparsed Rust code) without tokenizing them.

mod raw;
mod token;

pub use raw::scan_raw_block;
pub use token::{TokKind, Token};

use crate::diag::{CompileError, Span};

pub struct Lexer<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Lexer { src, pos: 0 }
    }

    /// Move the lexer to an absolute byte offset (used after raw blocks).
    pub fn seek(&mut self, pos: usize) {
        self.pos = pos.min(self.src.len());
    }

    fn peek_char(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let ch = self.peek_char()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn skip_trivia(&mut self) -> Result<(), CompileError> {
        loop {
            match self.peek_char() {
                Some(c) if c.is_whitespace() => {
                    self.bump();
                }
                Some('/') => {
                    let next = self.src[self.pos + 1..].chars().next();
                    match next {
                        Some('/') => {
                            while let Some(c) = self.peek_char() {
                                if c == '\n' {
                                    break;
                                }
                                self.bump();
                            }
                        }
                        Some('*') => {
                            let start = self.pos;
                            self.bump();
                            self.bump();
                            let mut depth = 1usize;
                            while depth > 0 {
                                match self.bump() {
                                    Some('/') if self.peek_char() == Some('*') => {
                                        self.bump();
                                        depth += 1;
                                    }
                                    Some('*') if self.peek_char() == Some('/') => {
                                        self.bump();
                                        depth -= 1;
                                    }
                                    Some(_) => {}
                                    None => {
                                        return Err(CompileError::at(
                                            "unterminated block comment",
                                            "<source>",
                                            Span::new(start, self.pos),
                                        ));
                                    }
                                }
                            }
                        }
                        _ => break,
                    }
                }
                _ => break,
            }
        }
        Ok(())
    }

    fn lex_string(&mut self, start: usize) -> Result<TokKind, CompileError> {
        self.bump(); // opening quote
        let mut value = String::new();
        loop {
            match self.bump() {
                Some('"') => {
                    return Ok(TokKind::Str(value));
                }
                Some('\\') => {
                    let esc = self.bump().ok_or_else(|| {
                        CompileError::at(
                            "unterminated string literal",
                            "<source>",
                            Span::new(start, self.pos),
                        )
                    })?;
                    match esc {
                        '"' => value.push('"'),
                        '\\' => value.push('\\'),
                        'n' => value.push('\n'),
                        'r' => value.push('\r'),
                        't' => value.push('\t'),
                        '0' => value.push('\0'),
                        other => {
                            return Err(CompileError::at(
                                format!("unknown escape sequence '\\{other}'"),
                                "<source>",
                                Span::new(start, self.pos),
                            ));
                        }
                    }
                }
                Some('\n') => {
                    return Err(CompileError::at(
                        "unterminated string literal",
                        "<source>",
                        Span::new(start, self.pos),
                    ));
                }
                Some(c) => value.push(c),
                None => {
                    return Err(CompileError::at(
                        "unterminated string literal",
                        "<source>",
                        Span::new(start, self.pos),
                    ));
                }
            }
        }
    }

    fn lex_number(&mut self, start: usize) -> Result<TokKind, CompileError> {
        let mut text = String::new();
        if self.src[self.pos..].starts_with('-') {
            text.push('-');
            self.bump();
        }
        while let Some(c) = self.peek_char() {
            if c.is_ascii_digit() {
                text.push(c);
                self.bump();
            } else {
                break;
            }
        }
        match text.parse::<i64>() {
            Ok(v) => Ok(TokKind::Int(v)),
            Err(_) => Err(CompileError::at(
                "integer literal is out of range",
                "<source>",
                Span::new(start, self.pos),
            )),
        }
    }

    fn lex_ident(&mut self) -> TokKind {
        let start = self.pos;
        while let Some(c) = self.peek_char() {
            if c.is_alphanumeric() || c == '_' {
                self.bump();
            } else {
                break;
            }
        }
        let word = &self.src[start..self.pos];
        match word {
            "fn" => TokKind::Fn,
            "return" => TokKind::Return,
            "import" => TokKind::Import,
            "scope" => TokKind::Scope,
            "highlevel" => TokKind::HighLevel,
            "lowlevel" => TokKind::LowLevel,
            "const" => TokKind::Const,
            _ => TokKind::Ident(word.to_string()),
        }
    }

    /// Produce the next token.
    pub fn next_token(&mut self) -> Result<Token, CompileError> {
        self.skip_trivia()?;
        let start = self.pos;
        let Some(c) = self.peek_char() else {
            return Ok(Token {
                kind: TokKind::Eof,
                span: Span::new(start, start),
            });
        };
        let kind = match c {
            '{' => {
                self.bump();
                TokKind::LBrace
            }
            '}' => {
                self.bump();
                TokKind::RBrace
            }
            '(' => {
                self.bump();
                TokKind::LParen
            }
            ')' => {
                self.bump();
                TokKind::RParen
            }
            ';' => {
                self.bump();
                TokKind::Semi
            }
            ',' => {
                self.bump();
                TokKind::Comma
            }
            '.' => {
                self.bump();
                TokKind::Dot
            }
            '<' => {
                self.bump();
                TokKind::Lt
            }
            '>' => {
                self.bump();
                TokKind::Gt
            }
            '=' => {
                self.bump();
                TokKind::Equal
            }
            '"' => self.lex_string(start)?,
            '-' | '0'..='9' => self.lex_number(start)?,
            c if c.is_alphabetic() || c == '_' => self.lex_ident(),
            other => {
                let end = start + other.len_utf8();
                return Err(CompileError::at(
                    format!("unexpected character '{other}'"),
                    "<source>",
                    Span::new(start, end),
                ));
            }
        };
        Ok(Token {
            kind,
            span: Span::new(start, self.pos),
        })
    }
}

/// Skip a string literal starting at `pos` (the opening quote) and return
/// the byte offset just after the closing quote.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_keywords_and_literals() {
        let mut lx = Lexer::new("fn main() { x = 42 }");
        let mut kinds = Vec::new();
        loop {
            let t = lx.next_token().unwrap();
            let done = t.kind == TokKind::Eof;
            kinds.push(t.kind);
            if done {
                break;
            }
        }
        assert_eq!(
            kinds,
            vec![
                TokKind::Fn,
                TokKind::Ident("main".into()),
                TokKind::LParen,
                TokKind::RParen,
                TokKind::LBrace,
                TokKind::Ident("x".into()),
                TokKind::Equal,
                TokKind::Int(42),
                TokKind::RBrace,
                TokKind::Eof,
            ]
        );
    }

    #[test]
    fn string_escapes_are_decoded() {
        let mut lx = Lexer::new(r#""a\"b\nc""#);
        let t = lx.next_token().unwrap();
        assert_eq!(t.kind, TokKind::Str("a\"b\nc".into()));
    }
}
