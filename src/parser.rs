use crate::ast::*;
use crate::lexer::Lexer;
use crate::token::{Token, TokenKind};

pub struct Parser { tokens: Vec<Token>, pos: usize }

#[derive(Debug)]
pub struct ParseError { pub msg: String, pub line: usize, pub col: usize }
impl ParseError {
    fn new(msg: impl Into<String>, tok: &Token) -> Self {
        Self { msg: msg.into(), line: tok.line, col: tok.col }
    }
}
pub type ParseResult<T> = Result<T, ParseError>;

impl Parser {
    pub fn new(src: &str) -> Self {
        Self { tokens: Lexer::new(src).tokenize(), pos: 0 }
    }
    fn peek(&self) -> &Token { &self.tokens[self.pos] }
    fn is_at_end(&self) -> bool { self.peek().kind == TokenKind::Eof }
    fn advance(&mut self) -> &Token {
        if !self.is_at_end() { self.pos += 1; }
        &self.tokens[self.pos - 1]
    }
    fn check(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(&self.peek().kind) == std::mem::discriminant(kind)
    }
    fn match_tok(&mut self, kind: &TokenKind) -> bool {
        if self.check(kind) { self.advance(); true } else { false }
    }
    fn expect(&mut self, kind: &TokenKind, msg: &str) -> ParseResult<&Token> {
        if self.check(kind) { Ok(self.advance()) }
        else { Err(ParseError::new(format!("{} — got {:?}", msg, self.peek().kind), self.peek())) }
    }
    fn peek_is(&self, kind: &TokenKind) -> bool { self.check(kind) }
}
