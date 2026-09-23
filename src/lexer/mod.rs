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

    /// Lex an integer or floating-point literal. A `.` only starts the
    /// fractional part when it is followed by a digit, so ranges (`0..5`)
    /// are never confused with floats.
    fn lex_number(&mut self, start: usize) -> Result<TokKind, CompileError> {
        let mut text = String::new();
        while let Some(c) = self.peek_char() {
            if c.is_ascii_digit() {
                text.push(c);
                self.bump();
            } else {
                break;
            }
        }
        let mut is_float = false;
        // Fractional part: `digits.digits` (not `digits..`).
        if self.peek_char() == Some('.')
            && self.src[self.pos + 1..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit())
        {
            is_float = true;
            text.push('.');
            self.bump();
            while let Some(c) = self.peek_char() {
                if c.is_ascii_digit() {
                    text.push(c);
                    self.bump();
                } else {
                    break;
                }
            }
        }
        // Optional exponent: `1e3`, `2.5e-2`.
        if matches!(self.peek_char(), Some('e') | Some('E')) {
            let mut lookahead = self.pos + 1;
            if self.src[lookahead..]
                .chars()
                .next()
                .is_some_and(|c| c == '+' || c == '-')
            {
                lookahead += 1;
            }
            if self.src[lookahead..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit())
            {
                is_float = true;
                text.push('e');
                self.bump();
                if matches!(self.peek_char(), Some('+') | Some('-')) {
                    text.push(self.bump().unwrap());
                }
                while let Some(c) = self.peek_char() {
                    if c.is_ascii_digit() {
                        text.push(c);
                        self.bump();
                    } else {
                        break;
                    }
                }
            }
        }
        if is_float {
            match text.parse::<f64>() {
                Ok(v) => Ok(TokKind::Float(v)),
                Err(_) => Err(CompileError::at(
                    "invalid float literal",
                    "<source>",
                    Span::new(start, self.pos),
                )),
            }
        } else {
            match text.parse::<i64>() {
                Ok(v) => Ok(TokKind::Int(v)),
                Err(_) => Err(CompileError::at(
                    "integer literal is out of range",
                    "<source>",
                    Span::new(start, self.pos),
                )),
            }
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
            "func" => TokKind::Func,
            "return" => TokKind::Return,
            "if" => TokKind::If,
            "else" => TokKind::Else,
            "else_if" => TokKind::ElseIf,
            "while" => TokKind::While,
            "for" => TokKind::For,
            "each" => TokKind::Each,
            "in" => TokKind::In,
            "switch" => TokKind::Switch,
            "case" => TokKind::Case,
            "default" => TokKind::Default,
            "try" => TokKind::Try,
            "catch" => TokKind::Catch,
            "import" => TokKind::Import,
            "scope" => TokKind::Scope,
            "highlevel" => TokKind::HighLevel,
            "lowlevel" => TokKind::LowLevel,
            "const" => TokKind::Const,
            "true" => TokKind::True,
            "false" => TokKind::False,
            "and" => TokKind::And,
            "or" => TokKind::Or,
            "break" => TokKind::Break,
            "continue" => TokKind::Continue,
            "NULL" | "null" => TokKind::Null,
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
            ':' => {
                self.bump();
                TokKind::Colon
            }
            ',' => {
                self.bump();
                TokKind::Comma
            }
            '.' => {
                self.bump();
                if self.peek_char() == Some('.') {
                    self.bump();
                    if self.peek_char() == Some('=') {
                        self.bump();
                        TokKind::DotDotEq
                    } else {
                        TokKind::DotDot
                    }
                } else {
                    TokKind::Dot
                }
            }
            '+' => {
                self.bump();
                if self.peek_char() == Some('=') {
                    self.bump();
                    TokKind::PlusEq
                } else {
                    TokKind::Plus
                }
            }
            '-' => {
                self.bump();
                if self.peek_char() == Some('>') {
                    self.bump();
                    TokKind::Arrow
                } else if self.peek_char() == Some('=') {
                    self.bump();
                    TokKind::MinusEq
                } else {
                    TokKind::Minus
                }
            }
            '*' => {
                self.bump();
                if self.peek_char() == Some('=') {
                    self.bump();
                    TokKind::StarEq
                } else {
                    TokKind::Star
                }
            }
            '/' => {
                self.bump();
                TokKind::Slash
            }
            '%' => {
                self.bump();
                TokKind::Percent
            }
            '!' => {
                self.bump();
                if self.peek_char() == Some('=') {
                    self.bump();
                    TokKind::BangEq
                } else {
                    TokKind::Bang
                }
            }
            '&' => {
                let end = start + 1;
                return Err(CompileError::at(
                    "unexpected character '&' (use the keyword 'and')",
                    "<source>",
                    Span::new(start, end),
                ));
            }
            '|' => {
                let end = start + 1;
                return Err(CompileError::at(
                    "unexpected character '|' (use the keyword 'or')",
                    "<source>",
                    Span::new(start, end),
                ));
            }
            '<' => {
                self.bump();
                if self.peek_char() == Some('=') {
                    self.bump();
                    TokKind::Le
                } else {
                    TokKind::Lt
                }
            }
            '>' => {
                self.bump();
                if self.peek_char() == Some('=') {
                    self.bump();
                    TokKind::Ge
                } else {
                    TokKind::Gt
                }
            }
            '=' => {
                self.bump();
                if self.peek_char() == Some('=') {
                    self.bump();
                    TokKind::EqEq
                } else {
                    TokKind::Equal
                }
            }
            '"' => self.lex_string(start)?,
            '0'..='9' => self.lex_number(start)?,
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

    #[test]
    fn lexes_float_literals() {
        let mut lx = Lexer::new("3.14 1.0 2.5e2 0.5");
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
                TokKind::Float(3.14),
                TokKind::Float(1.0),
                TokKind::Float(250.0),
                TokKind::Float(0.5),
                TokKind::Eof,
            ]
        );
    }

    #[test]
    fn ranges_are_not_floats() {
        // `0..5` must lex as Int, DotDot, Int (not a float).
        let mut lx = Lexer::new("0..5");
        let t1 = lx.next_token().unwrap();
        assert_eq!(t1.kind, TokKind::Int(0));
        let t2 = lx.next_token().unwrap();
        assert_eq!(t2.kind, TokKind::DotDot);
        let t3 = lx.next_token().unwrap();
        assert_eq!(t3.kind, TokKind::Int(5));
    }

    #[test]
    fn lexes_compound_assignment() {
        let mut lx = Lexer::new("x += 1\ny -= 2\nz *= 3");
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
                TokKind::Ident("x".into()),
                TokKind::PlusEq,
                TokKind::Int(1),
                TokKind::Ident("y".into()),
                TokKind::MinusEq,
                TokKind::Int(2),
                TokKind::Ident("z".into()),
                TokKind::StarEq,
                TokKind::Int(3),
                TokKind::Eof,
            ]
        );
    }

    #[test]
    fn lexes_break_continue_null() {
        let mut lx = Lexer::new("break continue NULL null");
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
                TokKind::Break,
                TokKind::Continue,
                TokKind::Null,
                TokKind::Null,
                TokKind::Eof,
            ]
        );
    }
}
