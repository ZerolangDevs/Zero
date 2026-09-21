//! Zero compiler CLI.
//!
//! Usage: zeroc <input.zero> [-o <output.rs>]
//! Without `-o`, the generated Rust code is printed to stdout.

mod ast;
mod codegen;
mod diag;
mod import;
mod lexer;
mod parser;
mod scope;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use diag::CompileError;

const USAGE: &str = "\
Zero compiler (zeroc) v0.3.0
Compiles Zero source code to Rust.

USAGE:
    zeroc <input.zero> [-o <output.rs>]

OPTIONS:
    -o <output.rs>   Write generated Rust code to a file (default: stdout)
    -h, --help       Show this help

EXAMPLES:
    zeroc hello.zero -o hello.rs
    rustc hello.rs -o hello
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();

    if args.len() == 1 || args[1] == "-h" || args[1] == "--help" {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }

    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-o" => {
                i += 1;
                match args.get(i) {
                    Some(path) => output = Some(PathBuf::from(path)),
                    None => {
                        eprintln!("error: missing path after '-o'");
                        return ExitCode::FAILURE;
                    }
                }
            }
            other if input.is_none() => input = Some(PathBuf::from(other)),
            other => {
                eprintln!("error: unexpected argument '{other}'");
                eprint!("{USAGE}");
                return ExitCode::FAILURE;
            }
        }
        i += 1;
    }

    let Some(input) = input else {
        eprintln!("error: missing input file");
        eprint!("{USAGE}");
        return ExitCode::FAILURE;
    };

    match compile(&input) {
        Ok(code) => match output {
            Some(out) => {
                if let Err(e) = std::fs::write(&out, &code) {
                    eprintln!("error: cannot write '{}': {e}", out.display());
                    return ExitCode::FAILURE;
                }
                println!("✓ generated {}", out.display());
                ExitCode::SUCCESS
            }
            None => {
                print!("{code}");
                ExitCode::SUCCESS
            }
        },
        Err(err) => {
            eprintln!("{}", render_error(&err));
            ExitCode::FAILURE
        }
    }
}

/// Full compile pipeline: parse -> load headers -> scope analysis -> codegen.
fn compile(input: &Path) -> Result<String, CompileError> {
    let file = input.display().to_string();
    let src = std::fs::read_to_string(input).map_err(|e| {
        CompileError::new(
            format!("cannot read '{}': {e}", input.display()),
            file.clone(),
        )
    })?;

    let prog = parser::parse(&src, &file)?;
    if !prog.has_main() {
        return Err(CompileError::new(
            "no 'fn main' function found in program",
            file,
        ));
    }

    let root = input.parent().unwrap_or(Path::new("."));
    let mut loader = import::Loader::new(root);
    loader.register_main(&prog, &file)?;
    loader.load(&prog, &file)?;

    let fns = &loader.functions;
    let io_imported = loader.io_imported;
    let mut analyzer = scope::ScopeAnalyzer::new(fns, &file, io_imported);
    analyzer.analyze_program(&prog)?;
    for (display, header_prog) in &loader.zero_headers {
        let mut header_analyzer = scope::ScopeAnalyzer::new(fns, display, io_imported);
        header_analyzer.analyze_program(header_prog)?;
    }

    Ok(codegen::generate(&prog, &loader, &file))
}

/// Render an error; uses the file on disk for the source snippet.
fn render_error(err: &CompileError) -> String {
    let src = std::fs::read_to_string(&err.file).unwrap_or_default();
    diag::render_error(err, &src)
}
