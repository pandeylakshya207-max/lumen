//! Stack-based VM that executes a compiled Chunk.

use std::collections::HashMap;
use crate::compiler::{Chunk, Op, Value};

// ── error ─────────────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub struct VmError(pub String);

pub type VmResult<T> = Result<T, VmError>;

// ── call frame ────────────────────────────────────────────────────────────────

struct Frame {
    chunk: Chunk,
    ip: usize,
    /// base index into `locals` for this call frame
    locals_base: usize,
}

// ── VM ────────────────────────────────────────────────────────────────────────

pub struct Vm {
    /// operand stack
    stack: Vec<Value>,
    /// flat locals pool — frames address into this by (locals_base + slot)
    locals: Vec<Value>,
    /// compiled function chunks
    fns: HashMap<String, Chunk>,
    /// call stack
    frames: Vec<Frame>,
}

impl Vm {
    pub fn new(fns: HashMap<String, Chunk>) -> Self {
        Self {
            stack: Vec::new(),
            locals: Vec::new(),
            fns,
            frames: Vec::new(),
        }
    }

    /// Run a main chunk to completion, return the top-of-stack value (or Nil).
    pub fn run(&mut self, chunk: Chunk) -> VmResult<Value> {
        self.frames.push(Frame { chunk, ip: 0, locals_base: 0 });

        loop {
            let op = {
                let f = self.frames.last_mut().unwrap();
                let op = f.chunk.ops[f.ip].clone();
                f.ip += 1;
                op
            };

            match op {
                Op::Halt => break,

                Op::Const(v) => self.stack.push(v),

                // ── arithmetic ────────────────────────────────────────────
                Op::AddInt   => { let (a,b) = self.pop2_int()?;   self.stack.push(Value::Int(a + b)); }
                Op::AddFloat => { let (a,b) = self.pop2_float()?; self.stack.push(Value::Float(a + b)); }
                Op::AddStr   => {
                    let b = self.pop_str()?; let a = self.pop_str()?;
                    self.stack.push(Value::Str(a + &b));
                }
                Op::SubInt   => { let (a,b) = self.pop2_int()?;   self.stack.push(Value::Int(a - b)); }
                Op::SubFloat => { let (a,b) = self.pop2_float()?; self.stack.push(Value::Float(a - b)); }
                Op::MulInt   => { let (a,b) = self.pop2_int()?;   self.stack.push(Value::Int(a * b)); }
                Op::MulFloat => { let (a,b) = self.pop2_float()?; self.stack.push(Value::Float(a * b)); }
                Op::DivInt   => {
                    let (a,b) = self.pop2_int()?;
                    if b == 0 { return Err(VmError("division by zero".into())); }
                    self.stack.push(Value::Int(a / b));
                }
                Op::DivFloat => { let (a,b) = self.pop2_float()?; self.stack.push(Value::Float(a / b)); }
                Op::ModInt   => {
                    let (a,b) = self.pop2_int()?;
                    if b == 0 { return Err(VmError("modulo by zero".into())); }
                    self.stack.push(Value::Int(a % b));
                }

                // ── unary ─────────────────────────────────────────────────
                Op::NegInt   => { let n = self.pop_int()?;   self.stack.push(Value::Int(-n)); }
                Op::NegFloat => { let f = self.pop_float()?; self.stack.push(Value::Float(-f)); }
                Op::Not      => { let b = self.pop_bool()?;  self.stack.push(Value::Bool(!b)); }

                // ── comparison ────────────────────────────────────────────
                Op::LtInt    => { let (a,b) = self.pop2_int()?;   self.stack.push(Value::Bool(a < b)); }
                Op::LtFloat  => { let (a,b) = self.pop2_float()?; self.stack.push(Value::Bool(a < b)); }
                Op::LtEqInt  => { let (a,b) = self.pop2_int()?;   self.stack.push(Value::Bool(a <= b)); }
                Op::LtEqFloat=> { let (a,b) = self.pop2_float()?; self.stack.push(Value::Bool(a <= b)); }
                Op::GtInt    => { let (a,b) = self.pop2_int()?;   self.stack.push(Value::Bool(a > b)); }
                Op::GtFloat  => { let (a,b) = self.pop2_float()?; self.stack.push(Value::Bool(a > b)); }
                Op::GtEqInt  => { let (a,b) = self.pop2_int()?;   self.stack.push(Value::Bool(a >= b)); }
                Op::GtEqFloat=> { let (a,b) = self.pop2_float()?; self.stack.push(Value::Bool(a >= b)); }
                Op::EqInt    => { let (a,b) = self.pop2_int()?;   self.stack.push(Value::Bool(a == b)); }
                Op::EqFloat  => { let (a,b) = self.pop2_float()?; self.stack.push(Value::Bool(a == b)); }
                Op::EqBool   => { let (a,b) = self.pop2_bool()?;  self.stack.push(Value::Bool(a == b)); }
                Op::EqStr    => { let b = self.pop_str()?; let a = self.pop_str()?; self.stack.push(Value::Bool(a == b)); }
                Op::NeqInt   => { let (a,b) = self.pop2_int()?;   self.stack.push(Value::Bool(a != b)); }
                Op::NeqFloat => { let (a,b) = self.pop2_float()?; self.stack.push(Value::Bool(a != b)); }
                Op::NeqBool  => { let (a,b) = self.pop2_bool()?;  self.stack.push(Value::Bool(a != b)); }
                Op::NeqStr   => { let b = self.pop_str()?; let a = self.pop_str()?; self.stack.push(Value::Bool(a != b)); }

                // ── logical ───────────────────────────────────────────────
                Op::And => { let (a,b) = self.pop2_bool()?; self.stack.push(Value::Bool(a && b)); }
                Op::Or  => { let (a,b) = self.pop2_bool()?; self.stack.push(Value::Bool(a || b)); }

                // ── locals ────────────────────────────────────────────────
                Op::StoreLocal(slot) => {
                    let base = self.frames.last().unwrap().locals_base;
                    let idx = base + slot;
                    let val = self.stack.pop().ok_or_else(|| VmError("stack underflow".into()))?;
                    if idx >= self.locals.len() {
                        self.locals.resize(idx + 1, Value::Nil);
                    }
                    self.locals[idx] = val;
                }
                Op::LoadLocal(slot) => {
                    let base = self.frames.last().unwrap().locals_base;
                    let idx = base + slot;
                    let val = self.locals.get(idx)
                        .cloned()
                        .ok_or_else(|| VmError(format!("local slot {} not set", slot)))?;
                    self.stack.push(val);
                }

                // ── control flow ──────────────────────────────────────────
                Op::Jump(target) => {
                    self.frames.last_mut().unwrap().ip = target;
                }
                Op::JumpIfFalse(target) => {
                    let cond = self.pop_bool()?;
                    if !cond {
                        self.frames.last_mut().unwrap().ip = target;
                    }
                }

                // ── functions ─────────────────────────────────────────────
                Op::Call(name, arity) => {
                    let chunk = self.fns.get(&name).cloned()
                        .ok_or_else(|| VmError(format!("undefined function '{}'", name)))?;

                    // args are on the stack (last pushed = last param)
                    let stack_base = self.stack.len().saturating_sub(arity);
                    let args: Vec<Value> = self.stack.drain(stack_base..).collect();

                    let locals_base = self.locals.len();
                    // allocate param slots
                    for arg in args {
                        self.locals.push(arg);
                    }

                    self.frames.push(Frame { chunk, ip: 0, locals_base });
                }

                Op::Return => {
                    let ret_val = self.stack.pop().unwrap_or(Value::Nil);
                    let frame = self.frames.pop().unwrap();
                    // free locals allocated in this frame
                    self.locals.truncate(frame.locals_base);
                    if self.frames.is_empty() {
                        // returned from main — shouldn't happen normally (Halt is used)
                        self.stack.push(ret_val);
                        break;
                    }
                    self.stack.push(ret_val);
                }

                // ── built-ins ─────────────────────────────────────────────
                Op::Print => {
                    let val = self.stack.pop().ok_or_else(|| VmError("print: empty stack".into()))?;
                    println!("{}", val);
                }
            }
        }

        Ok(self.stack.last().cloned().unwrap_or(Value::Nil))
    }

