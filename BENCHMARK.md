# Lumen — VM vs Tree-Walking Interpreter Benchmark

Lumen ships two execution backends:

- **Bytecode VM** (`src/compiler.rs` + `src/vm.rs`) — compiles the AST to a flat `Vec<Op>` of typed opcodes then executes on a stack machine with a flat locals pool and call frames.
- **Tree-walking interpreter** (`src/interpreter.rs`) — walks AST nodes directly, using a `Vec<HashMap>` scope chain for variable lookup and cloning `FnDef` structs on each call.

Both backends use the same lexer, parser, and type checker. The comparison is purely about execution strategy.

## Results

Hardware: Ubuntu 24 (Linux), single core, release build (`opt-level = 3`).  
Each program run **10 000 iterations**; timing shown is **average nanoseconds per run**.

```
Program                                               VM (ns)   Interp (ns)   Winner
─────────────────────────────────────────────────────────────────────────────────────
1 + 2;                                                     92           72   Interp ×1.28
let x = 10; let y = 20; let z = x + y; z;                150          305   VM     ×2.03
fn add(a: int, b: int) { return a; } add(3, 4);           305          577   VM     ×1.89
if/else branch (bool cond, two let bodies)                147          237   VM     ×1.61
Fibonacci-10 iterative (while + assign loop)             1086         5540   VM     ×5.10
```

## Analysis

**Why the interpreter wins on `1 + 2;` (×1.28)**

The VM pipeline has fixed startup overhead: `compile_program()` allocates a `Vec<Op>`, `Vm::new()` clones the `fns` HashMap, and the dispatch loop enters. For a 3-instruction program (`Const`, `Const`, `AddInt`) this overhead exceeds the cost of a single `eval_expr` recursive call. The crossover happens once programs have 3+ variables.

**Why the VM dominates everywhere else**

Variable lookup is the core difference. The interpreter walks a `Vec<HashMap>` scope chain on every `Var` expression — hashing the name, searching scopes in reverse. The VM resolves variable names to integer slot indices at compile time; `LoadLocal(n)` is a direct `locals[base + n]` array index — no hashing, no scope walk, no iteration.

**Function calls (×1.89)**

The interpreter clones the entire `FnDef` (params + body `Vec<Stmt>`) on each call. The VM calls into a pre-compiled `Chunk` (`Vec<Op>`), pushes a `Frame`, and binds args by moving `Value`s into the locals pool — cheaper, and the delta grows with call frequency.

**While loop / iteration (×5.10)**

The clearest signal. The iterative Fibonacci-10 program accesses `i`, `a`, `b`, `tmp` on every iteration — 4 lookups and 4 stores per loop body. The interpreter scope-walks each one; the VM dispatches each as a direct array index. Over 10 iterations × 10 000 benchmark runs the difference dominates.

**Takeaway**

The tree-walking interpreter is simpler to write and fast enough for short scripts. The bytecode VM becomes meaningfully faster the moment programs use variables and loops — which is almost every real program. This matches how production language runtimes make the same tradeoff (CPython bytecode VM, Ruby YARV, etc.).

## Reproducing

```bash
cargo run --release -- --bench
```

To time a single program in code:

```rust
use lumen::bench::bench;
let result = bench("let x = 0; while x < 100 { x = x + 1; } x;", 10_000);
println!("{}", result);
```
