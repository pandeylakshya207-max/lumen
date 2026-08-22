use crate::token::{keyword, Token, TokenKind};

pub struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
    line: usize,
    col: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Self { src: src.as_bytes(), pos: 0, line: 1, col: 1 }
    }
    pub fn is_at_end(&self) -> bool { self.pos >= self.src.len() }
    pub fn peek(&self) -> Option<u8> { self.src.get(self.pos).copied() }
    pub fn peek_next(&self) -> Option<u8> { self.src.get(self.pos + 1).copied() }
    pub fn advance(&mut self) -> u8 {
        let b = self.src[self.pos]; self.pos += 1;
        if b == b'\n' { self.line += 1; self.col = 1; } else { self.col += 1; }
        b
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
            b'&' => if self.peek()==Some(b'&'){self.advance();TokenKind::AmpAmp}else{TokenKind::Eof},
            b'|' => if self.peek()==Some(b'|'){self.advance();TokenKind::PipePipe}else{TokenKind::Eof},
            b'"' => self.lex_string(),
            b if b.is_ascii_digit() => self.lex_number(b),
            b if b.is_ascii_alphabetic() || b == b'_' => self.lex_ident(b),
            _ => TokenKind::Eof,
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

    fn lex_string(&mut self) -> TokenKind {
        let mut s = String::new();
        loop {
            match self.peek() {
                None | Some(b'"') => { self.advance(); break; }
                Some(b'\\') => {
                    self.advance();
                    match self.peek() {
                        Some(b'n')  => { self.advance(); s.push('\n'); }
                        Some(b't')  => { self.advance(); s.push('\t'); }
                        Some(b'"')  => { self.advance(); s.push('"'); }
                        Some(b'\\') => { self.advance(); s.push('\\'); }
                        _ => {}
                    }
                }
                Some(c) => { s.push(c as char); self.advance(); }
            }
        }
        TokenKind::Str(s)
    }

    fn lex_number(&mut self, first: u8) -> TokenKind {
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
        if is_float { TokenKind::Float(raw.parse().unwrap_or(0.0)) }
        else { TokenKind::Int(raw.parse().unwrap_or(0)) }
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
    #[test] fn comment_at_end_of_file_no_newline() {
        assert_eq!(tok("1 // eof comment"), vec![TokenKind::Int(1), TokenKind::Eof]);
    }
}
