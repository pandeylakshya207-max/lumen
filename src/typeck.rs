use std::collections::HashMap;
use crate::ast::*;

#[derive(Debug, PartialEq)]
pub struct TypeError { pub msg: String }
impl TypeError { fn new(msg: impl Into<String>) -> Self { Self { msg: msg.into() } } }
pub type TyResult<T> = Result<T, TypeError>;

#[derive(Debug, Default)]
struct Env { scopes: Vec<HashMap<String, Ty>> }
impl Env {
    fn push(&mut self) { self.scopes.push(HashMap::new()); }
    fn pop(&mut self)  { self.scopes.pop(); }
    fn define(&mut self, name: &str, ty: Ty) {
        if let Some(s) = self.scopes.last_mut() { s.insert(name.to_string(), ty); }
    }
    fn lookup(&self, name: &str) -> Option<&Ty> {
        for s in self.scopes.iter().rev() { if let Some(t) = s.get(name) { return Some(t); } }
        None
    }
}

#[derive(Debug, Clone)]
pub struct FnSig { pub params: Vec<Ty>, pub ret: Ty }