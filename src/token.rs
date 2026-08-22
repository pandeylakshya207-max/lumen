#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Int(i64), Float(f64), Str(String), Ident(String),
    Let, Fn, If, Else, While, Return, True, False, Nil,
    Plus, Minus, Star, Slash, Percent,
    LParen, RParen, LBrace, RBrace,
    Comma, Semicolon, Colon,
    // 1-2 char operators
    Bang, BangEqual, Equal, EqualEqual,
    Less, LessEqual, Greater, GreaterEqual,
    AmpAmp, PipePipe,
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
    pub col: usize,
}

impl Token {
    pub fn new(kind: TokenKind, line: usize, col: usize) -> Self {
        Self { kind, line, col }
    }
}
