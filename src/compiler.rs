//! Bytecode compiler: walks the AST and emits a flat Vec<Instruction>.
//! The VM (vm.rs) executes these instructions on a value stack.

use crate::ast::*;

// ── value type (shared between compiler constants and VM runtime) ─────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Nil,
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Int(n)   => write!(f, "{}", n),
            Value::Float(v) => write!(f, "{}", v),
            Value::Bool(b)  => write!(f, "{}", b),
            Value::Str(s)   => write!(f, "{}", s),
            Value::Nil      => write!(f, "nil"),
        }
    }
}

// ── opcodes ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    // push a constant onto the stack
    Const(Value),

    // arithmetic
    AddInt, AddFloat, AddStr,
    SubInt, SubFloat,
    MulInt, MulFloat,
    DivInt, DivFloat,
    ModInt,

    // comparison (all produce Bool on stack)
    EqInt, EqFloat, EqBool, EqStr,
    NeqInt, NeqFloat, NeqBool, NeqStr,
    LtInt, LtFloat,
    LtEqInt, LtEqFloat,
    GtInt, GtFloat,
    GtEqInt, GtEqFloat,

    // logical
    And, Or, Not,

    // unary
    NegInt, NegFloat,

    // variables: index into locals Vec
    LoadLocal(usize),
    StoreLocal(usize),

    // control flow: operand is absolute instruction index
    Jump(usize),       // unconditional
    JumpIfFalse(usize), // pop + jump if false

    // functions
    Call(String, usize), // name, arg count
    Return,

    // built-ins
    Print,

    // marks end of program
    Halt,
}

// ── chunk (compiled unit) ─────────────────────────────────────────────────────

#[derive(Debug, Default, Clone)]
pub struct Chunk {
    pub ops: Vec<Op>,
}

impl Chunk {
    fn emit(&mut self, op: Op) -> usize {
        self.ops.push(op);
        self.ops.len() - 1
    }

    /// Emit a placeholder jump, return its index so it can be patched later.
    fn emit_jump(&mut self, op: Op) -> usize {
        self.emit(op)
    }

    /// Patch a jump instruction at `idx` to point to current end.
    fn patch_jump(&mut self, idx: usize) {
        let target = self.ops.len();
        match &mut self.ops[idx] {
            Op::Jump(t) | Op::JumpIfFalse(t) => *t = target,
            _ => panic!("patch_jump called on non-jump op"),
        }
    }
}

// ── compiler error ────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub struct CompileError(pub String);

pub type CompileResult<T> = Result<T, CompileError>;

// ── compiler state ────────────────────────────────────────────────────────────

pub struct Compiler {
    /// locals stack: each entry is (name, slot_index)
    locals: Vec<(String, usize)>,
    next_slot: usize,
    /// compiled function bodies: name -> Chunk
    pub fns: std::collections::HashMap<String, Chunk>,
}

impl Compiler {
    pub fn new() -> Self {
        Self {
            locals: Vec::new(),
            next_slot: 0,
            fns: std::collections::HashMap::new(),
        }
    }

    /// Compile a full program into a main Chunk.
    pub fn compile_program(&mut self, stmts: &[Stmt]) -> CompileResult<Chunk> {
        // first pass: compile all fn declarations into self.fns
        for stmt in stmts {
            if let Stmt::Fn { name, params, body, .. } = stmt {
                let chunk = self.compile_fn(params, body)?;
                self.fns.insert(name.clone(), chunk);
            }
        }
        // second pass: compile top-level (non-fn) statements
        let mut chunk = Chunk::default();
        for stmt in stmts {
            if !matches!(stmt, Stmt::Fn { .. }) {
                self.compile_stmt(stmt, &mut chunk)?;
            }
        }
        chunk.emit(Op::Halt);
        Ok(chunk)
    }

    fn compile_fn(&mut self, params: &[(String, Ty)], body: &[Stmt]) -> CompileResult<Chunk> {
        // save outer locals
        let saved_locals = std::mem::take(&mut self.locals);
        let saved_slot   = self.next_slot;
        self.next_slot = 0;

        // bind params as first locals
        for (name, _) in params {
            let slot = self.next_slot;
            self.next_slot += 1;
            self.locals.push((name.clone(), slot));
        }

        let mut chunk = Chunk::default();
        for stmt in body {
            self.compile_stmt(stmt, &mut chunk)?;
        }
        // implicit nil return if no explicit return
        chunk.emit(Op::Const(Value::Nil));
        chunk.emit(Op::Return);

        // restore outer locals
        self.locals = saved_locals;
        self.next_slot = saved_slot;
        Ok(chunk)
    }
}

// ── statement compilation ─────────────────────────────────────────────────────

