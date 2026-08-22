#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Literals
    Int(i64),
    Float(f64),
    Str(String),
    Ident(String),

    // Keywords
    Let, Fn, If, Else, While, Return, True, False, Nil,

    Eof,
}
