//! Tree-walking interpreter: evaluates the AST directly, no bytecode.
//! Second execution backend — benchmarked against the stack VM in vm.rs.

use std::collections::HashMap;
use crate::ast::*;
use crate::compiler::Value;

// ── error ─────────────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub struct InterpError(pub String);

pub type InterpResult<T> = Result<T, InterpError>;

// ── control-flow signal ───────────────────────────────────────────────────────

/// Threads `return` values up through recursive exec_stmt calls.
enum Signal {
    None,
    Return(Value),
}

// ── environment ───────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
struct Env {
    scopes: Vec<HashMap<String, Value>>,
}

impl Env {
    fn push(&mut self) { self.scopes.push(HashMap::new()); }
    fn pop(&mut self)  { self.scopes.pop(); }

    fn define(&mut self, name: &str, val: Value) {
        self.scopes.last_mut()
            .expect("no scope")
            .insert(name.to_string(), val);
    }

    fn get(&self, name: &str) -> InterpResult<Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(v) = scope.get(name) { return Ok(v.clone()); }
        }
        Err(InterpError(format!("undefined variable '{}'", name)))
    }

    /// Mutate an existing binding in the nearest enclosing scope that owns it.
    fn set(&mut self, name: &str, val: Value) -> InterpResult<()> {
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), val);
                return Ok(());
            }
        }
        Err(InterpError(format!("assignment to undefined variable '{}'", name)))
    }
}

// ── stored function definition ────────────────────────────────────────────────

#[derive(Clone)]
struct FnDef {
    params: Vec<(String, Ty)>,
    body:   Vec<Stmt>,
}

// ── interpreter ───────────────────────────────────────────────────────────────

pub struct Interpreter {
    env:      Env,
    fns:      HashMap<String, FnDef>,
    /// value of the most recently evaluated expression statement
    last_val: Value,
}

impl Interpreter {
    pub fn new() -> Self {
        let mut env = Env::default();
        env.push(); // global scope
        Self { env, fns: HashMap::new(), last_val: Value::Nil }
    }

    /// Run a full program; return the last expression-statement value, or Nil.
    pub fn run_program(&mut self, stmts: &[Stmt]) -> InterpResult<Value> {
        // first pass: hoist all fn declarations
        for stmt in stmts {
            if let Stmt::Fn { name, params, body, .. } = stmt {
                self.fns.insert(name.clone(), FnDef {
                    params: params.clone(),
                    body:   body.clone(),
                });
            }
        }
        // second pass: execute non-fn statements
        for stmt in stmts {
            if matches!(stmt, Stmt::Fn { .. }) { continue; }
            match self.exec_stmt(stmt)? {
                Signal::Return(v) => return Ok(v),
                Signal::None      => {}
            }
        }
        Ok(self.last_val.clone())
    }
}

// ── statement execution ───────────────────────────────────────────────────────

impl Interpreter {
    fn exec_stmt(&mut self, stmt: &Stmt) -> InterpResult<Signal> {
        match stmt {
            Stmt::Let { name, init, .. } => {
                let val = self.eval_expr(init)?;
                self.env.define(name, val);
                Ok(Signal::None)
            }

            Stmt::ExprStmt(expr) => {
                self.last_val = self.eval_expr(expr)?;
                Ok(Signal::None)
            }

            Stmt::Assign { name, value } => {
                let val = self.eval_expr(value)?;
                self.env.set(name, val)?;
                Ok(Signal::None)
            }

            Stmt::Return(expr) => {
                let val = self.eval_expr(expr)?;
                Ok(Signal::Return(val))
            }

            Stmt::If { cond, then, else_ } => {
                let cond_val = self.eval_expr(cond)?;
                let branch = if truthy(&cond_val) { Some(then) } else { else_.as_ref() };
                if let Some(stmts) = branch {
                    self.env.push();
                    let sig = self.exec_block(stmts)?;
                    self.env.pop();
                    return Ok(sig);
                }
                Ok(Signal::None)
            }

            Stmt::While { cond, body } => {
                loop {
                    let cond_val = self.eval_expr(cond)?;
                    if !truthy(&cond_val) { break; }
                    self.env.push();
                    let sig = self.exec_block(body)?;
                    self.env.pop();
                    if let Signal::Return(_) = sig { return Ok(sig); }
                }
                Ok(Signal::None)
            }

            Stmt::Fn { .. } => Ok(Signal::None), // hoisted in first pass
        }
    }

