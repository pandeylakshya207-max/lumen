use crate::ast::*;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64), Float(f64), Bool(bool), Str(String), Nil,
}
impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Int(n) => write!(f,"{}", n), Value::Float(v) => write!(f,"{}", v),
            Value::Bool(b) => write!(f,"{}", b), Value::Str(s) => write!(f,"{}", s),
            Value::Nil => write!(f,"nil"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Const(Value),
    AddInt, AddFloat, AddStr, SubInt, SubFloat, MulInt, MulFloat,
    DivInt, DivFloat, ModInt,
    EqInt, EqFloat, EqBool, EqStr, NeqInt, NeqFloat, NeqBool, NeqStr,
    LtInt, LtFloat, LtEqInt, LtEqFloat, GtInt, GtFloat, GtEqInt, GtEqFloat,
    And, Or, Not, NegInt, NegFloat,
    LoadLocal(usize), StoreLocal(usize),
    Jump(usize), JumpIfFalse(usize),
    Call(String, usize), Return, Print, Halt,
}

#[derive(Debug, Default, Clone)]
pub struct Chunk { pub ops: Vec<Op> }
impl Chunk {
    fn emit(&mut self, op: Op) -> usize { self.ops.push(op); self.ops.len() - 1 }
    fn emit_jump(&mut self, op: Op) -> usize { self.emit(op) }
    fn patch_jump(&mut self, idx: usize) {
        let t = self.ops.len();
        match &mut self.ops[idx] { Op::Jump(x) | Op::JumpIfFalse(x) => *x = t, _ => panic!() }
    }
}

#[derive(Debug, PartialEq)]
pub struct CompileError(pub String);
pub type CompileResult<T> = Result<T, CompileError>;
