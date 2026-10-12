//! Zero compiler CLI.
//!
//! Usage: zeroc <input.zero> [-o <output.rs>] [--build]
//! Without `-o`, the generated Rust code is printed to stdout. With
//! `--build`, the Rust code is compiled into an executable via `rustc`.

mod ast;
mod codegen;
mod diag;
mod import;
mod lexer;
mod parser;
mod scope;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use diag::CompileError;

const USAGE: &str = "\
Zero compiler (zeroc) v0.3.0
Compiles Zero source code to Rust.

USAGE:
    zeroc <input.zero> [-o <output.rs>] [--build]

OPTIONS:
    -o <output.rs>   Write generated Rust code to a file (default: stdout)
    --build          After generating Rust code, compile it into an
                     executable with rustc (sits next to the .rs file)
    -h, --help       Show this help

EXAMPLES:
    zeroc hello.zero -o hello.rs
    zeroc hello.zero --build            # hello.rs + hello / hello.exe
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();

    if args.len() == 1 || args[1] == "-h" || args[1] == "--help" {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }

    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut build = false;
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
            "--build" => build = true,
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
        Ok((code, uses_win)) => {
            if build {
                build_and_compile(&input, output.as_deref(), &code, uses_win)
            } else {
                match output {
                    Some(out) => {
                        if let Err(e) = std::fs::write(&out, &code) {
                            eprintln!("error: cannot write '{}': {e}", out.display());
                            return ExitCode::FAILURE;
                        }
                        println!("✓ generated {}", out.display());
                        if uses_win {
                            eprintln!("note: this program uses `win`; use --build to compile it with Slint");
                        }
                        ExitCode::SUCCESS
                    }
                    None => {
                        print!("{code}");
                        ExitCode::SUCCESS
                    }
                }
            }
        }
        Err(err) => {
            eprintln!("{}", render_error(&err));
            ExitCode::FAILURE
        }
    }
}

/// Generate Rust code to disk and compile it into an executable.
fn build_and_compile(input: &Path, output: Option<&Path>, code: &str, uses_win: bool) -> ExitCode {
    // Resolve the Rust output path: `-o` wins, otherwise `<stem>.rs` in the
    // current directory, based on the input file name.
    let rust_path = match output {
        Some(p) => p.to_path_buf(),
        None => {
            let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("out");
            PathBuf::from(format!("{stem}.rs"))
        }
    };

    if let Err(e) = std::fs::write(&rust_path, code) {
        eprintln!("error: cannot write '{}': {e}", rust_path.display());
        return ExitCode::FAILURE;
    }
    println!("✓ generated {}", rust_path.display());

    let result = if uses_win {
        build_with_cargo(&rust_path)
    } else {
        build_with_rustc(&rust_path)
    };
    match result {
        Ok(exe_path) => {
            println!("✓ built {}", exe_path.display());
            ExitCode::SUCCESS
        }
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::FAILURE
        }
    }
}

