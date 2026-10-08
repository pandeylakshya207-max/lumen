//! Recursive-descent parser: tokens -> AST.
//!
//! Grammar (statements):
//!   program   := stmt* EOF
//!   stmt      := let | fn | if | while | return | assign | expr_stmt
//!   let       := "let" IDENT (":" type)? "=" expr ";"
//!   fn        := "fn" IDENT "(" (IDENT ":" type ("," IDENT ":" type)*)? ")" ("->" type)? block
//!   if        := "if" expr block ("else" (if | block))?
//!   while     := "while" expr block
//!   return    := "return" expr? ";"
//!   assign    := IDENT "=" expr ";"
//!   expr_stmt := expr ";"
//!   block     := "{" stmt* "}"
//!
//! Expressions, lowest to highest precedence, all binary operators left-associative:
//!   ||   &&   == !=   < <= > >=   + -   * / %   unary - !   call / primary
//!
//! The ";" that ends a statement may be left out at the very end of the input,
//! so a single expression typed at the REPL works without one.

use crate::ast::*;
use crate::lexer::Lexer;
use crate::token::{Token, TokenKind};

/// Deepest recursion the parser accepts, counted across blocks and expressions.
/// Keeps hostile input such as ten thousand "(" from overflowing the stack.
const MAX_DEPTH: usize = 100;

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    depth: usize,
    /// The lexer reports a character it does not know as Eof and stops.
    /// When that happened the input was cut short, and this is set.
    stray_char: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub msg: String,
    pub line: usize,
    pub col: usize,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (line {}, col {})", self.msg, self.line, self.col)
    }
}

pub type ParseResult<T> = Result<T, ParseError>;

impl Parser {
    pub fn new(src: &str) -> Self {
        let mut lexer = Lexer::new(src);
        let mut tokens = Vec::new();
        let mut stray_char = false;
        loop {
            lexer.skip_whitespace();
            let at_end = lexer.is_at_end();
            let tok = lexer.next_token();
            let done = tok.kind == TokenKind::Eof;
            if done { stray_char = !at_end; }
            tokens.push(tok);
            if done { break; }
        }
        Self { tokens, pos: 0, depth: 0, stray_char }
    }

    // ---- token helpers -----------------------------------------------------

    fn peek(&self) -> &Token { &self.tokens[self.pos] }

    fn peek_next(&self) -> Option<&TokenKind> {
        self.tokens.get(self.pos + 1).map(|t| &t.kind)
    }

    fn is_at_end(&self) -> bool { self.peek().kind == TokenKind::Eof }

    fn advance(&mut self) -> Token {
        let tok = self.tokens[self.pos].clone();
        if !self.is_at_end() { self.pos += 1; }
        tok
    }

    /// Same variant as `kind`; the payload of Int/Float/Str/Ident is ignored.
    fn check(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(&self.peek().kind) == std::mem::discriminant(kind)
    }

    fn match_tok(&mut self, kind: &TokenKind) -> bool {
        if self.check(kind) { self.advance(); true } else { false }
    }

    fn error<T>(&self, expected: &str) -> ParseResult<T> {
        let tok = self.peek();
        let msg = if tok.kind == TokenKind::Eof && self.stray_char {
            format!("expected {}, found a character lumen does not use", expected)
        } else {
            format!("expected {}, found {}", expected, describe(&tok.kind))
        };
        Err(ParseError { msg, line: tok.line, col: tok.col })
    }

    fn expect(&mut self, kind: &TokenKind, expected: &str) -> ParseResult<Token> {
        if self.check(kind) { Ok(self.advance()) } else { self.error(expected) }
    }

    fn expect_ident(&mut self, expected: &str) -> ParseResult<String> {
        if let TokenKind::Ident(name) = &self.peek().kind {
            let name = name.clone();
            self.advance();
            Ok(name)
        } else {
            self.error(expected)
        }
    }