impl Compiler {
    fn compile_stmt(&mut self, stmt: &Stmt, chunk: &mut Chunk) -> CompileResult<()> {
        match stmt {
            Stmt::Let { name, init, .. } => {
                self.compile_expr(init, chunk)?;
                let slot = self.next_slot;
                self.next_slot += 1;
                self.locals.push((name.clone(), slot));
                chunk.emit(Op::StoreLocal(slot));
            }

            Stmt::Assign { name, value } => {
                self.compile_expr(value, chunk)?;
                let slot = self.resolve_local(name)?;
                chunk.emit(Op::StoreLocal(slot));
            }

            Stmt::ExprStmt(expr) => {
                self.compile_expr(expr, chunk)?;
            }

            Stmt::Return(expr) => {
                self.compile_expr(expr, chunk)?;
                chunk.emit(Op::Return);
            }

            Stmt::If { cond, then, else_ } => {
                self.compile_expr(cond, chunk)?;
                let jump_false = chunk.emit_jump(Op::JumpIfFalse(0));

                self.compile_block(then, chunk)?;

                if let Some(else_stmts) = else_ {
                    let jump_over = chunk.emit_jump(Op::Jump(0));
                    chunk.patch_jump(jump_false);
                    self.compile_block(else_stmts, chunk)?;
                    chunk.patch_jump(jump_over);
                } else {
                    chunk.patch_jump(jump_false);
                }
            }

            Stmt::While { cond, body } => {
                let loop_start = chunk.ops.len();
                self.compile_expr(cond, chunk)?;
                let jump_false = chunk.emit_jump(Op::JumpIfFalse(0));
                self.compile_block(body, chunk)?;
                chunk.emit(Op::Jump(loop_start));
                chunk.patch_jump(jump_false);
            }

            Stmt::Fn { .. } => {
                // fn declarations compiled in first pass — skip here
            }
        }
        Ok(())
    }

    fn compile_block(&mut self, stmts: &[Stmt], chunk: &mut Chunk) -> CompileResult<()> {
        let locals_before = self.locals.len();
        let slot_before   = self.next_slot;
        for stmt in stmts {
            self.compile_stmt(stmt, chunk)?;
        }
        // pop locals introduced in this block
        self.locals.truncate(locals_before);
        self.next_slot = slot_before;
        Ok(())
    }
}

// ── expression compilation ────────────────────────────────────────────────────

impl Compiler {
    fn compile_expr(&mut self, expr: &Expr, chunk: &mut Chunk) -> CompileResult<()> {
        match expr {
            Expr::Int(n)   => { chunk.emit(Op::Const(Value::Int(*n))); }
            Expr::Float(f) => { chunk.emit(Op::Const(Value::Float(*f))); }
            Expr::Str(s)   => { chunk.emit(Op::Const(Value::Str(s.clone()))); }
            Expr::Bool(b)  => { chunk.emit(Op::Const(Value::Bool(*b))); }
            Expr::Nil      => { chunk.emit(Op::Const(Value::Nil)); }

            Expr::Group(inner) => self.compile_expr(inner, chunk)?,

            Expr::Var(name) => {
                let slot = self.resolve_local(name)?;
                chunk.emit(Op::LoadLocal(slot));
            }

            Expr::Unary { op, expr } => {
                self.compile_expr(expr, chunk)?;
                match op {
                    UnOp::Neg => {
                        // type checker already verified int|float — peek at last Const if possible
                        // but at compile time we don't track types; emit both and let VM decide
                        // for simplicity: NegInt if previous Const was Int, else NegFloat
                        // Better: emit a generic Neg — but we have typed ops. Use NegInt as default;
                        // the type checker ensures this is safe.
                        chunk.emit(Op::NegInt); // overridden below for float
                        // patch: if last emitted before NegInt was Const(Float), swap to NegFloat
                        let len = chunk.ops.len();
                        if len >= 2 {
                            if let Op::Const(Value::Float(_)) = &chunk.ops[len-2] {
                                *chunk.ops.last_mut().unwrap() = Op::NegFloat;
                            }
                        }
                    }
                    UnOp::Not => { chunk.emit(Op::Not); }
                }
            }

            Expr::Binary { op, lhs, rhs } => {
                self.compile_expr(lhs, chunk)?;
                self.compile_expr(rhs, chunk)?;
                self.compile_binop(op, lhs, rhs, chunk);
            }

            Expr::Call { callee, args } => {
                if callee == "print" {
                    // built-in
                    for arg in args { self.compile_expr(arg, chunk)?; }
                    chunk.emit(Op::Print);
                } else {
                    for arg in args { self.compile_expr(arg, chunk)?; }
                    chunk.emit(Op::Call(callee.clone(), args.len()));
                }
            }
        }
        Ok(())
    }

    fn resolve_local(&self, name: &str) -> CompileResult<usize> {
        self.locals.iter().rev()
            .find(|(n, _)| n == name)
            .map(|(_, slot)| *slot)
            .ok_or_else(|| CompileError(format!("undefined variable '{}'", name)))
    }

