//! The bytecode VM and the tree-walking interpreter must give the same answer
//! for every well-typed program.
//!
//! Two checks:
//!   1. hand-written programs with known results;
//!   2. differential testing: a few thousand generated well-typed programs,
//!      each run on both backends and compared.
//!
//! The generator is seeded, so a failure is reproducible: the panic message
//! prints the seed and the program.

use lumen::interpreter::interp;
use lumen::parser::Parser;
use lumen::typeck::TypeChecker;
use lumen::vm::eval;

/// Runs `src` on both backends; the result is the printed value or the error message.
fn both(src: &str) -> (String, String) {
    let stmts = Parser::new(src).parse_program().unwrap_or_else(|e| panic!("parse error: {}\n{}", e, src));
    if let Err(e) = TypeChecker::new().check_program(&stmts) {
        panic!("type error: {}\n{}", e.msg, src);
    }
    let vm = match eval(src) { Ok(v) => format!("{:?}", v), Err(e) => format!("error: {}", e.0) };
    let it = match interp(src) { Ok(v) => format!("{:?}", v), Err(e) => format!("error: {}", e.0) };
    (vm, it)
}

// ---- 1. hand-written programs ------------------------------------------------

const PROGRAMS: &[(&str, &str, &str)] = &[
    ("precedence", "1 + 2 * 3 - 10 / 5;", "Int(5)"),
    ("grouping", "(1 + 2) * 3;", "Int(9)"),
    ("float chain", "1.0 + 2.0 + 3.0;", "Float(6.0)"),
    ("float variable", "let x = 1.5; x * 2.0 + x;", "Float(4.5)"),
    ("string concat", r#"let a = "foo"; let b = a + "bar"; b + "!";"#, r#"Str("foobar!")"#),
    ("string equality", r#"let a = "x"; a + "y" == "xy";"#, "Bool(true)"),
    ("bool logic", "let t = true; let f = !t; t && !f || f;", "Bool(true)"),
    ("nil equality", "nil == nil;", "Bool(true)"),
    ("sum 1..100", "let s = 0; let i = 1; while i <= 100 { s = s + i; i = i + 1; } s;", "Int(5050)"),
    ("iterative fib", "let a = 0; let b = 1; let i = 0; while i < 10 { let t = b; b = a + b; a = t; i = i + 1; } a;", "Int(55)"),
    ("recursive fib", "fn fib(n: int) -> int { if n < 2 { return n; } return fib(n - 1) + fib(n - 2); } fib(20);", "Int(6765)"),
    ("factorial", "fn fact(n: int) -> int { if n <= 1 { return 1; } return n * fact(n - 1); } fact(20);", "Int(2432902008176640000)"),
    ("factorial overflow", "fn fact(n: int) -> int { if n <= 1 { return 1; } return n * fact(n - 1); } fact(21);", "error: integer overflow"),
    ("gcd", "fn gcd(a: int, b: int) -> int { while b != 0 { let t = b; b = a % b; a = t; } return a; } gcd(48, 18);", "Int(6)"),
    ("else if", "let x = 5; let r = 0; if x < 3 { r = 1; } else if x < 10 { r = 2; } else { r = 3; } r;", "Int(2)"),
    ("early return in loop", "fn first_over(n: int) -> int { let i = 0; while true { if i * i > n { return i; } i = i + 1; } return 0; } first_over(50);", "Int(8)"),
    ("float function", "fn area(r: float) -> float { let pi = 3.0; return pi * r * r; } area(2.0) > 11.5;", "Bool(true)"),
    ("mutual recursion", "fn even(n: int) -> bool { if n == 0 { return true; } return odd(n - 1); } fn odd(n: int) -> bool { if n == 0 { return false; } return even(n - 1); } even(10);", "Bool(true)"),
    ("shadowing", "let x = 1; if true { let x = 2.5; x + 0.5; }", "Float(3.0)"),
    ("statements inside a function", "fn f(x: float) -> float { 5 / 4; return x + 1.0; } f(1.0) + f(2.0);", "Float(5.0)"),
    ("no expression statement", "let x = 1;", "Nil"),
    ("division by zero", "let z = 0; 1 / z;", "error: division by zero"),
    ("modulo by zero", "let z = 0; 1 % z;", "error: modulo by zero"),
    ("nan compares false", "let nan = 0.0 / 0.0; nan < 1.0 || nan >= 1.0 || nan == nan;", "Bool(false)"),
];

#[test]
fn hand_written_programs_agree() {
    for (name, src, expected) in PROGRAMS {
        let (vm, it) = both(src);
        assert_eq!(&vm, expected, "VM, program '{}'", name);
        assert_eq!(&it, expected, "interpreter, program '{}'", name);
    }
}

/// The program shown in the README runs, and gives the same result on both backends.
#[test]
fn example_program_agrees() {
    let (vm, it) = both(include_str!("../examples/demo.lm"));
    assert_eq!(vm, "Int(80)");
    assert_eq!(it, vm);
}

/// Both backends allow the same number of nested calls and fail the same way
/// beyond it. The interpreter needs several native frames per call, so this
/// runs on a thread with a large stack, as the command-line tool does.
#[test]
fn recursion_limit_agrees() {
    let check = || {
        let at_limit = format!(
            "fn down(n: int) -> int {{ if n == 0 {{ return 0; }} return 1 + down(n - 1); }} down({});",
            lumen::MAX_CALL_DEPTH - 1
        );
        let (vm, it) = both(&at_limit);
        assert_eq!(vm, format!("Int({})", lumen::MAX_CALL_DEPTH - 1));
        assert_eq!(it, vm);

        let (vm, it) = both("fn f(n: int) -> int { return f(n + 1); } f(0);");
        assert_eq!(vm, format!("error: stack overflow: more than {} nested calls", lumen::MAX_CALL_DEPTH));
        assert_eq!(it, vm);
    };
    std::thread::Builder::new().stack_size(64 * 1024 * 1024).spawn(check).unwrap().join().unwrap();
}

// ---- 2. generated programs ---------------------------------------------------

/// xorshift64*: small, seeded, no dependencies.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    /// Uniform in 0..n.
    fn below(&mut self, n: usize) -> usize { (self.next() >> 33) as usize % n }
    /// True with probability pct/100.
    fn chance(&mut self, pct: usize) -> bool { self.below(100) < pct }
    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str { items[self.below(items.len())] }
}

#[derive(Clone, Copy, PartialEq)]
enum T { Int, Float, Bool, Str }

const TYPES: [T; 4] = [T::Int, T::Float, T::Bool, T::Str];

fn name_of(t: T) -> &'static str {
    match t { T::Int => "int", T::Float => "float", T::Bool => "bool", T::Str => "str" }
}

