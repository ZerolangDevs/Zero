//! Scanning of raw `lowlevel` blocks: unparsed Rust code between the
//! braces of `scope lowlevel { ... }`. Braces inside strings, char
//! literals, raw strings and comments are ignored.

use crate::diag::{CompileError, Span};

fn skip_string(src: &str, mut pos: usize) -> Result<usize, CompileError> {
    pos += 1; // opening quote
    while pos < src.len() {
        match src[pos..].chars().next().unwrap() {
            '\\' => {
                pos += 1;
                if pos < src.len() {
                    pos += src[pos..].chars().next().unwrap().len_utf8();
                }
            }
            '"' => return Ok(pos + 1),
            '\n' => {
                return Err(CompileError::at(
                    "unterminated string literal in lowlevel block",
                    "<source>",
                    Span::new(pos, pos + 1),
                ));
            }
            c => pos += c.len_utf8(),
        }
    }
    Err(CompileError::at(
        "unterminated string literal in lowlevel block",
        "<source>",
        Span::new(pos, pos),
    ))
}

/// Skip a raw string literal (`r"..."`, `r#"..."#`) starting at `pos`.
fn skip_raw_string(src: &str, mut pos: usize) -> Result<usize, CompileError> {
    pos += 1; // 'r'
    let mut hashes = 0usize;
    while src[pos..].starts_with('#') {
        hashes += 1;
        pos += 1;
    }
    // opening quote
    if !src[pos..].starts_with('"') {
        return Err(CompileError::at(
            "malformed raw string in lowlevel block",
            "<source>",
            Span::new(pos, pos + 1),
        ));
    }
    pos += 1;
    while pos < src.len() {
        if src[pos..].starts_with('"') && src[pos + 1..].starts_with(&"#".repeat(hashes)) {
            return Ok(pos + 1 + hashes);
        }
        let c = src[pos..].chars().next().unwrap();
        pos += c.len_utf8();
    }
    Err(CompileError::at(
        "unterminated raw string in lowlevel block",
        "<source>",
        Span::new(pos, pos),
    ))
}

/// Scan a raw `lowlevel` block: everything between the opening `{` at
/// `start` and its matching `}`. Returns the raw text and the offset just
/// after the closing brace. Braces inside strings, char literals, raw
/// strings and comments are ignored, so arbitrary Rust code can appear.
pub fn scan_raw_block(src: &str, start: usize) -> Result<(String, usize), CompileError> {
    if !src[start..].starts_with('{') {
        return Err(CompileError::at(
            "internal error: raw block does not start with '{'",
            "<source>",
            Span::new(start, start + 1),
        ));
    }
    let mut i = start + 1;
    let mut depth = 1i32;
    while i < src.len() {
        let c = src[i..].chars().next().unwrap();
        match c {
            '{' => {
                depth += 1;
                i += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let text = src[start + 1..i].to_string();
                    return Ok((text, i + 1));
                }
                i += 1;
            }
            '"' => i = skip_string(src, i)?,
            '\'' => {
                // Distinguish char literals ('a', '\n') from lifetimes ('a).
                let next = src[i + 1..].chars().next();
                let after = src[i + 1..].chars().nth(1);
                let is_char_literal = match (next, after) {
                    (Some('\\'), _) => true,
                    (Some(a), Some(b)) if a.is_alphanumeric() && b == '\'' => true,
                    (Some('}'), Some('\'')) => true,
                    _ => false,
                };
                if is_char_literal {
                    // Find the closing quote, honouring escapes.
                    let mut j = i + 1;
                    while j < src.len() {
                        let cc = src[j..].chars().next().unwrap();
                        if cc == '\\' {
                            j += 1;
                            if j < src.len() {
                                j += src[j..].chars().next().unwrap().len_utf8();
                            }
                        } else if cc == '\'' {
                            break;
                        } else {
                            j += cc.len_utf8();
                        }
                    }
                    if j < src.len() {
                        i = j + 1;
                    } else {
                        i += 1;
                    }
                } else {
                    // Lifetime or lone quote: treat as ordinary text.
                    i += 1;
                }
            }
            'r' if src[i + 1..].starts_with('"') || src[i + 1..].starts_with('#') => {
                i = skip_raw_string(src, i)?;
            }
            '/' if src[i + 1..].starts_with('/') => {
                while i < src.len() && src[i..].chars().next() != Some('\n') {
                    i += src[i..].chars().next().unwrap().len_utf8();
                }
            }
            '/' if src[i + 1..].starts_with('*') => {
                let mut j = i + 2;
                let mut d = 1usize;
                while j < src.len() {
                    if src[j..].starts_with("/*") {
                        d += 1;
                        j += 2;
                    } else if src[j..].starts_with("*/") {
                        d -= 1;
                        j += 2;
                        if d == 0 {
                            break;
                        }
                    } else {
                        j += src[j..].chars().next().unwrap().len_utf8();
                    }
                }
                i = j;
            }
            _ => i += c.len_utf8(),
        }
    }
    Err(CompileError::at(
        "unterminated lowlevel block (missing '}')",
        "<source>",
        Span::new(start, src.len()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_block_ignores_braces_inside_strings() {
        let src = "{ println!(\"}}\"); if x { let y = 1; } }";
        let (text, end) = scan_raw_block(src, 0).unwrap();
        assert_eq!(text, " println!(\"}}\"); if x { let y = 1; } ");
        assert_eq!(&src[end - 1..end], "}");
    }

    #[test]
    fn raw_block_handles_comments_and_lifetimes() {
        let src = "{ // { not a brace\n let r: &'a str = r#\"{\"#; /* } */ }";
        let (text, end) = scan_raw_block(src, 0).unwrap();
        assert!(text.contains("r#\"{\"#"));
        assert_eq!(&src[end - 1..end], "}");
    }
}