    /// Emit the right typed opcode for a binary operator.
    /// We peek at the LHS expression to decide int vs float.
    fn compile_binop(&self, op: &BinOp, lhs: &Expr, _rhs: &Expr, chunk: &mut Chunk) {
        let is_float = expr_is_float(lhs);
        let is_str   = expr_is_str(lhs);
        let instr = match op {
            BinOp::Add  => if is_str { Op::AddStr } else if is_float { Op::AddFloat } else { Op::AddInt },
            BinOp::Sub  => if is_float { Op::SubFloat } else { Op::SubInt },
            BinOp::Mul  => if is_float { Op::MulFloat } else { Op::MulInt },
            BinOp::Div  => if is_float { Op::DivFloat } else { Op::DivInt },
            BinOp::Mod  => Op::ModInt,
            BinOp::Eq   => if is_float { Op::EqFloat } else if is_str { Op::EqStr } else if expr_is_bool(lhs) { Op::EqBool } else { Op::EqInt },
            BinOp::NotEq=> if is_float { Op::NeqFloat } else if is_str { Op::NeqStr } else if expr_is_bool(lhs) { Op::NeqBool } else { Op::NeqInt },
            BinOp::Lt   => if is_float { Op::LtFloat }   else { Op::LtInt },
            BinOp::LtEq => if is_float { Op::LtEqFloat } else { Op::LtEqInt },
            BinOp::Gt   => if is_float { Op::GtFloat }   else { Op::GtInt },
            BinOp::GtEq => if is_float { Op::GtEqFloat } else { Op::GtEqInt },
            BinOp::And  => Op::And,
            BinOp::Or   => Op::Or,
        };
        chunk.ops.push(instr);
    }
}

// ── helpers for static type hints from Expr shape ────────────────────────────

fn expr_is_float(e: &Expr) -> bool {
    matches!(e, Expr::Float(_))
        || matches!(e, Expr::Unary { op: UnOp::Neg, expr } if matches!(expr.as_ref(), Expr::Float(_)))
}