/// Build a program that uses `win` as a Cargo project with the Slint
/// dependency (Material Design via SLINT_STYLE=material). The generated
/// code is copied into `<stem>_app/src/main.rs`; the built executable is
/// copied back next to the `.rs` file.
fn build_with_cargo(rust_path: &Path) -> Result<PathBuf, String> {
    let stem = rust_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("out");
    let parent = rust_path.parent().unwrap_or(Path::new("."));
    let app_dir = parent.join(format!("{stem}_app"));

    let main_rs = app_dir.join("src").join("main.rs");
    std::fs::create_dir_all(main_rs.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(app_dir.join(".cargo")).map_err(|e| e.to_string())?;
    std::fs::copy(rust_path, &main_rs).map_err(|e| format!("cannot copy generated code: {e}"))?;

    let cargo_toml = format!(
        "[package]\nname = \"{stem}_app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nslint = {{ version = \"1\", features = [\"backend-winit\", \"renderer-femtovg\"] }}\n\n[[bin]]\nname = \"{stem}_app\"\npath = \"src/main.rs\"\n"
    );
    std::fs::write(app_dir.join("Cargo.toml"), cargo_toml).map_err(|e| e.to_string())?;
    // 共享 target 目录：同一目录下的多个窗口程序复用已编译的 Slint。
    std::fs::write(
        app_dir.join(".cargo").join("config.toml"),
        "[build]\ntarget-dir = \"../.zeroc_target\"\n",
    )
    .map_err(|e| e.to_string())?;

    let status = Command::new("cargo")
        .current_dir(&app_dir)
        .env("SLINT_STYLE", "material")
        .arg("build")
        .status()
        .map_err(|e| format!("cannot run cargo: {e}"))?;
    if !status.success() {
        return Err(format!("cargo failed to build '{}'", rust_path.display()));
    }

    let bin = if exe_suffix().is_empty() {
        format!("{stem}_app")
    } else {
        format!("{stem}_app.{}", exe_suffix())
    };
    let exe = parent.join(".zeroc_target").join("debug").join(&bin);
    let out = rust_path.with_extension(exe_suffix());
    std::fs::copy(&exe, &out).map_err(|e| format!("cannot copy built exe: {e}"))?;
    Ok(out)
}

/// Compile a generated `.rs` file with `rustc`. The first attempt uses the
/// default linker; if that fails (e.g. no MSVC `link.exe`), it retries with
/// the `rust-lld` linker bundled with the Rust toolchain.
fn build_with_rustc(rust_path: &Path) -> Result<PathBuf, String> {
    let exe_path = rust_path.with_extension(exe_suffix());

    let run = |use_lld: bool| -> Result<bool, String> {
        let mut cmd = Command::new("rustc");
        cmd.arg(rust_path).arg("-o").arg(&exe_path);
        if use_lld {
            cmd.args(["-C", "linker=rust-lld"]);
        }
        let status = cmd.status().map_err(|e| format!("cannot run rustc: {e}"))?;
        Ok(status.success())
    };

    if run(false)? {
        return Ok(exe_path);
    }
    eprintln!("note: rustc failed with the default linker, retrying with the bundled rust-lld...");
    if run(true)? {
        return Ok(exe_path);
    }
    Err(format!("rustc failed to build '{}'", rust_path.display()))
}

/// Executable suffix for the current platform ("" on Unix).
fn exe_suffix() -> &'static str {
    if cfg!(windows) {
        "exe"
    } else {
        ""
    }
}

/// Full compile pipeline: parse -> load headers -> scope analysis -> codegen.
fn compile(input: &Path) -> Result<(String, bool), CompileError> {
    let file = input.display().to_string();
    let src = std::fs::read_to_string(input).map_err(|e| {
        CompileError::new(
            format!("cannot read '{}': {e}", input.display()),
            file.clone(),
        )
    })?;

    let prog = parser::parse(&src, &file)?;
    let has_top = prog
        .items
        .iter()
        .any(|item| matches!(item, ast::Item::Top(_)));
    let has_main = prog.has_main();
    if has_top {
        // Python-style entry: top-level statements, no `main` function.
        if has_main {
            return Err(CompileError::new(
                "the entry point is the top-level code; remove 'fn main'",
                file,
            ));
        }
    } else if !has_main {
        return Err(CompileError::new(
            "no top-level statements or 'fn main' function found",
            file,
        ));
    }

    let root = input.parent().unwrap_or(Path::new("."));
    let mut loader = import::Loader::new(root);
    loader.register_main(&prog, &file)?;
    loader.load(&prog, &file)?;

    let fns = &loader.functions;
    let io_imported = loader.io_imported;
    let mut analyzer = scope::ScopeAnalyzer::new(fns, &file, io_imported, loader.sys_imported);
    analyzer.analyze_program(&prog)?;
    for (display, header_prog) in &loader.zero_headers {
        let mut header_analyzer =
            scope::ScopeAnalyzer::new(fns, display, io_imported, loader.sys_imported);
        header_analyzer.analyze_program(header_prog)?;
    }

    let uses_win = loader.win_imported;
    Ok((codegen::generate(&prog, &loader, &file), uses_win))
}

/// Render an error; uses the file on disk for the source snippet.
fn render_error(err: &CompileError) -> String {
    let src = std::fs::read_to_string(&err.file).unwrap_or_default();
    diag::render_error(err, &src)
}
