//! Standard library header registry.
//! Dependency chains: `io` -> `stream_io` -> `stream`, `math` ->
//! `lowlevel_math`, `strings` -> `lowlevel_strings`, `network` ->
//! `network_stream`, plus the syntax libraries `win` / `control`.
//!
//! Sources are loaded from disk at runtime (not compiled into the compiler),
//! so editing the standard library never requires rebuilding the compiler.

pub const STD_DEPS: &[(&str, &[&str])] = &[
    ("io", &["stream_io"]),
    ("stream_io", &["stream"]),
    ("stream", &[]),
    ("math", &["lowlevel_math"]),
    ("lowlevel_math", &[]),
    ("strings", &["lowlevel_strings"]),
    ("lowlevel_strings", &[]),
    ("sys", &["lowlevel_sys"]),
    ("lowlevel_sys", &[]),
    ("win", &["lowlevel_win"]),
    ("lowlevel_win", &[]),
    ("sleep", &["lowlevel_sleep"]),
    ("lowlevel_sleep", &[]),
    ("network_stream", &["stream"]),
    ("network", &["network_stream"]),
];

/// Standard library header file names (`name` -> file). `.rs` files are raw
/// Rust; `.zh` files are written in Zero and compiled.
pub const STD_FILES: &[(&str, &str)] = &[
    ("stream", "stream.rs"),
    ("stream_io", "stream_io.rs"),
    ("io", "io.rs"),
    ("lowlevel_math", "lowlevel_math.rs"),
    ("math", "math.zh"),
    ("lowlevel_strings", "lowlevel_strings.rs"),
    ("strings", "strings.zh"),
    ("lowlevel_sys", "lowlevel_sys.rs"),
    ("sys", "sys.zh"),
    ("lowlevel_win", "lowlevel_win.rs"),
    ("lowlevel_sleep", "lowlevel_sleep.rs"),
    ("sleep", "sleep.zh"),
    ("network_stream", "network_stream.rs"),
];

/// Directory holding the standard library files. Resolution order:
/// 1. `ZERO_STD` environment variable;
/// 2. `<exe>/../std` (installed layout);
/// 3. `./std` (source / test layout).
fn std_dir() -> String {
    if let Ok(dir) = std::env::var("ZERO_STD") {
        return dir;
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let candidate = parent.join("..").join("std");
            if candidate.is_dir() {
                return candidate.display().to_string();
            }
        }
    }
    // 从当前目录向上逐级查找 std/（覆盖 workspace 根、crate 测试目录、
    // 源码检出等场景）。
    if let Ok(cwd) = std::env::current_dir() {
        let mut dir = cwd.as_path();
        loop {
            let candidate = dir.join("std");
            if candidate.join("io.rs").is_file() {
                return candidate.display().to_string();
            }
            match dir.parent() {
                Some(p) => dir = p,
                None => break,
            }
        }
    }
    "std".to_string()
}

/// Read a standard library header source from disk.
pub(crate) fn std_source(name: &str) -> Result<String, String> {
    let file = STD_FILES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, f)| *f)
        .ok_or_else(|| format!("unknown std header: {name}"))?;
    let path = std::path::Path::new(&std_dir()).join(file);
    std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read std header '{}': {e}", path.display()))
}

/// Builtin header aliases mapped to Rust `use` statements.
pub const BUILTIN_IMPORTS: &[(&str, &str)] = &[
    // NOTE: I/O lives in the `io` standard header, not in `stdio`.
    ("fs", "use std::fs;"),
    ("path", "use std::path::{Path, PathBuf};"),
    ("process", "use std::process::Command;"),
    ("env", "use std::env;"),
    ("time", "use std::time;"),
    ("collections", "use std::collections;"),
    ("string", "use std::string::String;"),
];

pub(crate) fn builtin_use(name: &str) -> Option<&'static str> {
    BUILTIN_IMPORTS
        .iter()
        .find(|(alias, _)| *alias == name)
        .map(|(_, use_stmt)| *use_stmt)
}
