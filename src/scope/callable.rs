//! Callable metadata: builtin functions, `io` API names and arity.

pub const BUILTIN_CALL_RUST: &str = "call_rust";
pub const BUILTIN_CALL_SYS: &str = "call_sys";

/// Standard library `io` functions (only callable after `import io`).
pub const IO_FUNCTIONS: &[&str] = &["print", "input_s", "input", "set_stream"];

/// Metadata for every callable function known to the compilation unit.
#[derive(Debug, Clone)]
pub struct FnInfo {
    /// Fixed arity. `None` means the arity is unknown (raw `.rs` header) or
    /// variable (`io` functions are checked specially).
    pub arity: Option<usize>,
}
