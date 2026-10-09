use std::io::{self, Write};
use lumen::{compiler::Compiler, interpreter::Interpreter, parser::Parser, typeck::TypeChecker, vm::Vm, bench};

/// The type checker, compiler and interpreter recurse over the syntax tree, and
/// the interpreter recurses once more for every lumen function call. The work
/// runs on a thread with a stack of known size so that the limits in the
/// parser (nesting depth) and the backends (call depth) are safe on every
/// platform, including Windows where the main thread gets only 1 MB.
const STACK_BYTES: usize = 64 * 1024 * 1024;

fn main() {
    let worker = std::thread::Builder::new().stack_size(STACK_BYTES).spawn(run).expect("cannot start worker thread");
    if worker.join().is_err() { std::process::exit(101); }
}

fn run() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("--bench")  => run_bench(),
        Some("--interp") => { if let Some(f) = args.get(2) { run_file_interp(f) } else { repl_interp() } }
        Some(file)       => run_file_vm(file),
        None             => repl_vm(),
    }
}
fn exec_vm(src: &str) {
    let stmts = match Parser::new(src).parse_program() {
        Ok(s) => s, Err(e) => { eprintln!("[parse error] {}  (line {}, col {})", e.msg, e.line, e.col); return; }
    };
    if let Err(e) = TypeChecker::new().check_program(&stmts) { eprintln!("[type error] {}", e.msg); return; }
    let mut c = Compiler::new();
    let chunk = match c.compile_program(&stmts) { Ok(ch) => ch, Err(e) => { eprintln!("[compile error] {}", e.0); return; } };
    match Vm::new(c.fns).run(chunk) {
        Ok(v) => { if format!("{}", v) != "nil" { println!("{}", v); } }
        Err(e) => eprintln!("[runtime error] {}", e.0),
    }
}
fn exec_interp(src: &str) {
    let stmts = match Parser::new(src).parse_program() {
        Ok(s) => s, Err(e) => { eprintln!("[parse error] {}  (line {}, col {})", e.msg, e.line, e.col); return; }
    };
    if let Err(e) = TypeChecker::new().check_program(&stmts) { eprintln!("[type error] {}", e.msg); return; }
    match Interpreter::new().run_program(&stmts) {
        Ok(v) => { if format!("{}", v) != "nil" { println!("{}", v); } }
        Err(e) => eprintln!("[runtime error] {}", e.0),
    }
}
fn repl_vm() {
    println!("lumen v0.1.0  [bytecode VM]  — Ctrl-C to exit");
    loop { print!(">> "); io::stdout().flush().unwrap(); let mut l = String::new(); if io::stdin().read_line(&mut l).unwrap() == 0 { break; } let s = l.trim(); if s.is_empty() { continue; } exec_vm(s); }
}
fn repl_interp() {
    println!("lumen v0.1.0  [tree-walking interpreter]  — Ctrl-C to exit");
    loop { print!(">> "); io::stdout().flush().unwrap(); let mut l = String::new(); if io::stdin().read_line(&mut l).unwrap() == 0 { break; } let s = l.trim(); if s.is_empty() { continue; } exec_interp(s); }
}
fn run_file_vm(path: &str) { let src = std::fs::read_to_string(path).unwrap_or_else(|e| { eprintln!("error: {}", e); std::process::exit(1) }); exec_vm(&src); }
fn run_file_interp(path: &str) { let src = std::fs::read_to_string(path).unwrap_or_else(|e| { eprintln!("error: {}", e); std::process::exit(1) }); exec_interp(&src); }
fn run_bench() {
    println!("lumen benchmark — VM vs tree-walking interpreter\n{}", "─".repeat(60));
    for r in bench::run_suite() { println!("{}\n", r); }
}