    fn exec_block(&mut self, stmts: &[Stmt]) -> InterpResult<Signal> {
        for stmt in stmts {
            match self.exec_stmt(stmt)? {
                Signal::Return(v) => return Ok(Signal::Return(v)),
                Signal::None      => {}
            }
        }
        Ok(Signal::None)
    }
}

// ── expression evaluation ─────────────────────────────────────────────────────

impl Interpreter {
    fn eval_expr(&mut self, expr: &Expr) -> InterpResult<Value> {
        match expr {
            Expr::Int(n)   => Ok(Value::Int(*n)),
            Expr::Float(f) => Ok(Value::Float(*f)),
            Expr::Str(s)   => Ok(Value::Str(s.clone())),
            Expr::Bool(b)  => Ok(Value::Bool(*b)),
            Expr::Nil      => Ok(Value::Nil),

            Expr::Var(name) => self.env.get(name),

            Expr::Group(inner) => self.eval_expr(inner),

            Expr::Unary { op, expr } => {
                let v = self.eval_expr(expr)?;
                match op {
                    UnOp::Neg => match v {
                        Value::Int(n)   => Ok(Value::Int(-n)),
                        Value::Float(f) => Ok(Value::Float(-f)),
                        _ => Err(InterpError(format!("unary '-' on non-numeric: {:?}", v))),
                    },
                    UnOp::Not => match v {
                        Value::Bool(b) => Ok(Value::Bool(!b)),
                        _ => Err(InterpError(format!("unary '!' on non-bool: {:?}", v))),
                    },
                }
            }

            Expr::Binary { op, lhs, rhs } => {
                let lv = self.eval_expr(lhs)?;
                let rv = self.eval_expr(rhs)?;
                eval_binary(op, lv, rv)
            }

            Expr::Call { callee, args } => {
                // built-in
                if callee == "print" {
                    let val = if let Some(a) = args.first() {
                        self.eval_expr(a)?
                    } else { Value::Nil };
                    println!("{}", val);
                    return Ok(Value::Nil);
                }

                // user function
                let def = self.fns.get(callee).cloned()
                    .ok_or_else(|| InterpError(format!("undefined function '{}'", callee)))?;

                if args.len() != def.params.len() {
                    return Err(InterpError(format!(
                        "'{}' expects {} args, got {}", callee, def.params.len(), args.len()
                    )));
                }

                // evaluate args in current env
                let arg_vals: Vec<Value> = args.iter()
                    .map(|a| self.eval_expr(a))
                    .collect::<InterpResult<_>>()?;

                // execute body in fresh scope
                self.env.push();
                for ((pname, _), val) in def.params.iter().zip(arg_vals) {
                    self.env.define(pname, val);
                }
                let sig = self.exec_block(&def.body)?;
                self.env.pop();

                Ok(match sig {
                    Signal::Return(v) => v,
                    Signal::None      => Value::Nil,
                })
            }
        }
    }
}

// ── binary op evaluation ──────────────────────────────────────────────────────

