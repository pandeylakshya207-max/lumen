#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Int(i64), Float(f64), Str(String), Ident(String),
    Let, Fn, If, Else, While, Return, True, False, Nil,
    // Single-char punctuation
    Plus, Minus, Star, Slash, Percent,
    LParen, RParen, LBrace, RBrace,
    Comma, Semicolon, Colon,
    Eof,
}