struct Var { name: String, ty: T, assignable: bool }

/// Generates one well-typed program. Every loop is bounded and no function
/// calls itself, so every program terminates.
struct Gen {
    rng: Rng,
    /// variables in scope, innermost last; `marks` holds the scope boundaries
    vars: Vec<Var>,
    marks: Vec<usize>,
    /// functions declared so far: (name, parameter types, return type)
    fns: Vec<(String, Vec<T>, T)>,
    counter: usize,
    in_loop: usize,
}

impl Gen {
    fn new(seed: u64) -> Self {
        Self { rng: Rng(seed | 1), vars: Vec::new(), marks: Vec::new(), fns: Vec::new(), counter: 0, in_loop: 0 }
    }

    fn fresh(&mut self, prefix: &str) -> String { self.counter += 1; format!("{}{}", prefix, self.counter) }

    fn ty(&mut self) -> T { TYPES[self.rng.below(4)] }

    /// A visible variable of type `t`. A name is visible only through its innermost declaration.
    fn var(&mut self, t: T, assignable: bool) -> Option<String> {
        let mut names: Vec<&str> = Vec::new();
        for (i, v) in self.vars.iter().enumerate() {
            let shadowed = self.vars[i + 1..].iter().any(|w| w.name == v.name);
            if !shadowed && v.ty == t && (v.assignable || !assignable) { names.push(&v.name); }
        }
        if names.is_empty() { None } else { Some(names[self.rng.below(names.len())].to_string()) }
    }

