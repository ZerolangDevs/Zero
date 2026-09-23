//! Header import resolution for the Zero language.
//!
//! `import name` resolves in this order:
//!   1. standard library headers (dependency chains `io -> stream_io ->
//!      stream`, `math -> lowlevel_math`, `strings -> lowlevel_strings`;
//!      stream* and the lowlevel_* headers are inlined as raw Rust, `math`
//!      and `strings` are written in Zero and compiled)
//!   2. builtin alias table                  -> emits a Rust `use` statement
//!   3. a file next to the importing source:
//!        - `<name>.zh` / `<name>.zero` -> recursively compiled Zero header
//!        - `<name>.rs`                 -> raw Rust, inlined verbatim
//! Files are included once even when reachable through several imports.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::ast::{Function, Item, Program};
use crate::diag::CompileError;
use crate::parser;
use crate::scope::{FnInfo, IO_FUNCTIONS};

/// Standard library dependency chain: io -> stream_io -> stream.
mod stdlib;

use stdlib::{builtin_use, std_source};
pub use stdlib::{BUILTIN_IMPORTS, STD_DEPS};

pub struct Loader {
    root_dir: PathBuf,
    /// True once `import io` has been seen: the IO API becomes available.
    pub io_imported: bool,
    /// Builtin aliases used (ordered, deduplicated).
    pub builtins: Vec<String>,
    /// Raw Rust headers, inlined in order: (display path, content).
    pub rust_headers: Vec<(String, String)>,
    /// Compiled Zero headers: (display path, program).
    pub zero_headers: Vec<(String, Program)>,
    /// Every callable function in main + headers (+ io).
    pub functions: HashMap<String, FnInfo>,
    imported_files: HashSet<PathBuf>,
    builtin_set: HashSet<String>,
    std_set: HashSet<String>,
}

impl Loader {
    pub fn new(root_dir: &Path) -> Self {
        Loader {
            root_dir: root_dir.to_path_buf(),
            io_imported: false,
            builtins: Vec::new(),
            rust_headers: Vec::new(),
            zero_headers: Vec::new(),
            functions: HashMap::new(),
            imported_files: HashSet::new(),
            builtin_set: HashSet::new(),
            std_set: HashSet::new(),
        }
    }

    /// Register the functions of the main program (including `main`).
    pub fn register_main(&mut self, prog: &Program, file: &str) -> Result<(), CompileError> {
        for f in prog.functions() {
            self.register_function(f, file)?;
        }
        Ok(())
    }

    fn register_function(&mut self, f: &Function, file: &str) -> Result<(), CompileError> {
        if self.functions.contains_key(&f.name) {
            return Err(CompileError::at(
                format!("duplicate function '{}'", f.name),
                file.to_string(),
                f.span,
            ));
        }
        self.functions.insert(
            f.name.clone(),
            FnInfo {
                arity: Some(f.params.len()),
                params: f.params.iter().map(|p| p.ty).collect(),
            },
        );
        Ok(())
    }

    /// Load all headers reachable from `prog` (recursively).
    pub fn load(&mut self, prog: &Program, file: &str) -> Result<(), CompileError> {
        let root = self.root_dir.clone();
        self.load_program(prog, file, &root)
    }

