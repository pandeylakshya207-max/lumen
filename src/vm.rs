use std::collections::HashMap;
use crate::compiler::{Chunk, Op, Value};

#[derive(Debug, PartialEq)]
pub struct VmError(pub String);
pub type VmResult<T> = Result<T, VmError>;

struct Frame { chunk: Chunk, ip: usize, locals_base: usize }

pub struct Vm {
    stack: Vec<Value>,
    locals: Vec<Value>,
    fns: HashMap<String, Chunk>,
    frames: Vec<Frame>,
}
impl Vm {
    pub fn new(fns: HashMap<String, Chunk>) -> Self {
        Self { stack: Vec::new(), locals: Vec::new(), fns, frames: Vec::new() }
    }
    fn pop(&mut self) -> VmResult<Value> { self.stack.pop().ok_or_else(|| VmError("stack underflow".into())) }
    fn pop_int(&mut self) -> VmResult<i64> { match self.pop()? { Value::Int(n) => Ok(n), v => Err(VmError(format!("expected int, got {:?}", v))) } }
    fn pop_float(&mut self) -> VmResult<f64> { match self.pop()? { Value::Float(f) => Ok(f), v => Err(VmError(format!("expected float, got {:?}", v))) } }
    fn pop_bool(&mut self) -> VmResult<bool> { match self.pop()? { Value::Bool(b) => Ok(b), v => Err(VmError(format!("expected bool, got {:?}", v))) } }
    fn pop_str(&mut self) -> VmResult<String> { match self.pop()? { Value::Str(s) => Ok(s), v => Err(VmError(format!("expected str, got {:?}", v))) } }
    fn pop2_int(&mut self) -> VmResult<(i64,i64)> { let b=self.pop_int()?; let a=self.pop_int()?; Ok((a,b)) }
    fn pop2_float(&mut self) -> VmResult<(f64,f64)> { let b=self.pop_float()?; let a=self.pop_float()?; Ok((a,b)) }
    fn pop2_bool(&mut self) -> VmResult<(bool,bool)> { let b=self.pop_bool()?; let a=self.pop_bool()?; Ok((a,b)) }
}