    fn literal(&mut self, t: T) -> String {
        match t {
            T::Int   => self.rng.below(10).to_string(),
            T::Float => self.rng.pick(&["0.0", "0.5", "1.0", "1.5", "2.0", "2.25", "3.0", "10.0"]).to_string(),
            T::Bool  => self.rng.pick(&["true", "false"]).to_string(),
            T::Str   => format!("\"{}\"", self.rng.pick(&["", "a", "b", "ab", "lumen", "x y"])),
        }
    }

    fn expr(&mut self, t: T, depth: usize) -> String {
        if depth > 3 || self.rng.chance(25) {
            if self.rng.chance(70) {
                if let Some(v) = self.var(t, false) { return v; }
            }
            return self.literal(t);
        }
        let k = self.rng.below(100);
        if k < 15 {
            let candidates: Vec<usize> = (0..self.fns.len()).filter(|&i| self.fns[i].2 == t).collect();
            if !candidates.is_empty() {
                let (name, params, _) = self.fns[candidates[self.rng.below(candidates.len())]].clone();
                let args: Vec<String> = params.iter().map(|&p| self.expr(p, depth + 1)).collect();
                return format!("{}({})", name, args.join(", "));
            }
        }
        if k < 25 { return format!("({})", self.expr(t, depth + 1)); }
        match t {
            T::Int => {
                if k < 35 { return format!("-{}", self.expr(T::Int, depth + 2)); }
                let op = self.rng.pick(&["+", "-", "*", "/", "%", "+", "-"]);
                let lhs = self.expr(T::Int, depth + 1);
                // divide only by a non-zero literal so most programs produce a value
                let rhs = if op == "/" || op == "%" { (1 + self.rng.below(9)).to_string() } else { self.expr(T::Int, depth + 1) };
                format!("{} {} {}", lhs, op, rhs)
            }
            T::Float => {
                if k < 35 { return format!("-{}", self.expr(T::Float, depth + 2)); }
                let op = self.rng.pick(&["+", "-", "*", "/"]);
                let lhs = self.expr(T::Float, depth + 1);
                let rhs = if op == "/" { self.rng.pick(&["0.5", "2.0", "4.0"]).to_string() } else { self.expr(T::Float, depth + 1) };
                format!("{} {} {}", lhs, op, rhs)
            }
            T::Str => format!("{} + {}", self.expr(T::Str, depth + 1), self.expr(T::Str, depth + 1)),
            T::Bool => {
                if k < 35 { return format!("!({})", self.expr(T::Bool, depth + 2)); }
                if k < 55 {
                    let num = if self.rng.chance(50) { T::Int } else { T::Float };
                    let op = self.rng.pick(&["<", "<=", ">", ">=", "==", "!="]);
                    return format!("{} {} {}", self.expr(num, depth + 1), op, self.expr(num, depth + 1));
                }
                if k < 75 {
                    let operand = self.ty();
                    let op = self.rng.pick(&["==", "!="]);
                    let (lhs, rhs) = (self.expr(operand, depth + 1), self.expr(operand, depth + 1));
                    // `a == b == c` would compare a bool with c, so bool operands are bracketed
                    return if operand == T::Bool { format!("({}) {} ({})", lhs, op, rhs) } else { format!("{} {} {}", lhs, op, rhs) };
                }
                let op = self.rng.pick(&["&&", "||"]);
                format!("{} {} {}", self.expr(T::Bool, depth + 1), op, self.expr(T::Bool, depth + 1))
            }
        }
    }

    fn block(&mut self, depth: usize, ret: Option<T>) -> String {
        self.marks.push(self.vars.len());
        let n = 1 + self.rng.below(3);
        let stmts: Vec<String> = (0..n).map(|_| self.stmt(depth + 1, ret)).collect();
        let mark = self.marks.pop().unwrap();
        self.vars.truncate(mark);
        stmts.join(" ")
    }

