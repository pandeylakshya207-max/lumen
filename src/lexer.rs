use crate::token::{keyword, Token, TokenKind};

pub struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
    line: usize,
    col: usize,
    /// The first lexical error, as (message, line, col). The token stream ends
    /// with Eof at that point, and the parser reports this instead of Eof.
    pub error: Option<(String, usize, usize)>,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Self { src: src.as_bytes(), pos: 0, line: 1, col: 1, error: None }
    }
    pub fn is_at_end(&self) -> bool { self.pos >= self.src.len() }
    pub fn peek(&self) -> Option<u8> { self.src.get(self.pos).copied() }
    pub fn peek_next(&self) -> Option<u8> { self.src.get(self.pos + 1).copied() }
    pub fn advance(&mut self) -> u8 {
        let b = self.src[self.pos]; self.pos += 1;
        if b == b'\n' { self.line += 1; self.col = 1; } else { self.col += 1; }
        b
    }
    /// Records a lexical error (only the first one is kept) and ends the token stream.
    fn fail(&mut self, msg: impl Into<String>, line: usize, col: usize) -> TokenKind {
        if self.error.is_none() { self.error = Some((msg.into(), line, col)); }
        self.pos = self.src.len();
        TokenKind::Eof
    }

    pub fn skip_whitespace(&mut self) {
        loop {
            while let Some(b) = self.peek() {
                if b.is_ascii_whitespace() { self.advance(); } else { break; }
            }
            if self.peek() == Some(b'/') && self.peek_next() == Some(b'/') {
                while let Some(b) = self.peek() { self.advance(); if b == b'\n' { break; } }
            } else { break; }
        }
    }

    pub fn next_token(&mut self) -> Token {
        self.skip_whitespace();
        if self.is_at_end() { return Token::new(TokenKind::Eof, self.line, self.col); }
        let line = self.line; let col = self.col;
        let b = self.advance();
        let kind = match b {
            b'+' => TokenKind::Plus,   b'-' => TokenKind::Minus,
            b'*' => TokenKind::Star,   b'/' => TokenKind::Slash,
            b'%' => TokenKind::Percent,
            b'(' => TokenKind::LParen, b')' => TokenKind::RParen,
            b'{' => TokenKind::LBrace, b'}' => TokenKind::RBrace,
            b',' => TokenKind::Comma,  b';' => TokenKind::Semicolon,
            b':' => TokenKind::Colon,
            b'!' => if self.peek()==Some(b'='){self.advance();TokenKind::BangEqual}else{TokenKind::Bang},
            b'=' => if self.peek()==Some(b'='){self.advance();TokenKind::EqualEqual}else{TokenKind::Equal},
            b'<' => if self.peek()==Some(b'='){self.advance();TokenKind::LessEqual}else{TokenKind::Less},
            b'>' => if self.peek()==Some(b'='){self.advance();TokenKind::GreaterEqual}else{TokenKind::Greater},
            b'&' => if self.peek()==Some(b'&'){self.advance();TokenKind::AmpAmp}else{self.fail("expected '&&', found a single '&'", line, col)},
            b'|' => if self.peek()==Some(b'|'){self.advance();TokenKind::PipePipe}else{self.fail("expected '||', found a single '|'", line, col)},
            b'"' => self.lex_string(line, col),
            b if b.is_ascii_digit() => self.lex_number(b, line, col),
            b if b.is_ascii_alphabetic() || b == b'_' => self.lex_ident(b),
            b if b.is_ascii_graphic() => self.fail(format!("unexpected character '{}'", b as char), line, col),
            _ => self.fail("unexpected character", line, col),
        };
        Token::new(kind, line, col)
    }

    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            let tok = self.next_token();
            let done = tok.kind == TokenKind::Eof;
            tokens.push(tok);
            if done { break; }
        }
        tokens
    }

    /// `line`/`col` locate the opening quote, for the unterminated-string error.
    fn lex_string(&mut self, line: usize, col: usize) -> TokenKind {
        // collected as bytes so that UTF-8 text inside a string survives intact
        let mut bytes: Vec<u8> = Vec::new();
        loop {
            match self.peek() {
                None => return self.fail("unterminated string", line, col),
                Some(b'"') => { self.advance(); break; }
                Some(b'\\') => {
                    self.advance();
                    match self.peek() {
                        Some(b'n')  => { self.advance(); bytes.push(b'\n'); }
                        Some(b't')  => { self.advance(); bytes.push(b'\t'); }
                        Some(b'"')  => { self.advance(); bytes.push(b'"'); }
                        Some(b'\\') => { self.advance(); bytes.push(b'\\'); }
                        _ => {}
                    }
                }
                Some(c) => { bytes.push(c); self.advance(); }
            }
        }
        TokenKind::Str(String::from_utf8_lossy(&bytes).into_owned())
    }

    fn lex_number(&mut self, first: u8, line: usize, col: usize) -> TokenKind {
        let mut raw = String::new();
        raw.push(first as char);
        let mut is_float = false;
        loop {
            match self.peek() {
                Some(b) if b.is_ascii_digit() => { raw.push(b as char); self.advance(); }
                Some(b'.') if !is_float => { is_float = true; raw.push('.'); self.advance(); }
                _ => break,
            }
        }
        if is_float {
            TokenKind::Float(raw.parse().unwrap_or(0.0))
        } else {
            match raw.parse() {
                Ok(n) => TokenKind::Int(n),
                Err(_) => self.fail(format!("integer literal {} is too large", raw), line, col),
            }
        }
    }

    fn lex_ident(&mut self, first: u8) -> TokenKind {
        let mut raw = String::new();
        raw.push(first as char);
        while let Some(b) = self.peek() {
            if b.is_ascii_alphanumeric() || b == b'_' { raw.push(b as char); self.advance(); }
            else { break; }
        }
        keyword(&raw).unwrap_or(TokenKind::Ident(raw))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tok(src: &str) -> Vec<TokenKind> {
        Lexer::new(src).tokenize().into_iter().map(|t| t.kind).collect()
    }
    #[test] fn empty_source() { assert_eq!(tok(""), vec![TokenKind::Eof]); }
    #[test] fn single_plus()  { assert_eq!(tok("+"), vec![TokenKind::Plus, TokenKind::Eof]); }
    #[test] fn two_char_operators() {
        assert_eq!(tok("!= == <= >= && ||"), vec![
            TokenKind::BangEqual, TokenKind::EqualEqual,
            TokenKind::LessEqual, TokenKind::GreaterEqual,
            TokenKind::AmpAmp, TokenKind::PipePipe, TokenKind::Eof,
        ]);
    }
    #[test] fn integer_literal() { assert_eq!(tok("42"), vec![TokenKind::Int(42), TokenKind::Eof]); }
    #[test] fn float_literal()   { assert_eq!(tok("3.14"), vec![TokenKind::Float(3.14), TokenKind::Eof]); }
    #[test] fn string_literal()  { assert_eq!(tok(r#""hello""#), vec![TokenKind::Str("hello".into()), TokenKind::Eof]); }
    #[test] fn string_escape()   { assert_eq!(tok(r#""\n\t""#), vec![TokenKind::Str("\n\t".into()), TokenKind::Eof]); }
    #[test] fn keywords() {
        assert_eq!(tok("let fn if else while return true false nil"), vec![
            TokenKind::Let, TokenKind::Fn, TokenKind::If, TokenKind::Else,
            TokenKind::While, TokenKind::Return, TokenKind::True,
            TokenKind::False, TokenKind::Nil, TokenKind::Eof,
        ]);
    }
    #[test] fn identifier() { assert_eq!(tok("foo_bar"), vec![TokenKind::Ident("foo_bar".into()), TokenKind::Eof]); }
    #[test] fn position_tracking() {
        let mut lex = Lexer::new("a\nb");
        let t1 = lex.next_token(); let t2 = lex.next_token();
        assert_eq!(t1.line, 1); assert_eq!(t2.line, 2); assert_eq!(t2.col, 1);
    }
    #[test] fn whitespace_skipped() { assert_eq!(tok("  +  "), vec![TokenKind::Plus, TokenKind::Eof]); }
    #[test] fn full_expression() {
        assert_eq!(tok("let x = 1 + 2;"), vec![
            TokenKind::Let, TokenKind::Ident("x".into()), TokenKind::Equal,
            TokenKind::Int(1), TokenKind::Plus, TokenKind::Int(2),
            TokenKind::Semicolon, TokenKind::Eof,
        ]);
    }
    #[test] fn line_comment_skipped() { assert_eq!(tok("// comment\n+"), vec![TokenKind::Plus, TokenKind::Eof]); }
    #[test] fn inline_comment_after_token() {
        assert_eq!(tok("1 // comment\n+ 2"), vec![TokenKind::Int(1), TokenKind::Plus, TokenKind::Int(2), TokenKind::Eof]);
    }
    fn error_of(src: &str) -> Option<(String, usize, usize)> {
        let mut lex = Lexer::new(src);
        let tokens = lex.tokenize();
        assert_eq!(tokens.last().map(|t| &t.kind), Some(&TokenKind::Eof));
        lex.error
    }
    #[test] fn no_error_on_valid_source() { assert_eq!(error_of("let x = 1 + 2; // ok"), None); }
    #[test] fn unknown_character_is_an_error() {
        assert_eq!(error_of("1 @ 2"), Some(("unexpected character '@'".into(), 1, 3)));
    }
    #[test] fn single_ampersand_and_pipe_are_errors() {
        assert_eq!(error_of("a & b"), Some(("expected '&&', found a single '&'".into(), 1, 3)));
        assert_eq!(error_of("a | b"), Some(("expected '||', found a single '|'".into(), 1, 3)));
    }
    #[test] fn unterminated_string_is_an_error_not_a_panic() {
        assert_eq!(error_of("\"abc"), Some(("unterminated string".into(), 1, 1)));
        assert_eq!(error_of("let s = \"abc; s;"), Some(("unterminated string".into(), 1, 9)));
        assert_eq!(error_of("\"ends in a backslash\\"), Some(("unterminated string".into(), 1, 1)));
    }
    #[test] fn integer_literal_out_of_range_is_an_error() {
        assert_eq!(tok("9223372036854775807"), vec![TokenKind::Int(i64::MAX), TokenKind::Eof]);
        assert_eq!(error_of("9223372036854775808"), Some(("integer literal 9223372036854775808 is too large".into(), 1, 1)));
    }
    #[test] fn only_the_first_error_is_kept() {
        assert_eq!(error_of("@ #"), Some(("unexpected character '@'".into(), 1, 1)));
    }
    #[test] fn non_ascii_text_in_strings_is_kept() {
        assert_eq!(tok("\"h\u{e9}llo \u{4e16}\u{754c}\""), vec![TokenKind::Str("h\u{e9}llo \u{4e16}\u{754c}".into()), TokenKind::Eof]);
    }
    #[test] fn non_ascii_text_outside_strings_is_an_error() {
        assert_eq!(error_of("let \u{e9} = 1;"), Some(("unexpected character".into(), 1, 5)));
    }
    #[test] fn comment_at_end_of_file_no_newline() {
        assert_eq!(tok("1 // eof comment"), vec![TokenKind::Int(1), TokenKind::Eof]);
    }
}