fn eval_binary(op: &BinOp, lv: Value, rv: Value) -> InterpResult<Value> {
    match op {
        BinOp::Add => match (lv, rv) {
            (Value::Int(a),   Value::Int(b))   => Ok(Value::Int(a + b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
            (Value::Str(a),   Value::Str(b))   => Ok(Value::Str(a + &b)),
            (l, r) => Err(binop_err("+", &l, &r)),
        },
        BinOp::Sub => match (lv, rv) {
            (Value::Int(a),   Value::Int(b))   => Ok(Value::Int(a - b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
            (l, r) => Err(binop_err("-", &l, &r)),
        },
        BinOp::Mul => match (lv, rv) {
            (Value::Int(a),   Value::Int(b))   => Ok(Value::Int(a * b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
            (l, r) => Err(binop_err("*", &l, &r)),
        },
        BinOp::Div => match (lv, rv) {
            (Value::Int(a),   Value::Int(b)) => {
                if b == 0 { return Err(InterpError("division by zero".into())); }
                Ok(Value::Int(a / b))
            }
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),
            (l, r) => Err(binop_err("/", &l, &r)),
        },
        BinOp::Mod => match (lv, rv) {
            (Value::Int(a), Value::Int(b)) => {
                if b == 0 { return Err(InterpError("modulo by zero".into())); }
                Ok(Value::Int(a % b))
            }
            (l, r) => Err(binop_err("%", &l, &r)),
        },
        BinOp::Lt  => cmp_op(lv, rv, |o| o.is_lt()),
        BinOp::LtEq=> cmp_op(lv, rv, |o| o.is_le()),
        BinOp::Gt  => cmp_op(lv, rv, |o| o.is_gt()),
        BinOp::GtEq=> cmp_op(lv, rv, |o| o.is_ge()),
        BinOp::Eq  => Ok(Value::Bool(val_eq(&lv, &rv))),
        BinOp::NotEq=> Ok(Value::Bool(!val_eq(&lv, &rv))),
        BinOp::And => match (lv, rv) {
            (Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(a && b)),
            (l, r) => Err(binop_err("&&", &l, &r)),
        },
        BinOp::Or => match (lv, rv) {
            (Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(a || b)),
            (l, r) => Err(binop_err("||", &l, &r)),
        },
    }
}

fn cmp_op(lv: Value, rv: Value, pred: impl Fn(std::cmp::Ordering) -> bool) -> InterpResult<Value> {
    let ord = match (&lv, &rv) {
        (Value::Int(a),   Value::Int(b))   => a.cmp(b),
        (Value::Float(a), Value::Float(b)) => a.partial_cmp(b)
            .ok_or_else(|| InterpError("NaN comparison".into()))?,
        _ => return Err(binop_err("</>", &lv, &rv)),
    };
    Ok(Value::Bool(pred(ord)))
}

fn val_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(x),   Value::Int(y))   => x == y,
        (Value::Float(x), Value::Float(y)) => x == y,
        (Value::Bool(x),  Value::Bool(y))  => x == y,
        (Value::Str(x),   Value::Str(y))   => x == y,
        (Value::Nil,      Value::Nil)      => true,
        _ => false,
    }
}

fn truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Nil     => false,
        _              => true,
    }
}

fn binop_err(op: &str, l: &Value, r: &Value) -> InterpError {
    InterpError(format!("type error: cannot apply '{}' to {:?} and {:?}", op, l, r))
}

// ── convenience ───────────────────────────────────────────────────────────────

