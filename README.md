# lumen

A small, statically-typed compiled language written in Rust.

Lumen has **two execution backends** — a bytecode compiler + stack VM and a tree-walking interpreter — so their performance can be honestly benchmarked against each other on the same programs.

## Architecture

```
source
  │
  ▼
Lexer (src/lexer.rs)        — &str → Vec<Token>
  │
  ▼
Parser (src/parser.rs)      — tokens → AST (Expr / Stmt)
  │
  ▼
Type Checker (src/typeck.rs) — AST → Ok(()) | TypeError
  │
  ├──▶ Bytecode Compiler (src/compiler.rs)  → Chunk (Vec<Op>)
  │         │
  │         ▼
  │    Stack VM (src/vm.rs)                 → Value
  │
  └──▶ Tree-walking Interpreter (src/interpreter.rs) → Value
```

## Language

```lumen
// variables
let x: int = 42;
let name = "lumen";

// functions
fn add(a: int, b: int) -> int {
    return a;
}

// control flow
if x > 10 {
    print("big");
} else {
    print("small");
}

while x > 0 {
    let x = x;
}
```

### Types
`int` · `float` · `bool` · `str` · `nil`

### Operators
- Arithmetic: `+` `-` `*` `/` `%` (str `+` str = concatenation)
- Comparison: `<` `<=` `>` `>=` `==` `!=`
- Logical: `&&` `||` `!`
- Unary: `-` (numeric) · `!` (bool)

## Usage

```bash
# REPL (VM backend)
cargo run

# REPL (tree-walking interpreter)
cargo run -- --interp

# Run a file (VM)
cargo run -- program.lm

# Run a file (interpreter)
cargo run -- --interp program.lm

# Benchmark VM vs interpreter
cargo run -- --bench
```

## Benchmark

```
cargo run --release -- --bench
```

Runs five programs on both backends (10 000 iterations each) and reports which is faster and by how much.

## Tests

```bash
cargo test
```

189 tests across lexer, parser, type checker, bytecode compiler, VM, interpreter, and benchmark harness.

## Project structure

```
src/
  token.rs       — TokenKind, Token, keyword()
  lexer.rs       — Lexer: source → Vec<Token>
  ast.rs         — Expr, Stmt, BinOp, UnOp, Ty
  parser.rs      — recursive-descent Parser
  typeck.rs      — TypeChecker: scoped env + fn signatures
  compiler.rs    — Compiler: AST → Chunk (Vec<Op>) + Value type
  vm.rs          — Vm: stack-based bytecode executor
  interpreter.rs — Interpreter: tree-walking executor
  bench.rs       — BenchResult, bench(), run_suite()
  main.rs        — CLI: REPL / file runner / --bench
```

## License

MIT
