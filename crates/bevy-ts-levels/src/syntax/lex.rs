//! The lossless lexer. Each token owns the whitespace and comments before it (its trivia), so the
//! tokens of a file, trivia and text in order, are the file. Literal values are decoded as
//! upstream's lexer decodes them, so the differential test compares like with like.

use super::{Diagnostic, Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Ident,
    True,
    False,
    Int,
    Float,
    Str,
    LBracket,
    RBracket,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Comma,
    DoubleColon,
    Colon,
    DoubleMinus,
    Hash,
    At,
    /// `..`, lexed only to reject ranges and struct update by name.
    DotDot,
    /// The end of the file; its trivia is the file's trailing whitespace and comments.
    Eof,
}

impl Kind {
    pub fn describe(self) -> &'static str {
        match self {
            Kind::Ident => "an identifier",
            Kind::True | Kind::False => "a bool",
            Kind::Int => "an integer",
            Kind::Float => "a float",
            Kind::Str => "a string",
            Kind::LBracket => "`[`",
            Kind::RBracket => "`]`",
            Kind::LParen => "`(`",
            Kind::RParen => "`)`",
            Kind::LBrace => "`{`",
            Kind::RBrace => "`}`",
            Kind::Comma => "`,`",
            Kind::DoubleColon => "`::`",
            Kind::Colon => "`:`",
            Kind::DoubleMinus => "`--`",
            Kind::Hash => "`#`",
            Kind::At => "`@`",
            Kind::DotDot => "`..`",
            Kind::Eof => "the end of the file",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub kind: Kind,
    /// The whitespace and comments before the token.
    pub trivia: Span,
    pub span: Span,
}

/// The tokens of `src`, ending with [`Kind::Eof`], and the spans of its comments.
pub fn lex(src: &str) -> Result<(Vec<Token>, Vec<Span>), Diagnostic> {
    let mut lx = Lexer { src: src.as_bytes(), pos: 0, comments: Vec::new() };
    let mut tokens = Vec::new();
    loop {
        let trivia_start = lx.pos;
        lx.trivia()?;
        let trivia = Span::new(trivia_start, lx.pos);
        let start = lx.pos;
        let kind = lx.token()?;
        tokens.push(Token { kind, trivia, span: Span::new(start, lx.pos) });
        if kind == Kind::Eof {
            return Ok((tokens, lx.comments));
        }
    }
}

struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
    comments: Vec<Span>,
}

impl Lexer<'_> {
    fn peek(&self, ahead: usize) -> Option<u8> {
        self.src.get(self.pos + ahead).copied()
    }

    fn err(&self, message: impl Into<String>, start: usize) -> Diagnostic {
        let end = (start + 1).min(self.src.len()).max(start);
        Diagnostic::new(message, Span::new(start, end))
    }

    fn trivia(&mut self) -> Result<(), Diagnostic> {
        loop {
            match (self.peek(0), self.peek(1)) {
                (Some(c), _) if c.is_ascii_whitespace() => self.pos += 1,
                (Some(b'/'), Some(b'/')) => {
                    let start = self.pos;
                    while self.peek(0).is_some_and(|c| c != b'\n') {
                        self.pos += 1;
                    }
                    self.comments.push(Span::new(start, self.pos));
                }
                (Some(b'/'), Some(b'*')) => {
                    // Block comments nest, as Rust's do.
                    let start = self.pos;
                    self.pos += 2;
                    let mut depth = 1;
                    while depth > 0 {
                        match (self.peek(0), self.peek(1)) {
                            (None, _) => return Err(self.err("unterminated block comment", start)),
                            (Some(b'/'), Some(b'*')) => (depth, self.pos) = (depth + 1, self.pos + 2),
                            (Some(b'*'), Some(b'/')) => (depth, self.pos) = (depth - 1, self.pos + 2),
                            _ => self.pos += 1,
                        }
                    }
                    self.comments.push(Span::new(start, self.pos));
                }
                _ => return Ok(()),
            }
        }
    }

    fn token(&mut self) -> Result<Kind, Diagnostic> {
        let start = self.pos;
        let Some(c) = self.peek(0) else { return Ok(Kind::Eof) };
        let one = |lx: &mut Self, k| {
            lx.pos += 1;
            Ok(k)
        };
        match c {
            b'[' => one(self, Kind::LBracket),
            b']' => one(self, Kind::RBracket),
            b'(' => one(self, Kind::LParen),
            b')' => one(self, Kind::RParen),
            b'{' => one(self, Kind::LBrace),
            b'}' => one(self, Kind::RBrace),
            b',' => one(self, Kind::Comma),
            b'#' => one(self, Kind::Hash),
            b'@' => one(self, Kind::At),
            b':' if self.peek(1) == Some(b':') => {
                self.pos += 2;
                Ok(Kind::DoubleColon)
            }
            b':' => one(self, Kind::Colon),
            b'-' if self.peek(1) == Some(b'-') => {
                self.pos += 2;
                Ok(Kind::DoubleMinus)
            }
            b'"' => self.string(),
            b'.' if self.peek(1) == Some(b'.') => {
                self.pos += 2;
                Ok(Kind::DotDot)
            }
            b'-' | b'+' | b'.' | b'0'..=b'9' => self.number(),
            c if c.is_ascii_alphabetic() || c == b'_' => {
                while self.peek(0).is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_') {
                    self.pos += 1;
                }
                Ok(match &self.src[start..self.pos] {
                    b"true" => Kind::True,
                    b"false" => Kind::False,
                    _ => Kind::Ident,
                })
            }
            b'~' => Err(self.err("templates (`~`) are not supported in .bsn files", start)),
            b'|' => Err(self.err("closures are not supported in .bsn files", start)),
            _ => {
                let ch = std::str::from_utf8(&self.src[start..]).ok().and_then(|s| s.chars().next()).unwrap_or('?');
                Err(self.err(format!("unexpected character `{ch}`"), start))
            }
        }
    }

    fn digits(&mut self, hex: bool) -> usize {
        let start = self.pos;
        while self.peek(0).is_some_and(|c| if hex { c.is_ascii_hexdigit() } else { c.is_ascii_digit() }) {
            self.pos += 1;
        }
        self.pos - start
    }

    /// Upstream's number grammar: an optional sign, then `0x` hex or decimal digits for an int, or
    /// `digits? . digits? (e sign? digits)?` for a float. A `..` after the digits is left for the
    /// parser to name as a range.
    fn number(&mut self) -> Result<Kind, Diagnostic> {
        let start = self.pos;
        if matches!(self.peek(0), Some(b'-' | b'+')) {
            let next_digit = self.peek(1).is_some_and(|c| c.is_ascii_digit())
                || (self.peek(1) == Some(b'.') && self.peek(2).is_some_and(|c| c.is_ascii_digit()));
            if !next_digit {
                let ch = self.peek(0).unwrap() as char;
                return Err(self.err(format!("unexpected `{ch}`"), start));
            }
            self.pos += 1;
        }
        let kind = if self.peek(0) == Some(b'0') && matches!(self.peek(1), Some(b'x' | b'X')) {
            self.pos += 2;
            if self.digits(true) == 0 {
                return Err(self.err("expected hex digits after `0x`", start));
            }
            Kind::Int
        } else {
            let int_digits = self.digits(false);
            if self.peek(0) == Some(b'.') && self.peek(1) != Some(b'.') {
                self.pos += 1;
                let frac_digits = self.digits(false);
                if int_digits + frac_digits == 0 {
                    return Err(self.err("unexpected `.`", start));
                }
                if matches!(self.peek(0), Some(b'e' | b'E')) {
                    let sign = usize::from(matches!(self.peek(1), Some(b'+' | b'-')));
                    if self.peek(1 + sign).is_some_and(|c| c.is_ascii_digit()) {
                        self.pos += 1 + sign;
                        self.digits(false);
                    }
                }
                Kind::Float
            } else if int_digits == 0 {
                return Err(self.err("unexpected `.`", start));
            } else {
                Kind::Int
            }
        };
        if self.peek(0).is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_') {
            return Err(self.err("number suffixes are not supported in .bsn files", self.pos));
        }
        Ok(kind)
    }

    fn string(&mut self) -> Result<Kind, Diagnostic> {
        let start = self.pos;
        self.pos += 1;
        loop {
            match self.peek(0) {
                None => return Err(self.err("unterminated string", start)),
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(Kind::Str);
                }
                Some(b'\\') if self.peek(1).is_some() => self.pos += 2,
                _ => self.pos += 1,
            }
        }
    }
}