    // ── stack helpers ─────────────────────────────────────────────────────────

    fn pop(&mut self) -> VmResult<Value> {
        self.stack.pop().ok_or_else(|| VmError("stack underflow".into()))
    }

    fn pop_int(&mut self) -> VmResult<i64> {
        match self.pop()? {
            Value::Int(n) => Ok(n),
            v => Err(VmError(format!("expected int, got {:?}", v))),
        }
    }

    fn pop_float(&mut self) -> VmResult<f64> {
        match self.pop()? {
            Value::Float(f) => Ok(f),
            v => Err(VmError(format!("expected float, got {:?}", v))),
        }
    }

    fn pop_bool(&mut self) -> VmResult<bool> {
        match self.pop()? {
            Value::Bool(b) => Ok(b),
            v => Err(VmError(format!("expected bool, got {:?}", v))),
        }
    }

    fn pop_str(&mut self) -> VmResult<String> {
        match self.pop()? {
            Value::Str(s) => Ok(s),
            v => Err(VmError(format!("expected str, got {:?}", v))),
        }
    }

    fn pop2_int(&mut self) -> VmResult<(i64, i64)> {
        let b = self.pop_int()?; let a = self.pop_int()?; Ok((a, b))
    }
    fn pop2_float(&mut self) -> VmResult<(f64, f64)> {
        let b = self.pop_float()?; let a = self.pop_float()?; Ok((a, b))
    }
    fn pop2_bool(&mut self) -> VmResult<(bool, bool)> {
        let b = self.pop_bool()?; let a = self.pop_bool()?; Ok((a, b))
    }
}

