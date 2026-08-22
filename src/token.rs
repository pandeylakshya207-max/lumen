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

pub fn keyword(text: &str) -> Option<TokenKind> {
    match text {
        "let"    => Some(TokenKind::Let),
        "fn"     => Some(TokenKind::Fn),
        "if"     => Some(TokenKind::If),
        "else"   => Some(TokenKind::Else),
        "while"  => Some(TokenKind::While),
        "return" => Some(TokenKind::Return),
        "true"   => Some(TokenKind::True),
        "false"  => Some(TokenKind::False),
        "nil"    => Some(TokenKind::Nil),
        _        => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_let() { assert_eq!(keyword("let"), Some(TokenKind::Let)); }

    #[test]
    fn keyword_unknown() { assert_eq!(keyword("foo"), None); }

    #[test]
    fn token_position() {
        let t = Token::new(TokenKind::Plus, 3, 7);
        assert_eq!(t.line, 3);
        assert_eq!(t.col, 7);
    }
}
