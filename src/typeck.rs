//! Static type checker for Lumen.
//!
//! Single-pass: walks the AST, tracks a variable environment (scoped),
//! and returns typed errors with source context baked in via ParseError's
//! line/col — reused here as TypeError for simplicity.

use std::collections::HashMap;
use crate::ast::*;

// ── error type ────────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub struct TypeError {
    pub msg: String,
}

impl TypeError {
    fn new(msg: impl Into<String>) -> Self {
        Self { msg: msg.into() }
    }
}

pub type TyResult<T> = Result<T, TypeError>;

// ── environment (scoped variable map) ────────────────────────────────────────

#[derive(Debug, Default)]
struct Env {
    scopes: Vec<HashMap<String, Ty>>,
}

impl Env {
    fn push(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop(&mut self) {
        self.scopes.pop();
    }

    fn define(&mut self, name: &str, ty: Ty) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), ty);
        }
    }

    fn lookup(&self, name: &str) -> Option<&Ty> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty);
            }
        }
        None
    }
}

// ── function signature store ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct FnSig {
    pub params: Vec<Ty>,
    pub ret: Ty,
}

// ── type checker ─────────────────────────────────────────────────────────────

pub struct TypeChecker {
    env: Env,
    fns: HashMap<String, FnSig>,
    /// return type of the function we're currently inside (None = top-level)
    current_ret: Option<Ty>,
}

impl TypeChecker {
    pub fn new() -> Self {
        let mut tc = Self {
            env: Env::default(),
            fns: HashMap::new(),
            current_ret: None,
        };
        // global scope
        tc.env.push();
        // built-in: print(str) -> nil
        tc.fns.insert("print".into(), FnSig { params: vec![Ty::Str], ret: Ty::Nil });
        tc
    }

    /// Type-check a full program. Returns Ok(()) or the first error.
    pub fn check_program(&mut self, stmts: &[Stmt]) -> TyResult<()> {
        // first pass: register all fn signatures so forward calls work
        for stmt in stmts {
            if let Stmt::Fn { name, params, ret, .. } = stmt {
                let param_tys: Vec<Ty> = params.iter().map(|(_, ty)| ty.clone()).collect();
                let ret_ty = ret.clone().unwrap_or(Ty::Nil);
                self.fns.insert(name.clone(), FnSig { params: param_tys, ret: ret_ty });
            }
        }
        for stmt in stmts {
            self.check_stmt(stmt)?;
        }
        Ok(())
    }
}

// ── statement checking ────────────────────────────────────────────────────────

