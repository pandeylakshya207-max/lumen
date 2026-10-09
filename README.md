# lumen

[![ci](https://github.com/pandeylakshya207-max/lumen/actions/workflows/ci.yml/badge.svg)](https://github.com/pandeylakshya207-max/lumen/actions/workflows/ci.yml)

A small statically typed language written in Rust, with **two execution backends** that share one front end:

- a bytecode compiler and stack VM, and
- a tree-walking interpreter.

Having both makes two things possible: the backends can be benchmarked against each other on the same programs, and each one checks the other. A differential test runs thousands of generated programs on both and fails if they ever disagree.

No dependencies outside the Rust standard library.

## A program

This is [`examples/demo.lm`](examples/demo.lm), shortened. The full file is run by the test suite on both backends.

```
fn gcd(a: int, b: int) -> int {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    return a;
}

fn fib(n: int) -> int {
    if n < 2 { return n; }
    return fib(n - 1) + fib(n - 2);
}

fn average(a: float, b: float) -> float {
    return (a + b) / 2.0;
}

let greeting = "hello from " + "lumen";
print(greeting);

if average(1.5, 2.5) == 2.0 && gcd(48, 18) == 6 {
    print("checks passed");
}

gcd(48, 18) + fib(10);    // the last expression statement is the program's result: 61
```

## Usage

```bash
cargo run -- examples/demo.lm            # run a file on the VM
cargo run -- --interp examples/demo.lm   # run a file on the interpreter
cargo run                                # REPL (VM)
cargo run -- --interp                    # REPL (interpreter)
cargo run --release -- --bench           # VM vs interpreter benchmark
cargo test                               # 289 tests
```

In the REPL each line is a complete program; variables do not carry over to the next line.

## The language

**Types:** `int` (64-bit signed), `float` (64-bit), `bool`, `str`, `nil`.

**Statements**

| Statement | Form |
| --- | --- |
| Variable | `let x = 1;` or `let x: float = 1.5;` |
| Assignment | `x = x + 1;` |
| Function | `fn add(a: int, b: int) -> int { return a + b; }` |
| Conditional | `if c { ... } else if d { ... } else { ... }` |
| Loop | `while c { ... }` |
| Return | `return x;` or `return;` |
| Expression | `f(1, 2);` |

**Operators**, loosest to tightest. Binary operators group to the left.

| Operators | Operands |
| --- | --- |
| <code>&#124;&#124;</code> | `bool` |
| `&&` | `bool` |
| `==` `!=` | two values of the same type |
| `<` `<=` `>` `>=` | two `int` or two `float` |
| `+` `-` | two `int` or two `float`; `+` also joins two `str` |
| `*` `/` `%` | two `int` or two `float` (`%` is `int` only) |
| unary `-` `!` | `int`/`float`, `bool` |

**Rules worth knowing**

- There is no implicit conversion: `1 + 1.5` is a type error.
- A block (`{ ... }`) opens a scope. An inner `let` may shadow an outer name, with a different type if you like.
- Functions are declared at the top level and can be called before their declaration. A function body sees only its parameters and its own locals, not the variables around it.
- A function with a return type must return on every path. Without one it returns `nil`.
- `&&` and `||` evaluate both sides.
- The result of a program is the value of the last expression statement that ran.
- `print` takes one `str`.
- The `;` after the last statement of the input is optional, so `1 + 2` works at the REPL.

**Errors**

| Phase | Examples |
| --- | --- |
| Parse (with line and column) | `expected ';', found '}'`, `unterminated string`, `integer literal 99999999999999999999 is too large` |
| Type | `type mismatch in let 'x': declared Int but got Float`, `undefined variable 'g'`, `function 'f' can reach its end without returning a value of type Int` |
| Runtime | `division by zero`, `modulo by zero`, `integer overflow`, `stack overflow: more than 1000 nested calls` |

## How it is built

```
source
  |
Lexer (src/lexer.rs)            text -> tokens
  |
Parser (src/parser.rs)          tokens -> AST; recursive descent, precedence climbing for expressions
  |
Type checker (src/typeck.rs)    AST -> ok, or the first type error
  |
  +--> Compiler (src/compiler.rs)   AST -> bytecode with typed opcodes (AddInt, AddFloat, AddStr, ...)
  |        |
  |    Stack VM (src/vm.rs)         operand stack, flat locals pool, call frames
  |
  +--> Interpreter (src/interpreter.rs)   walks the AST directly with a chain of scopes
```

Some decisions:

- **Typed opcodes.** The VM has a separate opcode per operand type, so it never inspects a value to decide what `+` means. The compiler tracks the static type of every local, parameter and declared return type to choose them.
- **Slots instead of names.** The compiler resolves each variable to a slot index, so the VM reads a local with one array index. The interpreter looks names up in hash maps. This is the main reason for the speed difference.
- **Expression statements are popped.** Each one leaves exactly one value, which the VM removes and remembers as the program's result, so a loop cannot grow the operand stack.
- **Checked arithmetic.** Integer overflow is a runtime error on both backends, in debug and release builds alike.
- **Bounded recursion.** The parser rejects syntax trees deeper than 100 levels, and both backends stop at 1,000 nested calls, so malformed or runaway programs end in an error message. The command-line tool runs on a 64 MB thread so these limits hold on Windows too.

## Testing

289 tests, run on Linux and Windows on every push.

| Where | Tests | What |
| --- | --- | --- |
| `lexer`, `token` | 26 | tokens, positions, comments, lexical errors |
| `parser` | 56 | every statement form, precedence, associativity, error messages, depth limits |
| `typeck` | 55 | type rules, scoping, function checks |
| `compiler` | 33 | exact bytecode for each construct |
| `vm` | 57 | results and runtime errors |
| `interpreter` | 56 | results and runtime errors |
| `bench` | 2 | the benchmark harness runs |
| `tests/backends_agree.rs` | 4 | both backends against each other |

`tests/backends_agree.rs` holds the differential test. A seeded generator writes 3,000 well-typed programs (functions, loops, conditionals, shadowing, all four value types). Each one is type-checked, run on both backends, and the two results are compared. The same file checks 24 hand-written programs against known answers and confirms that both backends hit the recursion limit at the same depth.

This test found real bugs when it was introduced. The VM chose the integer opcode for any float, string or boolean that was not a literal, so `let x = 1.5; x + 2.5;` failed at runtime. Expression statements inside a function left values on the operand stack and corrupted the caller's arithmetic. The interpreter let a function read its caller's variables while the VM refused to compile the same program. Before those fixes about 90% of generated programs gave different results on the two backends, most of them because of the opcode bug; none do now.

## Benchmark

Average time per run over 10,000 runs, release build, on a GitHub Actions Linux runner:

| Program | VM | Interpreter | Faster |
| --- | ---: | ---: | --- |
| `1 + 2;` | 111 ns | 54 ns | interpreter, 2.1x |
| three variables and an add | 199 ns | 262 ns | VM, 1.3x |
| one function call | 366 ns | 619 ns | VM, 1.7x |
| `if`/`else` | 195 ns | 199 ns | no real difference |
| iterative Fibonacci, 10 iterations | 1,139 ns | 4,919 ns | VM, 4.3x |
| recursive `fib(10)` | 52,401 ns | 123,594 ns | VM, 2.4x |
| float loop, 20 iterations | 1,804 ns | 4,970 ns | VM, 2.8x |

The VM wins once a program uses variables, loops or calls; the interpreter wins on a three-instruction program because the VM has more to set up per run. Details, the exact programs and caveats are in [BENCHMARK.md](BENCHMARK.md). CI prints a fresh table on every push.

## Limits and gaps

- No arrays, structs, closures or modules. Strings support only `+`, `==` and `!=`.
- There is no conversion between types, so `print` can show only strings. Other values are visible as the result of the program.
- Functions cannot read or change top-level variables.
- No `break` or `continue`.
- Type errors do not carry a line number yet; parse errors do.
- A single expression can chain about 100 operators, because of the depth limit.
- The interpreter recurses on the native stack. A call that is 1,000 levels deep and also sits inside dozens of nested blocks or brackets at every level can exhaust it before the call limit is reached.

## Layout

```
src/
  token.rs        token kinds and keywords
  lexer.rs        text -> tokens
  ast.rs          Expr, Stmt, BinOp, UnOp, Ty
  parser.rs       tokens -> AST
  typeck.rs       static type checker
  compiler.rs     AST -> bytecode; the Value and Op types
  vm.rs           bytecode VM
  interpreter.rs  tree-walking interpreter
  bench.rs        benchmark harness
  main.rs         command-line tool: file runner, REPL, --bench
tests/
  backends_agree.rs   differential tests
examples/
  demo.lm
```

## License

MIT