    fn load_program(&mut self, prog: &Program, file: &str, dir: &Path) -> Result<(), CompileError> {
        let imports: Vec<_> = prog
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Import(imp) => Some(imp.clone()),
                _ => None,
            })
            .collect();

        for imp in imports {
            // 0. control-flow library: pure syntax, enabled by the parser
            if imp.name == "control" {
                continue;
            }

            // 1. standard library headers
            if STD_DEPS.iter().any(|(name, _)| *name == imp.name) {
                self.load_std(&imp.name)?;
                continue;
            }

            // 2. builtin aliases -> `use` statements
            if builtin_use(&imp.name).is_some() {
                if self.builtin_set.insert(imp.name.clone()) {
                    self.builtins.push(imp.name.clone());
                }
                continue;
            }

            // 3. header files
            let path = self.resolve_file(&imp.name, dir).ok_or_else(|| {
                let searched = self
                    .candidates(&imp.name, dir)
                    .into_iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                CompileError::at(
                    format!("cannot find header '{}' (searched: {searched})", imp.name),
                    file.to_string(),
                    imp.span,
                )
            })?;

            let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
            if !self.imported_files.insert(canonical.clone()) {
                continue; // already included once
            }

            let ext = canonical
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            match ext.as_str() {
                "rs" => {
                    let content = std::fs::read_to_string(&canonical).map_err(|e| {
                        CompileError::at(
                            format!("cannot read header '{}': {e}", canonical.display()),
                            file.to_string(),
                            imp.span,
                        )
                    })?;
                    self.register_rust_fns(&content);
                    self.rust_headers
                        .push((path.display().to_string(), content));
                }
                "zh" | "zero" => {
                    let content = std::fs::read_to_string(&canonical).map_err(|e| {
                        CompileError::at(
                            format!("cannot read header '{}': {e}", canonical.display()),
                            file.to_string(),
                            imp.span,
                        )
                    })?;
                    let display = path.display().to_string();
                    let header_prog = parser::parse(&content, &display)?;
                    for f in header_prog.functions() {
                        // A header may not define the entry point.
                        if f.name != "main" {
                            self.register_function(f, &display)?;
                        }
                    }
                    self.zero_headers
                        .push((display.clone(), header_prog.clone()));
                    let parent = path.parent().unwrap_or(Path::new("."));
                    self.load_program(&header_prog, &display, parent)?;
                }
                other => {
                    return Err(CompileError::at(
                        format!("unsupported header extension '.{other}'"),
                        file.to_string(),
                        imp.span,
                    ));
                }
            }
        }
        Ok(())
    }

    /// Load a standard library header plus its dependency chain.
    fn load_std(&mut self, name: &str) -> Result<(), CompileError> {
        if self.std_set.contains(name) {
            return Ok(());
        }
        self.std_set.insert(name.to_string());

        if name == "io" {
            self.io_imported = true;
            for fname in IO_FUNCTIONS.iter() {
                self.functions.entry(fname.to_string()).or_insert(FnInfo {
                    // Arity is checked specially by the analyzer.
                    arity: None,
                    params: Vec::new(),
                });
            }
        }

        // Dependencies first, then the header itself
        // (stream < stream_io < io, lowlevel_math < math).
        for dep in STD_DEPS
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, deps)| *deps)
            .unwrap_or(&[])
        {
            self.load_std(dep)?;
        }

        if name == "math" || name == "strings" {
            // These std headers are written in Zero: parse them like a
            // `.zh` header so their functions participate in scope analysis
            // and codegen. They build on the matching raw-Rust lowlevel
            // header (loaded above).
            let display = format!("std/{name}.zh");
            let header_prog = parser::parse(std_source(name), &display)?;
            for f in header_prog.functions() {
                if f.name != "main" {
                    self.register_function(f, &display)?;
                }
            }
            self.zero_headers.push((display, header_prog));
            return Ok(());
        }

        // Every other std header is raw Rust: inline it. The lowlevel
        // headers additionally expose callable names (`zadd`, `zlen`, ...)
        // used by their Zero wrappers.
        let content = std_source(name);
        if name == "lowlevel_math" || name == "lowlevel_strings" {
            self.register_rust_fns(&content);
        }
        self.rust_headers
            .push((format!("std/{name}.rs"), content.to_string()));
        Ok(())
    }

    /// Register function names found in a raw Rust header by scanning for
    /// `fn <name>`. A light heuristic; strings/comments are not skipped, but
    /// false positives only widen the set of callable names, never break.
    /// Arity is unknown for raw Rust functions, so it is left unchecked.
    fn register_rust_fns(&mut self, content: &str) {
        // Byte-wise scan that stays on UTF-8 char boundaries, so headers
        // with non-ASCII comments (e.g. Chinese) never panic. A light
        // heuristic: strings/comments are not skipped, but false positives
        // only widen the set of callable names, never break. Arity is
        // unknown for raw Rust functions, so it is left unchecked.
        let bytes = content.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if content[i..].starts_with("fn ") || content[i..].starts_with("fn\t") {
                let mut j = i + 2;
                while j < bytes.len() && (bytes[j] as char).is_whitespace() {
                    j += 1;
                }
                let start = j;
                while j < bytes.len() && ((bytes[j] as char).is_alphanumeric() || bytes[j] == b'_')
                {
                    j += 1;
                }
                if j > start {
                    let name = &content[start..j];
                    self.functions.entry(name.to_string()).or_insert(FnInfo {
                        arity: None,
                        params: Vec::new(),
                    });
                }
                i = j;
            } else {
                // Advance past one full UTF-8 character.
                i += 1;
                while i < bytes.len() && (bytes[i] & 0b1100_0000) == 0b1000_0000 {
                    i += 1;
                }
            }
        }
    }

    fn candidates(&self, name: &str, dir: &Path) -> Vec<PathBuf> {
        let p = Path::new(name);
        let mut out = vec![dir.join(p)];
        if p.extension().is_none() {
            for ext in ["zh", "zero", "rs"] {
                out.push(dir.join(format!("{name}.{ext}")));
            }
        }
        out
    }

    fn resolve_file(&self, name: &str, dir: &Path) -> Option<PathBuf> {
        self.candidates(name, dir).into_iter().find(|p| p.is_file())
    }
}