impl TypeChecker {
    fn check_stmt(&mut self, stmt: &Stmt) -> TyResult<()> {
        match stmt {
            Stmt::Let { name, ty, init } => {
                let init_ty = self.check_expr(init)?;
                if let Some(ann) = ty {
                    if !ty_compat(ann, &init_ty) {
                        return Err(TypeError::new(format!(
                            "type mismatch in let '{}': declared {:?} but got {:?}",
                            name, ann, init_ty
                        )));
                    }
                }
                self.env.define(name, ty.clone().unwrap_or(init_ty));
                Ok(())
            }

            Stmt::ExprStmt(expr) => {
                self.check_expr(expr)?;
                Ok(())
            }

            Stmt::Return(expr) => {
                let ret_ty = self.check_expr(expr)?;
                if let Some(expected) = &self.current_ret.clone() {
                    if !ty_compat(expected, &ret_ty) {
                        return Err(TypeError::new(format!(
                            "return type mismatch: expected {:?} got {:?}", expected, ret_ty
                        )));
                    }
                }
                Ok(())
            }

            Stmt::If { cond, then, else_ } => {
                let cond_ty = self.check_expr(cond)?;
                if cond_ty != Ty::Bool {
                    return Err(TypeError::new(format!(
                        "if condition must be bool, got {:?}", cond_ty
                    )));
                }
                self.env.push();
                for s in then { self.check_stmt(s)?; }
                self.env.pop();
                if let Some(else_stmts) = else_ {
                    self.env.push();
                    for s in else_stmts { self.check_stmt(s)?; }
                    self.env.pop();
                }
                Ok(())
            }

            Stmt::While { cond, body } => {
                let cond_ty = self.check_expr(cond)?;
                if cond_ty != Ty::Bool {
                    return Err(TypeError::new(format!(
                        "while condition must be bool, got {:?}", cond_ty
                    )));
                }
                self.env.push();
                for s in body { self.check_stmt(s)?; }
                self.env.pop();
                Ok(())
            }

            Stmt::Assign { name, value } => {
                let val_ty = self.check_expr(value)?;
                let var_ty = self.env.lookup(name)
                    .cloned()
                    .ok_or_else(|| TypeError::new(format!("assignment to undefined variable '{}'", name)))?;
                if !ty_compat(&var_ty, &val_ty) {
                    return Err(TypeError::new(format!(
                        "assignment type mismatch for '{}': expected {:?} got {:?}", name, var_ty, val_ty
                    )));
                }
                Ok(())
            }

            Stmt::Fn { name, params, ret, body } => {
                let ret_ty = ret.clone().unwrap_or(Ty::Nil);
                // register sig (may already exist from first pass — overwrite is fine)
                let param_tys: Vec<Ty> = params.iter().map(|(_, ty)| ty.clone()).collect();
                self.fns.insert(name.clone(), FnSig { params: param_tys, ret: ret_ty.clone() });

                // check body in a new scope with params defined
                let prev_ret = self.current_ret.replace(ret_ty);
                self.env.push();
                for (pname, pty) in params {
                    self.env.define(pname, pty.clone());
                }
                for s in body { self.check_stmt(s)?; }
                self.env.pop();
                self.current_ret = prev_ret;
                Ok(())
            }
        }
    }
}

// ── expression checking ───────────────────────────────────────────────────────

impl TypeChecker {
    fn check_expr(&mut self, expr: &Expr) -> TyResult<Ty> {
        match expr {
            Expr::Int(_)   => Ok(Ty::Int),
            Expr::Float(_) => Ok(Ty::Float),
            Expr::Str(_)   => Ok(Ty::Str),
            Expr::Bool(_)  => Ok(Ty::Bool),
            Expr::Nil      => Ok(Ty::Nil),

            Expr::Var(name) => {
                self.env.lookup(name)
                    .cloned()
                    .ok_or_else(|| TypeError::new(format!("undefined variable '{}'", name)))
            }

            Expr::Group(inner) => self.check_expr(inner),

            Expr::Unary { op, expr } => {
                let ty = self.check_expr(expr)?;
                match op {
                    UnOp::Neg => {
                        if ty == Ty::Int || ty == Ty::Float {
                            Ok(ty)
                        } else {
                            Err(TypeError::new(format!("unary '-' requires int/float, got {:?}", ty)))
                        }
                    }
                    UnOp::Not => {
                        if ty == Ty::Bool {
                            Ok(Ty::Bool)
                        } else {
                            Err(TypeError::new(format!("unary '!' requires bool, got {:?}", ty)))
                        }
                    }
                }
            }

            Expr::Binary { op, lhs, rhs } => {
                let lt = self.check_expr(lhs)?;
                let rt = self.check_expr(rhs)?;
                check_binary(op, &lt, &rt)
            }

            Expr::Call { callee, args } => {
                let sig = self.fns.get(callee).cloned().ok_or_else(|| {
                    TypeError::new(format!("undefined function '{}'", callee))
                })?;
                if args.len() != sig.params.len() {
                    return Err(TypeError::new(format!(
                        "function '{}' expects {} args, got {}",
                        callee, sig.params.len(), args.len()
                    )));
                }
                for (arg, expected) in args.iter().zip(sig.params.iter()) {
                    let got = self.check_expr(arg)?;
                    if !ty_compat(expected, &got) {
                        return Err(TypeError::new(format!(
                            "argument type mismatch in call to '{}': expected {:?} got {:?}",
                            callee, expected, got
                        )));
                    }
                }
                Ok(sig.ret.clone())
            }
        }
    }
}

