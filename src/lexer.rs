use crate::token::{Token, TokenKind};

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
        let b = self.src[self.pos];
        self.pos += 1;
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
}
