use std::time::Instant;
use crate::parser::Parser;
use crate::compiler::Compiler;
use crate::vm::Vm;
use crate::interpreter::Interpreter;

pub struct BenchResult {
    pub program: &'static str,
    pub vm_ns: u128,
    pub interp_ns: u128,
    pub vm_faster: bool,
    pub speedup: f64,
}
impl std::fmt::Display for BenchResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f,
            "{}\n  VM:          {:>10} ns\n  Interpreter: {:>10} ns\n  {} is {:.2}x faster",
            self.program, self.vm_ns, self.interp_ns,
            if self.vm_faster { "VM" } else { "Interpreter" }, self.speedup)
    }
}
pub fn bench(program: &'static str, iters: u32) -> BenchResult {
    let stmts = Parser::new(program).parse_program().expect("parse");
    let mut compiler = Compiler::new();
    let chunk = compiler.compile_program(&stmts).expect("compile");
    let fns = compiler.fns.clone();
    let vm_start = Instant::now();
    for _ in 0..iters { Vm::new(fns.clone()).run(chunk.clone()).expect("vm"); }
    let vm_ns = vm_start.elapsed().as_nanos() / iters as u128;
    let interp_start = Instant::now();
    for _ in 0..iters { Interpreter::new().run_program(&stmts).expect("interp"); }
    let interp_ns = interp_start.elapsed().as_nanos() / iters as u128;
    let vm_faster = vm_ns <= interp_ns;
    let speedup = if vm_faster { interp_ns as f64 / vm_ns.max(1) as f64 }
                  else { vm_ns as f64 / interp_ns.max(1) as f64 };
    BenchResult { program, vm_ns, interp_ns, vm_faster, speedup }
}
pub fn run_suite() -> Vec<BenchResult> {
    const ITERS: u32 = 10_000;
    vec![
        bench("1 + 2;", ITERS),
        bench("let x = 10; let y = 20; let z = x + y; z;", ITERS),
        bench("fn add(a: int, b: int) { return a; } add(3, 4);", ITERS),
        bench("let x = true; if x { let y = 1; } else { let y = 2; } nil;", ITERS),
        bench("let a = 0; let b = 1; let i = 0; while i < 10 { let tmp = b; b = a + b; a = tmp; i = i + 1; } a;", ITERS),
    ]
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bench_runs_without_panic() {
        let results = run_suite();
        assert_eq!(results.len(), 5);
        for r in &results { let _ = format!("{}", r); }
    }
    #[test]
    fn bench_single_program() {
        let r = bench("1 + 2;", 100);
        assert!(r.speedup >= 0.0);
    }
}
