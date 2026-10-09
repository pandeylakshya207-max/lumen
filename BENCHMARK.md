# Benchmark: bytecode VM vs tree-walking interpreter

Lumen has two execution backends behind one front end (lexer, parser, type checker):

- **Bytecode VM** (`src/compiler.rs`, `src/vm.rs`): the AST is compiled to a flat list of typed opcodes and run on a stack machine. Variables are slot indices into a flat locals pool.
- **Tree-walking interpreter** (`src/interpreter.rs`): the AST is evaluated directly. Variables live in a chain of hash maps, one per scope.

## Results

| # | Program | VM | Interpreter | Faster |
| --- | --- | ---: | ---: | --- |
| 1 | `1 + 2;` | 111 ns | 54 ns | interpreter, 2.06x |
| 2 | `let x = 10; let y = 20; let z = x + y; z;` | 199 ns | 262 ns | VM, 1.32x |
| 3 | `fn add(a: int, b: int) -> int { return a + b; } add(3, 4);` | 366 ns | 619 ns | VM, 1.69x |
| 4 | `let x = true; if x { let y = 1; } else { let y = 2; } nil;` | 195 ns | 199 ns | VM, 1.02x |
| 5 | `let a = 0; let b = 1; let i = 0; while i < 10 { let tmp = b; b = a + b; a = tmp; i = i + 1; } a;` | 1,139 ns | 4,919 ns | VM, 4.32x |
| 6 | `fn fib(n: int) -> int { if n < 2 { return n; } return fib(n - 1) + fib(n - 2); } fib(10);` | 52,401 ns | 123,594 ns | VM, 2.36x |
| 7 | `let x = 1.5; let i = 0; while i < 20 { x = x * 1.01 + 0.5; i = i + 1; } x;` | 1,804 ns | 4,970 ns | VM, 2.75x |

**How these were measured**

- GitHub Actions `ubuntu-latest` runner, release build (`opt-level = 3`), commit `3d16356`, CI run 37895269496.
- Each program is run 10,000 times per backend; the figure is the mean wall-clock time of one run.
- The timed region is execution only. Parsing, type checking and compiling to bytecode happen once, before the clock starts.
- Each timed run starts from scratch: the VM run clones the compiled bytecode and function table into a new `Vm`; the interpreter run builds a new `Interpreter` and registers the functions again.

**How far to trust them**

This is one run on a shared CI machine. There is no warm-up, no repetition and no variance estimate, so treat differences of a few percent as noise: program 4 is a tie. On repeated runs the ordering stays the same and the larger ratios move by a few tenths. CI prints a fresh table on every push (the `Benchmark` step of the Linux job).

## Reading the results

**Program 1: the interpreter wins on a trivial program.** `1 + 2;` is five instructions on the VM and one small tree in the interpreter. At this size the fixed cost of starting a run dominates, and the VM's is larger: it clones the bytecode and the function table before executing anything.

**Programs 2, 5 and 7: variables are where the VM pulls ahead.** The interpreter resolves a variable by hashing its name and searching the scope chain from the innermost scope outwards, and it sets up and tears down a scope map every time a block is entered, which in a loop means once per iteration. The VM resolved every name to a slot index at compile time, so a read is one array index. The gap grows with the number of variable accesses: 1.3x for three variables, 4.3x for a ten-iteration loop that touches four variables per iteration.

**Programs 3 and 6: function calls.** The VM is ahead here too, but by less than in the loops. Both backends do avoidable work on every call: the interpreter clones the function's parameter list and body, and the VM clones the callee's bytecode into the new frame. Sharing these through a reference-counted pointer would speed up both.

**Program 4: no difference.** One variable read and one branch is too little work for the backends to separate.

## What this does and does not show

It shows that, for this language and these implementations, compiling to bytecode with resolved slots is two to four times faster than walking the tree once a program loops or calls functions. That is the same direction as the trade-off made by CPython, Ruby and Lua, which all compile to bytecode.

It does not show how fast lumen is in absolute terms. Neither backend has been optimized: there is no constant folding, values are cloned freely (the VM clones each instruction as it dispatches it), and dispatch is a plain `match`. The programs are also tiny. The comparison is between two straightforward implementations of the same semantics, checked against each other by the differential test in `tests/backends_agree.rs`.

## Reproducing

```bash
cargo run --release -- --bench
```

To time one program from code:

```rust
use lumen::bench::bench;

let result = bench("let x = 0; while x < 100 { x = x + 1; } x;", 10_000);
println!("{}", result);
```