    /// Ends a statement: ";" or, on the last statement only, the end of the input.
    fn end_stmt(&mut self) -> ParseResult<()> {
        if self.match_tok(&TokenKind::Semicolon) { return Ok(()); }
        if self.is_at_end() && !self.stray_char { return Ok(()); }
        self.error("';'")
    }

    fn enter(&mut self) -> ParseResult<()> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            let tok = self.peek();
            return Err(ParseError {
                msg: format!("nesting is deeper than {} levels", MAX_DEPTH),
                line: tok.line,
                col: tok.col,
            });
        }
        Ok(())
    }

    fn leave(&mut self) { self.depth -= 1; }

    // ---- statements --------------------------------------------------------

    pub fn parse_program(&mut self) -> ParseResult<Vec<Stmt>> {
        let mut stmts = Vec::new();
        while !self.is_at_end() {
            stmts.push(self.parse_stmt()?);
        }
        if self.stray_char {
            return self.error("a statement");
        }
        Ok(stmts)
    }

    fn parse_stmt(&mut self) -> ParseResult<Stmt> {
        match self.peek().kind {
            TokenKind::Let    => self.parse_let(),
            TokenKind::Fn     => self.parse_fn(),
            TokenKind::If     => self.parse_if(),
            TokenKind::While  => self.parse_while(),
            TokenKind::Return => self.parse_return(),
            TokenKind::Ident(_) if self.peek_next() == Some(&TokenKind::Equal) => self.parse_assign(),
            _ => self.parse_expr_stmt(),
        }
    }

    fn parse_let(&mut self) -> ParseResult<Stmt> {
        self.advance(); // let
        let name = self.expect_ident("a variable name after 'let'")?;
        let ty = if self.match_tok(&TokenKind::Colon) { Some(self.parse_type()?) } else { None };
        self.expect(&TokenKind::Equal, "'=' in let statement")?;
        let init = self.parse_expr()?;
        self.end_stmt()?;
        Ok(Stmt::Let { name, ty, init })
    }

    fn parse_assign(&mut self) -> ParseResult<Stmt> {
        let name = self.expect_ident("a variable name")?;
        self.expect(&TokenKind::Equal, "'='")?;
        let value = self.parse_expr()?;
        self.end_stmt()?;
        Ok(Stmt::Assign { name, value })
    }

    fn parse_fn(&mut self) -> ParseResult<Stmt> {
        self.advance(); // fn
        let name = self.expect_ident("a function name after 'fn'")?;
        self.expect(&TokenKind::LParen, "'(' after function name")?;
        let mut params = Vec::new();
        if !self.check(&TokenKind::RParen) {
            loop {
                let pname = self.expect_ident("a parameter name")?;
                self.expect(&TokenKind::Colon, "':' and a type after parameter name")?;
                params.push((pname, self.parse_type()?));
                if !self.match_tok(&TokenKind::Comma) { break; }
            }
        }
        self.expect(&TokenKind::RParen, "')' after parameters")?;
        // The lexer has no arrow token, so "->" arrives as '-' followed by '>'.
        let ret = if self.check(&TokenKind::Minus) && self.peek_next() == Some(&TokenKind::Greater) {
            self.advance();
            self.advance();
            Some(self.parse_type()?)
        } else {
            None
        };
        let body = self.parse_block()?;
        Ok(Stmt::Fn { name, params, ret, body })
    }

    fn parse_if(&mut self) -> ParseResult<Stmt> {
        self.advance(); // if
        let cond = self.parse_expr()?;
        let then = self.parse_block()?;
        let else_ = if self.match_tok(&TokenKind::Else) {
            if self.check(&TokenKind::If) {
                self.enter()?;
                let nested = self.parse_if()?;
                self.leave();
                Some(vec![nested])
            } else {
                Some(self.parse_block()?)
            }
        } else {
            None
        };
        Ok(Stmt::If { cond, then, else_ })
    }

    fn parse_while(&mut self) -> ParseResult<Stmt> {
        self.advance(); // while
        let cond = self.parse_expr()?;
        let body = self.parse_block()?;
        Ok(Stmt::While { cond, body })
    }

    fn parse_return(&mut self) -> ParseResult<Stmt> {
        self.advance(); // return
        let bare = self.check(&TokenKind::Semicolon)
            || self.check(&TokenKind::RBrace)
            || (self.is_at_end() && !self.stray_char);
        let value = if bare { Expr::Nil } else { self.parse_expr()? };
        self.end_stmt()?;
        Ok(Stmt::Return(value))
    }

    fn parse_expr_stmt(&mut self) -> ParseResult<Stmt> {
        let expr = self.parse_expr()?;
        self.end_stmt()?;
        Ok(Stmt::ExprStmt(expr))
    }

    fn parse_block(&mut self) -> ParseResult<Vec<Stmt>> {
        self.expect(&TokenKind::LBrace, "'{'")?;
        self.enter()?;
        let mut stmts = Vec::new();
        while !self.check(&TokenKind::RBrace) {
            if self.is_at_end() { return self.error("'}' to close the block"); }
            stmts.push(self.parse_stmt()?);
        }
        self.leave();
        self.advance(); // }
        Ok(stmts)
    }

    fn parse_type(&mut self) -> ParseResult<Ty> {
        if self.match_tok(&TokenKind::Nil) { return Ok(Ty::Nil); }
        let name = self.expect_ident("a type name")?;
        Ok(match name.as_str() {
            "int"   => Ty::Int,
            "float" => Ty::Float,
            "bool"  => Ty::Bool,
            "str"   => Ty::Str,
            _       => Ty::Named(name),
        })
    }

    // ---- expressions -------------------------------------------------------

    pub fn parse_expr(&mut self) -> ParseResult<Expr> { self.parse_binary(1) }

    /// Precedence climbing: parses a run of binary operators whose precedence
    /// is at least `min_prec`. Operators of equal precedence group to the left.
    fn parse_binary(&mut self, min_prec: u8) -> ParseResult<Expr> {
        self.enter()?;
        let mut lhs = self.parse_unary()?;
        while let Some((op, prec)) = binary_op(&self.peek().kind) {
            if prec < min_prec { break; }
            self.advance();
            let rhs = self.parse_binary(prec + 1)?;
            lhs = Expr::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        self.leave();
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> ParseResult<Expr> {
        let op = match self.peek().kind {
            TokenKind::Minus => UnOp::Neg,
            TokenKind::Bang  => UnOp::Not,
            _ => return self.parse_primary(),
        };
        self.advance();
        self.enter()?;
        let expr = self.parse_unary()?;
        self.leave();
        Ok(Expr::Unary { op, expr: Box::new(expr) })
    }

    fn parse_primary(&mut self) -> ParseResult<Expr> {
        let expr = match &self.peek().kind {
            TokenKind::Int(n)   => Expr::Int(*n),
            TokenKind::Float(x) => Expr::Float(*x),
            TokenKind::Str(s)   => Expr::Str(s.clone()),
            TokenKind::True     => Expr::Bool(true),
            TokenKind::False    => Expr::Bool(false),
            TokenKind::Nil      => Expr::Nil,
            TokenKind::Ident(name) => {
                let name = name.clone();
                self.advance();
                return if self.check(&TokenKind::LParen) { self.parse_call(name) } else { Ok(Expr::Var(name)) };
            }
            TokenKind::LParen => {
                self.advance();
                let inner = self.parse_expr()?;
                self.expect(&TokenKind::RParen, "')'")?;
                return Ok(Expr::Group(Box::new(inner)));
            }
            _ => return self.error("an expression"),
        };
        self.advance();
        Ok(expr)
    }

    fn parse_call(&mut self, callee: String) -> ParseResult<Expr> {
        self.advance(); // (
        let mut args = Vec::new();
        if !self.check(&TokenKind::RParen) {
            loop {
                args.push(self.parse_expr()?);
                if !self.match_tok(&TokenKind::Comma) { break; }
            }
        }
        self.expect(&TokenKind::RParen, "')' after arguments")?;
        Ok(Expr::Call { callee, args })
    }
}

/// Binary operator for a token, with its precedence (a higher number binds tighter).
fn binary_op(kind: &TokenKind) -> Option<(BinOp, u8)> {
    Some(match kind {
        TokenKind::PipePipe     => (BinOp::Or, 1),
        TokenKind::AmpAmp       => (BinOp::And, 2),
        TokenKind::EqualEqual   => (BinOp::Eq, 3),
        TokenKind::BangEqual    => (BinOp::NotEq, 3),
        TokenKind::Less         => (BinOp::Lt, 4),
        TokenKind::LessEqual    => (BinOp::LtEq, 4),
        TokenKind::Greater      => (BinOp::Gt, 4),
        TokenKind::GreaterEqual => (BinOp::GtEq, 4),
        TokenKind::Plus         => (BinOp::Add, 5),
        TokenKind::Minus        => (BinOp::Sub, 5),
        TokenKind::Star         => (BinOp::Mul, 6),
        TokenKind::Slash        => (BinOp::Div, 6),
        TokenKind::Percent      => (BinOp::Mod, 6),
        _ => return None,
    })
}

/// How a token is named in an error message.
fn describe(kind: &TokenKind) -> String {
    let fixed = match kind {
        TokenKind::Int(n)    => return format!("number {}", n),
        TokenKind::Float(x)  => return format!("number {}", x),
        TokenKind::Str(s)    => return format!("string {:?}", s),
        TokenKind::Ident(s)  => return format!("name '{}'", s),
        TokenKind::Eof       => return "end of input".to_string(),
        TokenKind::Let => "let", TokenKind::Fn => "fn", TokenKind::If => "if",
        TokenKind::Else => "else", TokenKind::While => "while", TokenKind::Return => "return",
        TokenKind::True => "true", TokenKind::False => "false", TokenKind::Nil => "nil",
        TokenKind::Plus => "+", TokenKind::Minus => "-", TokenKind::Star => "*",
        TokenKind::Slash => "/", TokenKind::Percent => "%",
        TokenKind::LParen => "(", TokenKind::RParen => ")",
        TokenKind::LBrace => "{", TokenKind::RBrace => "}",
        TokenKind::Comma => ",", TokenKind::Semicolon => ";", TokenKind::Colon => ":",
        TokenKind::Bang => "!", TokenKind::BangEqual => "!=",
        TokenKind::Equal => "=", TokenKind::EqualEqual => "==",
        TokenKind::Less => "<", TokenKind::LessEqual => "<=",
        TokenKind::Greater => ">", TokenKind::GreaterEqual => ">=",
        TokenKind::AmpAmp => "&&", TokenKind::PipePipe => "||",
    };
    format!("'{}'", fixed)
}

// ---- tests -----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn program(src: &str) -> Vec<Stmt> { Parser::new(src).parse_program().expect("parse error") }
    fn expr(src: &str) -> Expr { Parser::new(src).parse_expr().expect("parse error") }
    fn err(src: &str) -> ParseError { Parser::new(src).parse_program().expect_err("expected a parse error") }

    fn int(n: i64) -> Box<Expr> { Box::new(Expr::Int(n)) }
    fn var(s: &str) -> Box<Expr> { Box::new(Expr::Var(s.into())) }
    fn bin(op: BinOp, lhs: Box<Expr>, rhs: Box<Expr>) -> Box<Expr> { Box::new(Expr::Binary { op, lhs, rhs }) }

    // literals and primaries
    #[test] fn int_literal()   { assert_eq!(expr("42"), Expr::Int(42)); }
    #[test] fn float_literal() { assert_eq!(expr("2.5"), Expr::Float(2.5)); }
    #[test] fn str_literal()   { assert_eq!(expr(r#""hi""#), Expr::Str("hi".into())); }
    #[test] fn bool_literals() { assert_eq!(expr("true"), Expr::Bool(true)); assert_eq!(expr("false"), Expr::Bool(false)); }
    #[test] fn nil_literal()   { assert_eq!(expr("nil"), Expr::Nil); }
    #[test] fn variable()      { assert_eq!(expr("foo"), Expr::Var("foo".into())); }
    #[test] fn group()         { assert_eq!(expr("(1)"), Expr::Group(int(1))); }

    // precedence and associativity
    #[test]
    fn mul_binds_tighter_than_add() {
        assert_eq!(expr("1 + 2 * 3"), *bin(BinOp::Add, int(1), bin(BinOp::Mul, int(2), int(3))));
    }

    #[test]
    fn group_overrides_precedence() {
        let sum = Box::new(Expr::Group(bin(BinOp::Add, int(1), int(2))));
        assert_eq!(expr("(1 + 2) * 3"), *bin(BinOp::Mul, sum, int(3)));
    }

    #[test]
    fn subtraction_is_left_associative() {
        assert_eq!(expr("10 - 4 - 3"), *bin(BinOp::Sub, bin(BinOp::Sub, int(10), int(4)), int(3)));
    }

    #[test]
    fn division_is_left_associative() {
        assert_eq!(expr("8 / 4 / 2"), *bin(BinOp::Div, bin(BinOp::Div, int(8), int(4)), int(2)));
    }

    #[test]
    fn comparison_binds_tighter_than_equality() {
        assert_eq!(expr("1 < 2 == true"), *bin(BinOp::Eq, bin(BinOp::Lt, int(1), int(2)), Box::new(Expr::Bool(true))));
    }

    #[test]
    fn and_binds_tighter_than_or() {
        assert_eq!(expr("a || b && c"), *bin(BinOp::Or, var("a"), bin(BinOp::And, var("b"), var("c"))));
    }

    #[test]
    fn arithmetic_binds_tighter_than_comparison() {
        assert_eq!(expr("a + 1 < b * 2"), *bin(BinOp::Lt, bin(BinOp::Add, var("a"), int(1)), bin(BinOp::Mul, var("b"), int(2))));
    }

    #[test]
    fn every_binary_operator() {
        for (src, op) in [
            ("+", BinOp::Add), ("-", BinOp::Sub), ("*", BinOp::Mul), ("/", BinOp::Div), ("%", BinOp::Mod),
            ("==", BinOp::Eq), ("!=", BinOp::NotEq), ("<", BinOp::Lt), ("<=", BinOp::LtEq),
            (">", BinOp::Gt), (">=", BinOp::GtEq), ("&&", BinOp::And), ("||", BinOp::Or),
        ] {
            assert_eq!(expr(&format!("a {} b", src)), *bin(op, var("a"), var("b")), "operator {}", src);
        }
    }

    // unary
    #[test]
    fn unary_neg_and_not() {
        assert_eq!(expr("-1"), Expr::Unary { op: UnOp::Neg, expr: int(1) });
        assert_eq!(expr("!x"), Expr::Unary { op: UnOp::Not, expr: var("x") });
    }

    #[test]
    fn unary_nests() {
        let inner = Box::new(Expr::Unary { op: UnOp::Neg, expr: int(1) });
        assert_eq!(expr("--1"), Expr::Unary { op: UnOp::Neg, expr: inner });
    }

    #[test]
    fn unary_binds_tighter_than_binary() {
        let neg = Box::new(Expr::Unary { op: UnOp::Neg, expr: var("a") });
        assert_eq!(expr("-a * b"), *bin(BinOp::Mul, neg, var("b")));
    }

    #[test]
    fn minus_after_operand_is_binary() {
        let neg = Box::new(Expr::Unary { op: UnOp::Neg, expr: int(2) });
        assert_eq!(expr("1 - -2"), *bin(BinOp::Sub, int(1), neg));
    }

    // calls
    #[test]
    fn call_without_args() {
        assert_eq!(expr("f()"), Expr::Call { callee: "f".into(), args: vec![] });
    }

    #[test]
    fn call_with_args() {
        let sum = *bin(BinOp::Add, int(2), int(3));
        assert_eq!(expr("f(1, 2 + 3)"), Expr::Call { callee: "f".into(), args: vec![Expr::Int(1), sum] });
    }

    #[test]
    fn call_as_argument_and_operand() {
        let g = Expr::Call { callee: "g".into(), args: vec![Expr::Int(1)] };
        let f = Expr::Call { callee: "f".into(), args: vec![g] };
        assert_eq!(expr("f(g(1)) + 1"), *bin(BinOp::Add, Box::new(f), int(1)));
    }

    // let and assignment
    #[test]
    fn let_inferred() {
        assert_eq!(program("let x = 1;"), vec![Stmt::Let { name: "x".into(), ty: None, init: Expr::Int(1) }]);
    }

    #[test]
    fn let_with_every_type() {
        for (src, ty) in [
            ("int", Ty::Int), ("float", Ty::Float), ("bool", Ty::Bool), ("str", Ty::Str),
            ("nil", Ty::Nil), ("Point", Ty::Named("Point".into())),
        ] {
            let stmts = program(&format!("let x: {} = y;", src));
            assert_eq!(stmts, vec![Stmt::Let { name: "x".into(), ty: Some(ty), init: Expr::Var("y".into()) }]);
        }
    }

    #[test]
    fn assignment() {
        assert_eq!(program("x = x + 1;"), vec![Stmt::Assign { name: "x".into(), value: *bin(BinOp::Add, var("x"), int(1)) }]);
    }

    #[test]
    fn equality_statement_is_not_assignment() {
        assert_eq!(program("x == 1;"), vec![Stmt::ExprStmt(*bin(BinOp::Eq, var("x"), int(1)))]);
    }

    // functions
    #[test]
    fn fn_without_params_or_return_type() {
        assert_eq!(program("fn f() { }"), vec![Stmt::Fn { name: "f".into(), params: vec![], ret: None, body: vec![] }]);
    }

    #[test]
    fn fn_with_params_and_return_type() {
        let stmts = program("fn add(a: int, b: float) -> int { return a; }");
        assert_eq!(stmts, vec![Stmt::Fn {
            name: "add".into(),
            params: vec![("a".into(), Ty::Int), ("b".into(), Ty::Float)],
            ret: Some(Ty::Int),
            body: vec![Stmt::Return(Expr::Var("a".into()))],
        }]);
    }

    #[test]
    fn return_without_value_is_nil() {
        assert_eq!(program("fn f() { return; }"), vec![Stmt::Fn {
            name: "f".into(), params: vec![], ret: None, body: vec![Stmt::Return(Expr::Nil)],
        }]);
    }

    // control flow
    #[test]
    fn if_without_else() {
        assert_eq!(program("if x { y; }"), vec![Stmt::If {
            cond: Expr::Var("x".into()), then: vec![Stmt::ExprStmt(Expr::Var("y".into()))], else_: None,
        }]);
    }

    #[test]
    fn if_with_else() {
        assert_eq!(program("if x { 1; } else { 2; }"), vec![Stmt::If {
            cond: Expr::Var("x".into()),
            then: vec![Stmt::ExprStmt(Expr::Int(1))],
            else_: Some(vec![Stmt::ExprStmt(Expr::Int(2))]),
        }]);
    }

    #[test]
    fn else_if_nests_inside_else() {
        let inner = Stmt::If { cond: Expr::Var("b".into()), then: vec![], else_: Some(vec![]) };
        assert_eq!(program("if a { } else if b { } else { }"), vec![Stmt::If {
            cond: Expr::Var("a".into()), then: vec![], else_: Some(vec![inner]),
        }]);
    }

    #[test]
    fn while_loop() {
        assert_eq!(program("while i < 3 { i = i + 1; }"), vec![Stmt::While {
            cond: *bin(BinOp::Lt, var("i"), int(3)),
            body: vec![Stmt::Assign { name: "i".into(), value: *bin(BinOp::Add, var("i"), int(1)) }],
        }]);
    }

    #[test]
    fn nested_blocks() {
        let stmts = program("while a { if b { let c = 1; } }");
        match &stmts[0] {
            Stmt::While { body, .. } => assert!(matches!(&body[0], Stmt::If { then, .. } if then.len() == 1)),
            other => panic!("expected while, got {:?}", other),
        }
    }

    // whole programs
    #[test] fn empty_program() { assert_eq!(program(""), vec![]); }
    #[test] fn comment_only_program() { assert_eq!(program("// nothing here\n"), vec![]); }
    #[test] fn several_statements() { assert_eq!(program("let x = 1; x = 2; x;").len(), 3); }

    #[test]
    fn last_semicolon_is_optional() {
        assert_eq!(program("1 + 2"), program("1 + 2;"));
        assert_eq!(program("let x = 1; x"), program("let x = 1; x;"));
    }

    // errors
    #[test]
    fn missing_semicolon_between_statements() {
        assert_eq!(err("let x = 1 let y = 2;").msg, "expected ';', found 'let'");
    }

    #[test]
    fn missing_semicolon_inside_block() {
        assert_eq!(err("if true { 1 }").msg, "expected ';', found '}'");
    }

    #[test]
    fn missing_expression() {
        assert_eq!(err("let x = ;").msg, "expected an expression, found ';'");
        assert_eq!(err("1 +").msg, "expected an expression, found end of input");
    }

    #[test]
    fn missing_closing_paren() {
        assert_eq!(err("(1 + 2;").msg, "expected ')', found ';'");
        assert_eq!(err("f(1, 2;").msg, "expected ')' after arguments, found ';'");
    }

    #[test]
    fn unclosed_block() {
        assert_eq!(err("while true { 1;").msg, "expected '}' to close the block, found end of input");
    }

    #[test]
    fn let_needs_name_and_initialiser() {
        assert_eq!(err("let = 1;").msg, "expected a variable name after 'let', found '='");
        assert_eq!(err("let x;").msg, "expected '=' in let statement, found ';'");
    }

    #[test]
    fn fn_param_needs_type() {
        assert_eq!(err("fn f(a) { }").msg, "expected ':' and a type after parameter name, found ')'");
        assert_eq!(err("fn f(a: 1) { }").msg, "expected a type name, found number 1");
    }

    #[test]
    fn if_needs_block() {
        assert_eq!(err("if true 1;").msg, "expected '{', found number 1");
    }

    #[test]
    fn assignment_target_must_be_a_name() {
        assert_eq!(err("1 = 2;").msg, "expected ';', found '='");
    }

    #[test]
    fn unknown_character_is_reported() {
        assert_eq!(err("let x = 1; @").msg, "expected a statement, found a character lumen does not use");
        assert_eq!(err("let x = 1 # 2;").msg, "expected ';', found a character lumen does not use");
    }

    #[test]
    fn error_position() {
        let e = err("let x = 1;\nlet y = ;");
        assert_eq!((e.line, e.col), (2, 9));
        assert_eq!(e.to_string(), "expected an expression, found ';' (line 2, col 9)");
    }

    #[test]
    fn deep_nesting_is_rejected_not_a_stack_overflow() {
        let src = format!("{}1{}", "(".repeat(10_000), ")".repeat(10_000));
        assert!(Parser::new(&src).parse_expr().unwrap_err().msg.contains("nesting"));
        assert!(Parser::new(&"-".repeat(10_000)).parse_expr().unwrap_err().msg.contains("nesting"));
        let src = format!("{}{}", "if true { ".repeat(10_000), "}".repeat(10_000));
        assert!(err(&src).msg.contains("nesting"));
    }

    #[test]
    fn moderate_nesting_is_accepted() {
        let src = format!("{}1{}", "(".repeat(50), ")".repeat(50));
        assert!(Parser::new(&src).parse_expr().is_ok());
    }
}