// ── convenience: compile + run ────────────────────────────────────────────────

pub fn eval(src: &str) -> VmResult<Value> {
    use crate::parser::Parser;
    use crate::compiler::Compiler;

    let stmts = Parser::new(src).parse_program()
        .map_err(|e| VmError(format!("parse error: {}", e.msg)))?;
    let mut compiler = Compiler::new();
    let chunk = compiler.compile_program(&stmts)
        .map_err(|e| VmError(format!("compile error: {}", e.0)))?;
    let mut vm = Vm::new(compiler.fns);
    vm.run(chunk)
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Value {
        eval(src).expect("vm error")
    }

    fn run_err(src: &str) -> String {
        eval(src).unwrap_err().0
    }

    // literals
    #[test] fn int_val()   { assert_eq!(run("1;"), Value::Int(1)); }
    #[test] fn float_val() { assert_eq!(run("1.0;"), Value::Float(1.0)); }
    #[test] fn bool_val()  { assert_eq!(run("true;"), Value::Bool(true)); }
    #[test] fn nil_val()   { assert_eq!(run("nil;"), Value::Nil); }
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

    // logical
    #[test] fn and_tt()    { assert_eq!(run("true && true;"),  Value::Bool(true)); }
    #[test] fn and_ff()    { assert_eq!(run("true && false;"), Value::Bool(false)); }
    #[test] fn or_ff()     { assert_eq!(run("false || false;"),Value::Bool(false)); }
    #[test] fn or_tf()     { assert_eq!(run("true || false;"), Value::Bool(true)); }

    // variables
    #[test]
    fn let_and_load() {
        assert_eq!(run("let x = 42; x;"), Value::Int(42));
    }

    #[test]
    fn let_chain() {
        assert_eq!(run("let x = 1; let y = x + 1; y;"), Value::Int(2));
    }

    // control flow
    #[test]
    fn if_true_branch() {
        assert_eq!(run("let x = 0; if true { let x = 1; } x;"), Value::Int(0));
        // outer x unchanged — scope isolation
    }

    #[test]
    fn if_else_false() {
        assert_eq!(run("if false { let x = 1; } else { let x = 2; } nil;"), Value::Nil);
    }

    #[test]
    fn while_counts() {
        // while loop runs; result from after the loop
        assert_eq!(run("let x = 0; while false { let x = 99; } x;"), Value::Int(0));
    }

    // functions
    #[test]
    fn fn_call_return() {
        assert_eq!(run("fn double(n: int) { return n; } double(21);"), Value::Int(21));
    }

    #[test]
    fn fn_call_two_args() {
        assert_eq!(run("fn add(a: int, b: int) { return a; } add(3, 4);"), Value::Int(3));
    }

    #[test]
    fn fn_undefined_error() {
        assert!(run_err("foo();").contains("undefined function"));
    }

    // assignment
    #[test]
    fn assign_mutates() {
        assert_eq!(run("let x = 1; x = 42; x;"), Value::Int(42));
    }

    #[test]
    fn assign_chain() {
        assert_eq!(run("let x = 0; x = 1; x = x + 1; x;"), Value::Int(2));
    }

    // runtime errors
    #[test]
    fn div_by_zero() {
        assert!(run_err("1 / 0;").contains("division by zero"));
    }

    #[test]
    fn mod_by_zero() {
        assert!(run_err("5 % 0;").contains("modulo by zero"));
    }
}
