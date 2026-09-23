//! Standard library header definitions embedded into the compiler.
//! Dependency chains: `io` -> `stream_io` -> `stream`, and the Zero-written
//! `math` -> `lowlevel_math`, `strings` -> `lowlevel_strings`.

pub const STD_DEPS: &[(&str, &[&str])] = &[
    ("io", &["stream_io"]),
    ("stream_io", &["stream"]),
    ("stream", &[]),
    ("math", &["lowlevel_math"]),
    ("lowlevel_math", &[]),
    ("strings", &["lowlevel_strings"]),
    ("lowlevel_strings", &[]),
];

/// Standard library header sources, embedded into the compiler binary.
/// `io`/`stream*` and the `lowlevel_*` headers are raw Rust; `math` and
/// `strings` are written in Zero (parsed as `.zh` headers, building on the
/// matching lowlevel headers).
pub(crate) fn std_source(name: &str) -> &'static str {
    match name {
        "io" => include_str!("../../std/io.rs"),
        "stream_io" => include_str!("../../std/stream_io.rs"),
        "stream" => include_str!("../../std/stream.rs"),
        "lowlevel_math" => include_str!("../../std/lowlevel_math.rs"),
        "math" => include_str!("../../std/math.zh"),
        "lowlevel_strings" => include_str!("../../std/lowlevel_strings.rs"),
        "strings" => include_str!("../../std/strings.zh"),
        _ => unreachable!("unknown std header: {name}"),
    }
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