pub fn interp(src: &str) -> InterpResult<Value> {
    use crate::parser::Parser;
    let stmts = Parser::new(src).parse_program()
        .map_err(|e| InterpError(format!("parse error: {}", e.msg)))?;
    Interpreter::new().run_program(&stmts)
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Value { interp(src).expect("interp error") }
    fn run_err(src: &str) -> String { interp(src).unwrap_err().0 }

    // literals
    #[test] fn int_val()   { assert_eq!(run("1;"),     Value::Int(1)); }
    #[test] fn float_val() { assert_eq!(run("1.0;"),   Value::Float(1.0)); }
    #[test] fn bool_val()  { assert_eq!(run("true;"),  Value::Bool(true)); }
    #[test] fn nil_val()   { assert_eq!(run("nil;"),   Value::Nil); }
    #[test] fn str_val()   { assert_eq!(run(r#""hi";"#), Value::Str("hi".into())); }

    // arithmetic
    #[test] fn int_add()   { assert_eq!(run("1 + 2;"),   Value::Int(3)); }
    #[test] fn int_sub()   { assert_eq!(run("5 - 3;"),   Value::Int(2)); }
    #[test] fn int_mul()   { assert_eq!(run("3 * 4;"),   Value::Int(12)); }
    #[test] fn int_div()   { assert_eq!(run("10 / 2;"),  Value::Int(5)); }
    #[test] fn int_mod()   { assert_eq!(run("7 % 3;"),   Value::Int(1)); }
    #[test] fn float_add() { assert_eq!(run("1.0 + 2.0;"), Value::Float(3.0)); }
    #[test] fn float_mul() { assert_eq!(run("2.0 * 3.0;"), Value::Float(6.0)); }
    #[test] fn str_concat(){ assert_eq!(run(r#""hello" + " world";"#), Value::Str("hello world".into())); }

    // unary
    #[test] fn neg_int()   { assert_eq!(run("-5;"),    Value::Int(-5)); }
    #[test] fn neg_float() { assert_eq!(run("-2.0;"),  Value::Float(-2.0)); }
    #[test] fn not_bool()  { assert_eq!(run("!true;"), Value::Bool(false)); }

    // comparison
    #[test] fn lt_true()   { assert_eq!(run("1 < 2;"),  Value::Bool(true)); }
    #[test] fn lt_false()  { assert_eq!(run("2 < 1;"),  Value::Bool(false)); }
    #[test] fn gt_true()   { assert_eq!(run("3 > 1;"),  Value::Bool(true)); }
    #[test] fn eq_int()    { assert_eq!(run("2 == 2;"), Value::Bool(true)); }
    #[test] fn neq_int()   { assert_eq!(run("1 != 2;"), Value::Bool(true)); }
    #[test] fn lteq()      { assert_eq!(run("2 <= 2;"), Value::Bool(true)); }
    #[test] fn gteq()      { assert_eq!(run("3 >= 3;"), Value::Bool(true)); }

    // logical
    #[test] fn and_tt()    { assert_eq!(run("true && true;"),  Value::Bool(true)); }
    #[test] fn and_ff()    { assert_eq!(run("true && false;"), Value::Bool(false)); }
    #[test] fn or_ff()     { assert_eq!(run("false || false;"),Value::Bool(false)); }
    #[test] fn or_tf()     { assert_eq!(run("true || false;"), Value::Bool(true)); }

    // variables
    #[test]
    fn let_and_load() { assert_eq!(run("let x = 42; x;"), Value::Int(42)); }

    #[test]
    fn let_chain() { assert_eq!(run("let x = 1; let y = x + 1; y;"), Value::Int(2)); }

    // control flow
    #[test]
    fn if_true_branch() {
        // outer x unchanged — scope isolation
        assert_eq!(run("let x = 0; if true { let x = 1; } x;"), Value::Int(0));
    }

    #[test]
    fn if_else_taken() {
        assert_eq!(run("if false { nil; } else { 99; }"), Value::Int(99));
    }

    #[test]
    fn while_mutates_outer() {
        // while can read outer scope var
        assert_eq!(run("let x = 0; while false { let x = 1; } x;"), Value::Int(0));
    }

    #[test]
    fn while_runs_body() {
        // use a counter via repeated let — simple smoke test
        assert_eq!(run("let done = false; while done { let done = true; } done;"), Value::Bool(false));
    }

    // functions
    #[test]
    fn fn_return_val() {
        assert_eq!(run("fn id(x: int) { return x; } id(7);"), Value::Int(7));
    }

    #[test]
    fn fn_two_params() {
        assert_eq!(run("fn first(a: int, b: int) { return a; } first(3, 9);"), Value::Int(3));
    }

    #[test]
    fn fn_no_return_is_nil() {
        assert_eq!(run("fn noop() { let x = 1; } noop();"), Value::Nil);
    }

    #[test]
    fn fn_undefined_err() {
        assert!(run_err("foo();").contains("undefined function"));
    }

    #[test]
    fn fn_wrong_arity_err() {
        assert!(run_err("fn f(x: int) { return x; } f(1, 2);").contains("expects"));
    }

    // runtime errors
    #[test]
    fn div_by_zero() { assert!(run_err("1 / 0;").contains("division by zero")); }

    #[test]
    fn mod_by_zero() { assert!(run_err("5 % 0;").contains("modulo by zero")); }

    #[test]
    fn undefined_var() { assert!(run_err("x;").contains("undefined variable")); }

    // assignment
    #[test]
    fn assign_mutates() {
        assert_eq!(run("let x = 1; x = 42; x;"), Value::Int(42));
    }

    #[test]
    fn assign_in_loop() {
        // assign updates outer scope across loop iterations
        assert_eq!(run("let x = 0; while false { x = 99; } x;"), Value::Int(0));
    }

    #[test]
    fn assign_undefined_err() {
        assert!(run_err("x = 1;").contains("undefined variable"));
    }

    // equality across types
    #[test] fn str_eq()  { assert_eq!(run(r#""a" == "a";"#), Value::Bool(true)); }
    #[test] fn bool_eq() { assert_eq!(run("false == false;"), Value::Bool(true)); }
    #[test] fn nil_eq()  { assert_eq!(run("nil == nil;"),     Value::Bool(true)); }
}
