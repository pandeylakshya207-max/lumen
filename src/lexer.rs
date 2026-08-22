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
}