/// The value of a string token's text, quotes included, decoded as upstream decodes it: `\n`,
/// `\r`, `\t`, `\0`, `\\`, `\"` and `\xHH`; any other escape is kept as written.
pub fn unescape(text: &str) -> String {
    let body = &text[1..text.len() - 1];
    let mut out = String::with_capacity(body.len());
    let mut chars = body.char_indices();
    while let Some((i, c)) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let Some((_, e)) = chars.next() else {
            out.push('\\');
            break;
        };
        match e {
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            '0' => out.push('\0'),
            '\\' => out.push('\\'),
            '"' => out.push('"'),
            'x' => match body.get(i + 2..i + 4).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                Some(byte) => {
                    out.push(byte as char);
                    chars.next();
                    chars.next();
                }
                None => out.push_str("\\x"),
            },
            other => {
                out.push('\\');
                out.push(other);
            }
        }
    }
    out
}

/// A string as `.bsn` text, quotes included; the writer uses it for new values.
pub fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<Kind> {
        lex(src).unwrap().0.into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn tokens_and_trivia_cover_the_file() {
        let src = "// head\n#A /* x /* nested */ */ Foo { x: -1.5e3, y: 0x1F } -- \"s\\\"\" ..\n";
        let (tokens, comments) = lex(src).unwrap();
        let printed: String = tokens.iter().map(|t| &src[t.trivia.start..t.span.end]).collect();
        assert_eq!(printed, src);
        assert_eq!(comments.len(), 2);
        assert_eq!(
            kinds(src),
            [
                Kind::Hash,
                Kind::Ident,
                Kind::Ident,
                Kind::LBrace,
                Kind::Ident,
                Kind::Colon,
                Kind::Float,
                Kind::Comma,
                Kind::Ident,
                Kind::Colon,
                Kind::Int,
                Kind::RBrace,
                Kind::DoubleMinus,
                Kind::Str,
                Kind::DotDot,
                Kind::Eof
            ]
        );
    }

    #[test]
    fn numbers_and_ranges() {
        assert_eq!(kinds(".5 1. +2 0..5"), [Kind::Float, Kind::Float, Kind::Int, Kind::Int, Kind::DotDot, Kind::Int, Kind::Eof]);
        assert!(lex("1f32").unwrap_err().message.contains("suffixes"));
        assert!(lex("- 1").is_err());
        assert!(lex("~Foo").unwrap_err().message.contains("templates"));
    }

    #[test]
    fn strings_decode_as_upstream() {
        assert_eq!(unescape(r#""a\"b\n\x41\q""#), "a\"b\nA\\q");
        assert_eq!(unescape(&quote("a\"b\\c\n")), "a\"b\\c\n");
        assert!(lex("\"open").unwrap_err().message.contains("unterminated"));
    }
}
