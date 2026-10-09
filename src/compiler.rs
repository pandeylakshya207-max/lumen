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

    // pop the value of an expression statement; the VM keeps the most recent
    // one as the result of the program
    Pop,

    // arithmetic
    AddInt, AddFloat, AddStr,
    SubInt, SubFloat,
    MulInt, MulFloat,
    DivInt, DivFloat,
    ModInt,

    // comparison (all produce Bool on stack)
    EqInt, EqFloat, EqBool, EqStr, EqNil,
    NeqInt, NeqFloat, NeqBool, NeqStr, NeqNil,
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
    /// static type of each entry in `locals`, when it is known
    local_tys: Vec<Option<Ty>>,
    next_slot: usize,
    /// compiled function bodies: name -> Chunk
    pub fns: std::collections::HashMap<String, Chunk>,
    /// declared return type of each function that has one
    fn_rets: std::collections::HashMap<String, Ty>,
}

impl Compiler {
    pub fn new() -> Self {
        Self {
            locals: Vec::new(),
            local_tys: Vec::new(),
            next_slot: 0,
            fns: std::collections::HashMap::new(),
            fn_rets: std::collections::HashMap::new(),
        }
    }

    /// Compile a full program into a main Chunk.
    pub fn compile_program(&mut self, stmts: &[Stmt]) -> CompileResult<Chunk> {
        // record declared return types first, so a call compiles the same way
        // whether the function is declared before or after it
        for stmt in stmts {
            if let Stmt::Fn { name, ret: Some(ty), .. } = stmt {
                self.fn_rets.insert(name.clone(), ty.clone());
            }
        }
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
        let saved_tys    = std::mem::take(&mut self.local_tys);
        let saved_slot   = self.next_slot;
        self.next_slot = 0;

        // bind params as first locals
        for (name, ty) in params {
            let slot = self.next_slot;
            self.next_slot += 1;
            self.locals.push((name.clone(), slot));
            self.local_tys.push(known(ty));
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
        self.local_tys = saved_tys;
        self.next_slot = saved_slot;
        Ok(chunk)
    }
}

// ── statement compilation ─────────────────────────────────────────────────────

impl Compiler {
    fn compile_stmt(&mut self, stmt: &Stmt, chunk: &mut Chunk) -> CompileResult<()> {
        match stmt {
            Stmt::Let { name, ty, init } => {
                self.compile_expr(init, chunk)?;
                // work out the type before the name is bound: `let x = x;` reads the outer x
                let static_ty = ty.as_ref().and_then(known).or_else(|| self.type_of(init));
                let slot = self.next_slot;
                self.next_slot += 1;
                self.locals.push((name.clone(), slot));
                self.local_tys.push(static_ty);
                chunk.emit(Op::StoreLocal(slot));
            }

            Stmt::Assign { name, value } => {
                self.compile_expr(value, chunk)?;
                let slot = self.resolve_local(name)?;
                chunk.emit(Op::StoreLocal(slot));
            }

            Stmt::ExprStmt(expr) => {
                self.compile_expr(expr, chunk)?;
                // every expression leaves exactly one value; take it off again
                // so statements do not pile values up on the operand stack
                chunk.emit(Op::Pop);
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
        self.local_tys.truncate(locals_before);
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
                        let is_float = self.type_of(expr) == Some(Ty::Float);
                        chunk.emit(if is_float { Op::NegFloat } else { Op::NegInt });
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
                    // built-in: prints its first argument (nil when there is none)
                    match args.first() {
                        Some(arg) => self.compile_expr(arg, chunk)?,
                        None => { chunk.emit(Op::Const(Value::Nil)); }
                    }
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
    /// The operand type comes from the static type of the left side, or of the
    /// right side when the left is not known. With neither known the int opcode is used.
    fn compile_binop(&self, op: &BinOp, lhs: &Expr, rhs: &Expr, chunk: &mut Chunk) {
        let ty = self.type_of(lhs).or_else(|| self.type_of(rhs));
        let is_float = ty == Some(Ty::Float);
        let is_str   = ty == Some(Ty::Str);
        let is_bool  = ty == Some(Ty::Bool);
        let is_nil   = ty == Some(Ty::Nil);
        let instr = match op {
            BinOp::Add  => if is_str { Op::AddStr } else if is_float { Op::AddFloat } else { Op::AddInt },
            BinOp::Sub  => if is_float { Op::SubFloat } else { Op::SubInt },
            BinOp::Mul  => if is_float { Op::MulFloat } else { Op::MulInt },
            BinOp::Div  => if is_float { Op::DivFloat } else { Op::DivInt },
            BinOp::Mod  => Op::ModInt,
            BinOp::Eq   => if is_float { Op::EqFloat } else if is_str { Op::EqStr } else if is_bool { Op::EqBool } else if is_nil { Op::EqNil } else { Op::EqInt },
            BinOp::NotEq=> if is_float { Op::NeqFloat } else if is_str { Op::NeqStr } else if is_bool { Op::NeqBool } else if is_nil { Op::NeqNil } else { Op::NeqInt },
            BinOp::Lt   => if is_float { Op::LtFloat }   else { Op::LtInt },
            BinOp::LtEq => if is_float { Op::LtEqFloat } else { Op::LtEqInt },
            BinOp::Gt   => if is_float { Op::GtFloat }   else { Op::GtInt },
            BinOp::GtEq => if is_float { Op::GtEqFloat } else { Op::GtEqInt },
            BinOp::And  => Op::And,
            BinOp::Or   => Op::Or,
        };
        chunk.ops.push(instr);
    }

    /// Static type of an expression, or None when it cannot be told from the
    /// declarations in scope (a user-defined type, or a call to a function
    /// with no declared return type).
    fn type_of(&self, expr: &Expr) -> Option<Ty> {
        match expr {
            Expr::Int(_)   => Some(Ty::Int),
            Expr::Float(_) => Some(Ty::Float),
            Expr::Str(_)   => Some(Ty::Str),
            Expr::Bool(_)  => Some(Ty::Bool),
            Expr::Nil      => Some(Ty::Nil),
            Expr::Group(inner) => self.type_of(inner),
            Expr::Var(name) => self.locals.iter().rposition(|(n, _)| n == name)
                .and_then(|i| self.local_tys[i].clone()),
            Expr::Unary { op: UnOp::Not, .. } => Some(Ty::Bool),
            Expr::Unary { op: UnOp::Neg, expr } => self.type_of(expr),
            Expr::Binary { op, lhs, rhs } => match op {
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod =>
                    self.type_of(lhs).or_else(|| self.type_of(rhs)),
                _ => Some(Ty::Bool),
            },
            Expr::Call { callee, .. } => {
                if callee == "print" { Some(Ty::Nil) } else { self.fn_rets.get(callee).and_then(known) }
            }
        }
    }
}

/// A declared type the compiler can pick opcodes from; a user-defined name is not one.
fn known(ty: &Ty) -> Option<Ty> {
    if matches!(ty, Ty::Named(_)) { None } else { Some(ty.clone()) }
}

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
        // wrap in expr stmt so compile_program works, then drop the Pop that
        // ends the statement so the tests below show only the expression
        let mut ops = compile(&format!("{};", src));
        assert_eq!(ops.remove(ops.len() - 2), Op::Pop);
        ops
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
    fn expr_stmt_is_popped() {
        assert_eq!(compile("1; 2;"), vec![
            Op::Const(Value::Int(1)), Op::Pop, Op::Const(Value::Int(2)), Op::Pop, Op::Halt,
        ]);
    }

    #[test]
    fn opcode_follows_variable_type() {
        assert_eq!(compile("let x = 1.5; let y = x + x;"), vec![
            Op::Const(Value::Float(1.5)), Op::StoreLocal(0),
            Op::LoadLocal(0), Op::LoadLocal(0), Op::AddFloat, Op::StoreLocal(1),
            Op::Halt,
        ]);
    }

    #[test]
    fn opcode_follows_nested_expression_type() {
        assert_eq!(compile_expr("1.0 + 2.0 + 3.0"), vec![
            Op::Const(Value::Float(1.0)), Op::Const(Value::Float(2.0)), Op::AddFloat,
            Op::Const(Value::Float(3.0)), Op::AddFloat, Op::Halt,
        ]);
        assert_eq!(compile_expr(r#"("a" + "b") == "ab""#), vec![
            Op::Const(Value::Str("a".into())), Op::Const(Value::Str("b".into())), Op::AddStr,
            Op::Const(Value::Str("ab".into())), Op::EqStr, Op::Halt,
        ]);
    }

    #[test]
    fn opcode_follows_param_and_return_types() {
        let stmts = Parser::new("fn half(x: float) -> float { return x / 2.0; } let y = half(1.0) * 3.0;")
            .parse_program().unwrap();
        let mut c = Compiler::new();
        let main = c.compile_program(&stmts).unwrap();
        assert!(c.fns["half"].ops.contains(&Op::DivFloat));
        assert!(main.ops.contains(&Op::MulFloat));
    }

    #[test]
    fn neg_follows_variable_type() {
        let ops = compile("let x = 1.5; let y = -x; let n = 2; let m = -n;");
        assert_eq!(ops.iter().filter(|op| **op == Op::NegFloat).count(), 1);
        assert_eq!(ops.iter().filter(|op| **op == Op::NegInt).count(), 1);
    }

    #[test]
    fn unknown_type_falls_back_to_int_opcode() {
        // no declared return type: the right operand decides, else int
        let ops = compile("fn f() { return 1; } let a = f() + 2; let b = f() + 2.0;");
        assert!(ops.contains(&Op::AddInt));
        assert!(ops.contains(&Op::AddFloat));
    }

    #[test]
    fn print_without_argument_prints_nil() {
        assert_eq!(compile("print();"), vec![Op::Const(Value::Nil), Op::Print, Op::Pop, Op::Halt]);
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