    fn stmt(&mut self, depth: usize, ret: Option<T>) -> String {
        let k = self.rng.below(100);
        if k < 35 || depth > 2 {
            let t = self.ty();
            let init = self.expr(t, 0);
            let reuse = if self.rng.chance(15) { self.var(t, true) } else { None };
            let name = reuse.unwrap_or_else(|| self.fresh("v"));
            let annotation = if self.rng.chance(40) { format!(": {}", name_of(t)) } else { String::new() };
            self.vars.push(Var { name: name.clone(), ty: t, assignable: true });
            return format!("let {}{} = {};", name, annotation, init);
        }
        if k < 55 {
            // inside a loop a string is never reassigned: `s = s + s` would double it each time round
            let t = if self.in_loop > 0 { TYPES[self.rng.below(3)] } else { self.ty() };
            return match self.var(t, true) {
                Some(name) => format!("{} = {};", name, self.expr(t, 0)),
                None => format!("{};", self.expr(t, 0)),
            };
        }
        if k < 70 {
            let mut s = format!("if {} {{ {} }}", self.expr(T::Bool, 0), self.block(depth, ret));
            while self.rng.chance(30) {
                s += &format!(" else if {} {{ {} }}", self.expr(T::Bool, 0), self.block(depth, ret));
            }
            if self.rng.chance(60) { s += &format!(" else {{ {} }}", self.block(depth, ret)); }
            return s;
        }
        if k < 85 {
            let counter = self.fresh("i");
            self.vars.push(Var { name: counter.clone(), ty: T::Int, assignable: false });
            let limit = self.rng.below(5);
            self.in_loop += 1;
            let body = self.block(depth, ret);
            self.in_loop -= 1;
            return format!("let {c} = 0; while {c} < {limit} {{ {body} {c} = {c} + 1; }}", c = counter, limit = limit, body = body);
        }
        if k < 92 {
            if let Some(t) = ret { return format!("return {};", self.expr(t, 0)); }
        }
        let t = self.ty();
        format!("{};", self.expr(t, 0))
    }

    fn function(&mut self) -> String {
        let name = self.fresh("f");
        let n = self.rng.below(4);
        let params: Vec<(String, T)> = (0..n).map(|_| { let t = self.ty(); (self.fresh("p"), t) }).collect();
        let ret = self.ty();
        // a function body sees only its parameters
        let outer_vars = std::mem::take(&mut self.vars);
        let outer_marks = std::mem::take(&mut self.marks);
        for (p, t) in &params { self.vars.push(Var { name: p.clone(), ty: *t, assignable: true }); }
        let n = self.rng.below(4);
        let mut body: Vec<String> = (0..n).map(|_| self.stmt(1, Some(ret))).collect();
        body.push(format!("return {};", self.expr(ret, 0)));
        self.vars = outer_vars;
        self.marks = outer_marks;
        let signature: Vec<String> = params.iter().map(|(p, t)| format!("{}: {}", p, name_of(*t))).collect();
        self.fns.push((name.clone(), params.iter().map(|(_, t)| *t).collect(), ret));
        format!("fn {}({}) -> {} {{ {} }}", name, signature.join(", "), name_of(ret), body.join(" "))
    }

    fn program(&mut self) -> String {
        let mut out = Vec::new();
        for _ in 0..self.rng.below(4) { out.push(self.function()); }
        for _ in 0..1 + self.rng.below(6) { out.push(self.stmt(0, None)); }
        let t = self.ty();
        out.push(format!("{};", self.expr(t, 0)));
        out.join("\n")
    }
}

#[test]
fn generated_programs_agree() {
    const PROGRAMS: u64 = 3000;
    let mut values = 0;
    for seed in 1..=PROGRAMS {
        let src = Gen::new(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15)).program();
        let (vm, it) = both(&src);
        assert_eq!(vm, it, "backends disagree (left: VM, right: interpreter) on seed {}:\n{}\n", seed, src);
        if !vm.starts_with("error") { values += 1; }
    }
    // the comparison means little if most programs just fail the same way
    assert!(values > PROGRAMS * 8 / 10, "only {} of {} programs produced a value", values, PROGRAMS);
}
