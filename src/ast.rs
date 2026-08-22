#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    Add, Sub, Mul, Div, Mod,
    Eq, NotEq, Lt, LtEq, Gt, GtEq,
    And, Or,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnOp { Neg, Not }

#[derive(Debug, Clone, PartialEq)]
pub enum Ty { Int, Float, Bool, Str, Nil, Named(String) }

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Int(i64), Float(f64), Str(String), Bool(bool), Nil,
    Var(String),
    Binary { op: BinOp, lhs: Box<Expr>, rhs: Box<Expr> },
    Unary  { op: UnOp, expr: Box<Expr> },
    Call   { callee: String, args: Vec<Expr> },
    Group  (Box<Expr>),
}
