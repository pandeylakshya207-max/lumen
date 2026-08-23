use std::collections::HashMap;
use crate::ast::*;
use crate::compiler::Value;

#[derive(Debug, PartialEq)]
pub struct InterpError(pub String);
pub type InterpResult<T> = Result<T, InterpError>;

enum Signal { None, Return(Value) }

#[derive(Clone, Default)]
struct Env { scopes: Vec<HashMap<String, Value>> }
impl Env {
    fn push(&mut self) { self.scopes.push(HashMap::new()); }
    fn pop(&mut self)  { self.scopes.pop(); }
    fn define(&mut self, name: &str, val: Value) {
        self.scopes.last_mut().expect("no scope").insert(name.to_string(), val);
    }
    fn get(&self, name: &str) -> InterpResult<Value> {
        for s in self.scopes.iter().rev() { if let Some(v) = s.get(name) { return Ok(v.clone()); } }
        Err(InterpError(format!("undefined variable '{}'", name)))
    }
    fn set(&mut self, name: &str, val: Value) -> InterpResult<()> {
        for s in self.scopes.iter_mut().rev() {
            if s.contains_key(name) { s.insert(name.to_string(), val); return Ok(()); }
        }
        Err(InterpError(format!("assignment to undefined variable '{}'", name)))
    }
}

#[derive(Clone)]
struct FnDef { params: Vec<(String, Ty)>, body: Vec<Stmt> }

pub struct Interpreter { env: Env, fns: HashMap<String, FnDef>, last_val: Value }
impl Interpreter {
    pub fn new() -> Self {
        let mut env = Env::default(); env.push();
        Self { env, fns: HashMap::new(), last_val: Value::Nil }
    }
}