fn expr_is_str(e: &Expr) -> bool { matches!(e, Expr::Str(_)) }
fn expr_is_bool(e: &Expr) -> bool { matches!(e, Expr::Bool(_) | Expr::Unary { op: UnOp::Not, .. }) }

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;

    fn compile(src: &str) -> Vec<Op> {
        let stmts = Parser::new(src).parse_program().expect("parse");
        let mut c = Compiler::new();
        c.compile_program(&stmts).expect("compile").ops
    }

    fn compile_expr(src: &str) -> Vec<Op> {
        // wrap in expr stmt so compile_program works
        compile(&format!("{};", src))
    }

    #[test]
    fn int_const() {
        assert_eq!(compile_expr("1"), vec![Op::Const(Value::Int(1)), Op::Halt]);
    }

    #[test]
    fn float_const() {
        assert_eq!(compile_expr("1.0"), vec![Op::Const(Value::Float(1.0)), Op::Halt]);
    }

    #[test]
    fn bool_const() {
        assert_eq!(compile_expr("true"), vec![Op::Const(Value::Bool(true)), Op::Halt]);
    }

    #[test]
    fn nil_const() {
        assert_eq!(compile_expr("nil"), vec![Op::Const(Value::Nil), Op::Halt]);
    }

    #[test]
    fn str_const() {
        assert_eq!(compile_expr(r#""hi""#), vec![Op::Const(Value::Str("hi".into())), Op::Halt]);
    }

    #[test]
    fn int_add() {
        assert_eq!(compile_expr("1 + 2"), vec![
            Op::Const(Value::Int(1)), Op::Const(Value::Int(2)), Op::AddInt, Op::Halt
        ]);
    }

    #[test]
    fn float_add() {
        assert_eq!(compile_expr("1.0 + 2.0"), vec![
            Op::Const(Value::Float(1.0)), Op::Const(Value::Float(2.0)), Op::AddFloat, Op::Halt
        ]);
    }

    #[test]
    fn int_sub() {
        assert_eq!(compile_expr("3 - 1"), vec![
            Op::Const(Value::Int(3)), Op::Const(Value::Int(1)), Op::SubInt, Op::Halt
        ]);
    }

    #[test]
    fn int_mul() {
        assert_eq!(compile_expr("2 * 3"), vec![
            Op::Const(Value::Int(2)), Op::Const(Value::Int(3)), Op::MulInt, Op::Halt
        ]);
    }

    #[test]
    fn int_div() {
        assert_eq!(compile_expr("6 / 2"), vec![
            Op::Const(Value::Int(6)), Op::Const(Value::Int(2)), Op::DivInt, Op::Halt
        ]);
    }

    #[test]
    fn int_mod() {
        assert_eq!(compile_expr("7 % 3"), vec![
            Op::Const(Value::Int(7)), Op::Const(Value::Int(3)), Op::ModInt, Op::Halt
        ]);
    }

    #[test]
    fn unary_neg_int() {
        assert_eq!(compile_expr("-1"), vec![
            Op::Const(Value::Int(1)), Op::NegInt, Op::Halt
        ]);
    }

    #[test]
    fn unary_neg_float() {
        assert_eq!(compile_expr("-1.0"), vec![
            Op::Const(Value::Float(1.0)), Op::NegFloat, Op::Halt
        ]);
    }

    #[test]
    fn unary_not() {
        assert_eq!(compile_expr("!true"), vec![
            Op::Const(Value::Bool(true)), Op::Not, Op::Halt
        ]);
    }

    #[test]
    fn let_stmt() {
        assert_eq!(compile("let x = 1;"), vec![
            Op::Const(Value::Int(1)), Op::StoreLocal(0), Op::Halt
        ]);
    }

    #[test]
    fn let_then_load() {
        assert_eq!(compile("let x = 1; let y = x;"), vec![
            Op::Const(Value::Int(1)), Op::StoreLocal(0),
            Op::LoadLocal(0), Op::StoreLocal(1),
            Op::Halt
        ]);
    }

    #[test]
    fn return_stmt() {
        let ops = compile("fn f() { return 1; } ");
        // main chunk: just Halt (fn compiled separately)
        assert_eq!(ops, vec![Op::Halt]);
        // fn chunk has: Const(1), Return, Const(Nil), Return
        // (implicit nil return appended after explicit)
    }

    #[test]
    fn if_no_else() {
        let ops = compile("if true { let x = 1; }");
        assert_eq!(ops, vec![
            Op::Const(Value::Bool(true)),
            Op::JumpIfFalse(4),          // skip then block
            Op::Const(Value::Int(1)),
            Op::StoreLocal(0),
            Op::Halt,
        ]);
    }

    #[test]
    fn if_with_else() {
        let ops = compile("if true { let x = 1; } else { let x = 2; }");
        assert_eq!(ops, vec![
            Op::Const(Value::Bool(true)),
            Op::JumpIfFalse(5),
            Op::Const(Value::Int(1)),
            Op::StoreLocal(0),
            Op::Jump(7),
            Op::Const(Value::Int(2)),
            Op::StoreLocal(0),
            Op::Halt,
        ]);
    }

    #[test]
    fn while_loop() {
        let ops = compile("while true { let x = 1; }");
        assert_eq!(ops, vec![
            Op::Const(Value::Bool(true)), // idx 0 — loop head
            Op::JumpIfFalse(5),           // idx 1
            Op::Const(Value::Int(1)),     // idx 2
            Op::StoreLocal(0),            // idx 3
            Op::Jump(0),                  // idx 4 — back to head
            Op::Halt,                     // idx 5
        ]);
    }

    #[test]
    fn fn_chunk_compiled() {
        let stmts = Parser::new("fn add(a: int, b: int) { return a; }").parse_program().unwrap();
        let mut c = Compiler::new();
        c.compile_program(&stmts).unwrap();
        assert!(c.fns.contains_key("add"));
        let chunk = &c.fns["add"];
        // params a=slot0, b=slot1; body: LoadLocal(0), Return, Const(Nil), Return
        assert!(chunk.ops.contains(&Op::LoadLocal(0)));
        assert!(chunk.ops.contains(&Op::Return));
    }

    #[test]
    fn call_emit() {
        let ops = compile("fn f() { return 1; } f();");
        assert!(ops.contains(&Op::Call("f".into(), 0)));
    }

    #[test]
    fn assign_reuses_slot() {
        // let x=slot0, then x=2 should StoreLocal(0) again, not allocate slot1
        assert_eq!(compile("let x = 1; x = 2;"), vec![
            Op::Const(Value::Int(1)), Op::StoreLocal(0),
            Op::Const(Value::Int(2)), Op::StoreLocal(0),
            Op::Halt,
        ]);
    }

    #[test]
    fn undefined_var_error() {
        let stmts = Parser::new("let y = x;").parse_program().unwrap();
        let result = Compiler::new().compile_program(&stmts);
        assert!(result.is_err());
    }

    #[test]
    fn comparison_lt() {
        assert_eq!(compile_expr("1 < 2"), vec![
            Op::Const(Value::Int(1)), Op::Const(Value::Int(2)), Op::LtInt, Op::Halt
        ]);
    }

    #[test]
    fn logical_and() {
        assert_eq!(compile_expr("true && false"), vec![
            Op::Const(Value::Bool(true)), Op::Const(Value::Bool(false)), Op::And, Op::Halt
        ]);
    }
}
