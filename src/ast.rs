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

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Let    { name: String, ty: Option<Ty>, init: Expr },
    Assign { name: String, value: Expr },
    ExprStmt(Expr),
    If     { cond: Expr, then: Vec<Stmt>, else_: Option<Vec<Stmt>> },
    While  { cond: Expr, body: Vec<Stmt> },
    Return (Expr),
    Fn     { name: String, params: Vec<(String, Ty)>, ret: Option<Ty>, body: Vec<Stmt> },
}
