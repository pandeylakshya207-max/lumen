pub mod token;
pub mod lexer;
pub mod ast;
pub mod parser;
pub mod typeck;
pub mod compiler;
pub mod vm;
pub mod interpreter;
pub mod bench;

/// Most function calls that may be active at once, on either backend.
/// Runaway recursion ends in a "stack overflow" runtime error at this depth
/// instead of exhausting memory (VM) or the native stack (interpreter).
pub const MAX_CALL_DEPTH: usize = 1000;