// ── binary operator type rules ────────────────────────────────────────────────

fn check_binary(op: &BinOp, lt: &Ty, rt: &Ty) -> TyResult<Ty> {
    match op {
        // arithmetic: int op int -> int, float op float -> float
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
            match (lt, rt) {
                (Ty::Int,   Ty::Int)   => Ok(Ty::Int),
                (Ty::Float, Ty::Float) => Ok(Ty::Float),
                // str + str -> str (concatenation)
                (Ty::Str,   Ty::Str) if *op == BinOp::Add => Ok(Ty::Str),
                _ => Err(TypeError::new(format!(
                    "arithmetic op {:?} not valid for {:?} and {:?}", op, lt, rt
                ))),
            }
        }
        // comparison: same numeric type -> bool
        BinOp::Lt | BinOp::LtEq | BinOp::Gt | BinOp::GtEq => {
            match (lt, rt) {
                (Ty::Int,   Ty::Int)   => Ok(Ty::Bool),
                (Ty::Float, Ty::Float) => Ok(Ty::Bool),
                _ => Err(TypeError::new(format!(
                    "comparison op {:?} requires matching numeric types, got {:?} and {:?}", op, lt, rt
                ))),
            }
        }
        // equality: same type -> bool
        BinOp::Eq | BinOp::NotEq => {
            if ty_compat(lt, rt) || ty_compat(rt, lt) {
                Ok(Ty::Bool)
            } else {
                Err(TypeError::new(format!(
                    "equality op requires same types, got {:?} and {:?}", lt, rt
                )))
            }
        }
        // logical: bool op bool -> bool
        BinOp::And | BinOp::Or => {
            if lt == &Ty::Bool && rt == &Ty::Bool {
                Ok(Ty::Bool)
            } else {
                Err(TypeError::new(format!(
                    "logical op {:?} requires bool operands, got {:?} and {:?}", op, lt, rt
                )))
            }
        }
    }
}

