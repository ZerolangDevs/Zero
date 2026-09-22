//! Standard library header definitions embedded into the compiler.
//! Dependency chain: `io` -> `stream_io` -> `stream`.

pub const STD_DEPS: &[(&str, &[&str])] = &[
    ("io", &["stream_io"]),
    ("stream_io", &["stream"]),
    ("stream", &[]),
];

/// Standard library header sources, embedded into the compiler binary.
pub(crate) fn std_source(name: &str) -> &'static str {
    match name {
        "io" => include_str!("../../std/io.rs"),
        "stream_io" => include_str!("../../std/stream_io.rs"),
        "stream" => include_str!("../../std/stream.rs"),
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