/// Structural type compatibility (for now: exact match or Named wildcard).
fn ty_compat(expected: &Ty, got: &Ty) -> bool {
    expected == got || matches!(expected, Ty::Named(_)) || matches!(got, Ty::Named(_))
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;

    fn check(src: &str) -> TyResult<()> {
        let stmts = Parser::new(src).parse_program().expect("parse error");
        TypeChecker::new().check_program(&stmts)
    }

    fn check_expr_ty(src: &str) -> TyResult<Ty> {
        let expr = Parser::new(src).parse_expr().expect("parse error");
        TypeChecker::new().check_expr(&expr)
    }

    // literal types
    #[test] fn int_literal_ty()   { assert_eq!(check_expr_ty("1"),     Ok(Ty::Int)); }
    #[test] fn float_literal_ty() { assert_eq!(check_expr_ty("1.0"),   Ok(Ty::Float)); }
    #[test] fn bool_literal_ty()  { assert_eq!(check_expr_ty("true"),  Ok(Ty::Bool)); }
    #[test] fn str_literal_ty()   { assert_eq!(check_expr_ty(r#""hi""#), Ok(Ty::Str)); }
    #[test] fn nil_literal_ty()   { assert_eq!(check_expr_ty("nil"),   Ok(Ty::Nil)); }

    // arithmetic
    #[test] fn int_add()   { assert_eq!(check_expr_ty("1 + 2"),     Ok(Ty::Int)); }
    #[test] fn float_add() { assert_eq!(check_expr_ty("1.0 + 2.0"), Ok(Ty::Float)); }
    #[test] fn str_concat(){ assert_eq!(check_expr_ty(r#""a" + "b""#), Ok(Ty::Str)); }

    #[test]
    fn int_float_mismatch() {
        assert!(check_expr_ty("1 + 1.0").is_err());
    }

    // comparison
    #[test] fn int_lt()   { assert_eq!(check_expr_ty("1 < 2"),     Ok(Ty::Bool)); }
    #[test] fn float_gt() { assert_eq!(check_expr_ty("1.0 > 2.0"), Ok(Ty::Bool)); }

    #[test]
    fn compare_type_mismatch() {
        assert!(check_expr_ty("1 < 1.0").is_err());
    }

    // equality
    #[test] fn int_eq()  { assert_eq!(check_expr_ty("1 == 1"),   Ok(Ty::Bool)); }
    #[test] fn bool_neq(){ assert_eq!(check_expr_ty("true != false"), Ok(Ty::Bool)); }

    #[test]
    fn eq_type_mismatch() {
        assert!(check_expr_ty("1 == true").is_err());
    }

    // logical
    #[test] fn bool_and() { assert_eq!(check_expr_ty("true && false"), Ok(Ty::Bool)); }
    #[test] fn bool_or()  { assert_eq!(check_expr_ty("true || false"), Ok(Ty::Bool)); }

    #[test]
    fn logical_non_bool() {
        assert!(check_expr_ty("1 && 2").is_err());
    }

    // unary
    #[test] fn unary_neg_int()   { assert_eq!(check_expr_ty("-1"),    Ok(Ty::Int)); }
    #[test] fn unary_neg_float() { assert_eq!(check_expr_ty("-1.0"),  Ok(Ty::Float)); }
    #[test] fn unary_not_bool()  { assert_eq!(check_expr_ty("!true"), Ok(Ty::Bool)); }

    #[test]
    fn unary_not_non_bool() {
        assert!(check_expr_ty("!1").is_err());
    }

    #[test]
    fn unary_neg_bool_err() {
        assert!(check_expr_ty("-true").is_err());
    }

    // let statements
    #[test]
    fn let_inferred() {
        assert!(check("let x = 1;").is_ok());
    }

    #[test]
    fn let_annotated_match() {
        assert!(check("let x: int = 1;").is_ok());
    }

    #[test]
    fn let_annotated_mismatch() {
        assert!(check("let x: float = 1;").is_err());
    }

    // variable resolution
    #[test]
    fn var_defined() {
        assert!(check("let x = 1; let y = x;").is_ok());
    }

    #[test]
    fn var_undefined() {
        assert!(check("let y = x;").is_err());
    }

    // if/while condition must be bool
    #[test]
    fn if_bool_cond() {
        assert!(check("if true { let x = 1; }").is_ok());
    }

    #[test]
    fn if_non_bool_cond() {
        assert!(check("if 1 { let x = 1; }").is_err());
    }

    #[test]
    fn while_bool_cond() {
        assert!(check("while true { let x = 1; }").is_ok());
    }

    #[test]
    fn while_non_bool_cond() {
        assert!(check("while 1 { let x = 1; }").is_err());
    }

    // function declarations
    #[test]
    fn fn_decl_ok() {
        assert!(check("fn add(a: int, b: int) { let c = a; }").is_ok());
    }

    #[test]
    fn fn_return_type_mismatch() {
        assert!(check("fn f() -> int { return true; }").is_err());
    }

    #[test]
    fn fn_call_ok() {
        assert!(check("fn double(x: int) { let y = x; } double(1);").is_ok());
    }

    #[test]
    fn fn_call_wrong_arg_count() {
        assert!(check("fn f(x: int) { let y = x; } f(1, 2);").is_err());
    }

    #[test]
    fn fn_call_wrong_arg_type() {
        assert!(check("fn f(x: int) { let y = x; } f(true);").is_err());
    }

    #[test]
    fn fn_undefined() {
        assert!(check("foo();").is_err());
    }

    // assignment
    #[test]
    fn assign_ok() {
        assert!(check("let x = 1; x = 2;").is_ok());
    }

    #[test]
    fn assign_type_mismatch() {
        assert!(check("let x = 1; x = true;").is_err());
    }

    #[test]
    fn assign_undefined() {
        assert!(check("x = 1;").is_err());
    }

    // scoping
    #[test]
    fn scope_isolation() {
        // variable defined inside if block not visible outside
        assert!(check("if true { let x = 1; } let y = x;").is_err());
    }

    #[test]
    fn nested_scope_ok() {
        assert!(check("let x = 1; if true { let y = x; }").is_ok());
    }
}
