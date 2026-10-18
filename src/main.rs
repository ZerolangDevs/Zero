//! Zero compiler CLI.
//!
//! Usage: zero <input.zero> [-rust | -wasm] [-o <output>]
//! Default builds a native executable for the current platform. `-rust`
//! only emits Rust source; `-wasm` compiles to WebAssembly.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use zeroc_ast as ast;
use zeroc_codegen as codegen;
use zeroc_diag as diag;
use zeroc_import as import;
use zeroc_parser as parser;
use zeroc_scope as scope;

use diag::CompileError;

const USAGE: &str = "\
Zero compiler (zero)
Compiles Zero source code to native executables, Rust source, or WebAssembly.

USAGE:
    zero <input.zero> [-rust | -wasm] [-o1 | -o2 | -o3] [-o <output>]

MODES:
    (default)   Build a native executable for the current platform
    -rust       Only generate Rust source code (stdout, or -o <file.rs>)
    -wasm       Generate Rust and compile it to WebAssembly (.wasm)

OPTIONS:
    -o1           No optimization (fastest build)
    -o2           High value optimization (default)
    -o3           Full optimization (LTO; slower build, smallest & fastest)
    -o <output>   Output path: executable (default), .rs (-rust), .wasm (-wasm)
    -h, --help    Show this help

EXAMPLES:
    zero hello.zero                       # hello.exe / hello
    zero hello.zero -rust -o hello.rs
    zero hello.zero -wasm -o hello.wasm
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();

    if args.len() == 1 || args[1] == "-h" || args[1] == "--help" {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }

    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut mode_rust = false;
    let mut mode_wasm = false;
    // 优化档位：1 = 无，2 = 高性价比（默认），3 = 完全优化。
    let mut opt_level: u8 = 2;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-rust" => mode_rust = true,
            "-wasm" => mode_wasm = true,
            "-o1" => opt_level = 1,
            "-o2" => opt_level = 2,
            "-o3" => opt_level = 3,
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

    if mode_rust && mode_wasm {
        eprintln!("error: choose one of -rust / -wasm (not both)");
        return ExitCode::FAILURE;
    }

    let Some(input) = input else {
        eprintln!("error: missing input file");
        eprint!("{USAGE}");
        return ExitCode::FAILURE;
    };

    match compile(&input) {
        Ok((code, uses_win)) => {
            if mode_rust {
                emit_rust(&code, output.as_deref(), uses_win)
            } else if mode_wasm {
                build_wasm(&input, output.as_deref(), &code, uses_win, opt_level)
            } else {
                build_native(&input, output.as_deref(), &code, uses_win, opt_level)
            }
        }
        Err(err) => {
            eprintln!("{}", render_error(&err));
            ExitCode::FAILURE
        }
    }
}

/// `-rust`: emit Rust source only (stdout, or `-o` file).
fn emit_rust(code: &str, output: Option<&Path>, uses_win: bool) -> ExitCode {
    match output {
        Some(out) => {
            if let Err(e) = std::fs::write(out, code) {
                eprintln!("error: cannot write '{}': {e}", out.display());
                return ExitCode::FAILURE;
            }
            println!("✓ generated {}", out.display());
            if uses_win {
                eprintln!(
                    "note: this program uses `win`; compile it as a Cargo project with Slint"
                );
            }
            ExitCode::SUCCESS
        }
        None => {
            print!("{code}");
            ExitCode::SUCCESS
        }
    }
}

/// rustc 优化参数：1 = 无，2 = 高性价比（默认），3 = 完全（LTO）。
fn rustc_opt_flags(opt: u8) -> Vec<&'static str> {
    match opt {
        1 => Vec::new(),
        3 => vec![
            "-C",
            "opt-level=3",
            "-C",
            "strip=symbols",
            "-C",
            "lto=fat",
            "-C",
            "codegen-units=1",
        ],
        _ => vec!["-C", "opt-level=2", "-C", "strip=symbols"],
    }
}

/// Default mode: build a native executable.
fn build_native(
    input: &Path,
    output: Option<&Path>,
    code: &str,
    uses_win: bool,
    opt: u8,
) -> ExitCode {
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("out");
    let rust_path = PathBuf::from(format!("{stem}.rs"));
    if let Err(e) = std::fs::write(&rust_path, code) {
        eprintln!("error: cannot write '{}': {e}", rust_path.display());
        return ExitCode::FAILURE;
    }
    println!("✓ generated {}", rust_path.display());

    let exe_path = match output {
        Some(p) => p.to_path_buf(),
        None => {
            let ext = exe_suffix();
            PathBuf::from(if ext.is_empty() {
                stem.to_string()
            } else {
                format!("{stem}.{ext}")
            })
        }
    };
    let result = if uses_win {
        build_with_cargo(&rust_path, &exe_path, opt)
    } else {
        build_with_rustc(&rust_path, &exe_path, opt)
    };
    match result {
        Ok(()) => {
            println!("✓ built {}", exe_path.display());
            ExitCode::SUCCESS
        }
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::FAILURE
        }
    }
}

/// `-wasm`: generate Rust and compile it to WebAssembly.
fn build_wasm(
    input: &Path,
    output: Option<&Path>,
    code: &str,
    uses_win: bool,
    opt: u8,
) -> ExitCode {
    if uses_win {
        eprintln!("error: -wasm is not supported for `win` (Slint) programs");
        return ExitCode::FAILURE;
    }
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("out");
    let rust_path = PathBuf::from(format!("{stem}.rs"));
    if let Err(e) = std::fs::write(&rust_path, code) {
        eprintln!("error: cannot write '{}': {e}", rust_path.display());
        return ExitCode::FAILURE;
    }
    println!("✓ generated {}", rust_path.display());

    let wasm_path = match output {
        Some(p) => p.to_path_buf(),
        None => PathBuf::from(format!("{stem}.wasm")),
    };
    let mut cmd = Command::new("rustc");
    cmd.arg(&rust_path)
        .arg("--target")
        .arg("wasm32-unknown-unknown");
    cmd.args(rustc_opt_flags(opt));
    cmd.arg("-o").arg(&wasm_path);
    let status = cmd
        .status()
        .map_err(|e| e.to_string())
        .and_then(|s| Ok(s.success()));
    match status {
        Ok(true) => {
            println!("✓ built {}", wasm_path.display());
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!(
                "error: wasm build failed (hint: `rustup target add wasm32-unknown-unknown`)"
            );
            ExitCode::FAILURE
        }
    }
}

/// Build a program that uses `win` as a Cargo project with the Slint
/// dependency (Material Design via SLINT_STYLE=material). The generated
/// code is copied into `<stem>_app/src/main.rs`; the built executable is
/// copied to `exe_path`.
fn build_with_cargo(rust_path: &Path, exe_path: &Path, opt: u8) -> Result<(), String> {
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
        "[package]\nname = \"{stem}_app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nslint = {{ version = \"1\", features = [\"backend-winit\", \"renderer-femtovg\"] }}\n\n[[bin]]\nname = \"{stem}_app\"\npath = \"src/main.rs\"\n\n[workspace]\n"
    );
    std::fs::write(app_dir.join("Cargo.toml"), cargo_toml).map_err(|e| e.to_string())?;
    // 共享 target 目录：同一目录下的多个窗口程序复用已编译的 Slint。
    std::fs::write(
        app_dir.join(".cargo").join("config.toml"),
        "[build]\ntarget-dir = \"../.zeroc_target\"\n",
    )
    .map_err(|e| e.to_string())?;

    let profile = if opt == 1 { "debug" } else { "release" };
    let mut cmd = Command::new("cargo");
    cmd.current_dir(&app_dir).env("SLINT_STYLE", "material");
    if opt == 1 {
        // -o1：无优化，debug 构建。
        cmd.arg("build");
    } else {
        cmd.arg("build").arg("--release");
        cmd.env("CARGO_PROFILE_RELEASE_STRIP", "symbols");
        if opt == 2 {
            // -o2：高性价比 —— thin LTO。
            cmd.env("CARGO_PROFILE_RELEASE_LTO", "thin");
        } else {
            // -o3：完全优化 —— fat LTO + 单 codegen 单元。
            cmd.env("CARGO_PROFILE_RELEASE_LTO", "fat");
            cmd.env("CARGO_PROFILE_RELEASE_CODEGEN_UNITS", "1");
            cmd.env("CARGO_PROFILE_RELEASE_OPT_LEVEL", "3");
        }
    }
    let status = cmd.status().map_err(|e| format!("cannot run cargo: {e}"))?;
    if !status.success() {
        return Err(format!("cargo failed to build '{}'", rust_path.display()));
    }

    let bin = if exe_suffix().is_empty() {
        format!("{stem}_app")
    } else {
        format!("{stem}_app.{}", exe_suffix())
    };
    let exe = parent.join(".zeroc_target").join(profile).join(&bin);
    std::fs::copy(&exe, exe_path).map_err(|e| format!("cannot copy built exe: {e}"))?;
    Ok(())
}

/// Compile a generated `.rs` file with `rustc`. The first attempt uses the
/// default linker; if that fails (e.g. no MSVC `link.exe`), it retries with
/// the `rust-lld` linker bundled with the Rust toolchain.
fn build_with_rustc(rust_path: &Path, exe_path: &Path, opt: u8) -> Result<(), String> {
    let run = |use_lld: bool| -> Result<bool, String> {
        let mut cmd = Command::new("rustc");
        cmd.arg(rust_path).arg("-o").arg(exe_path);
        // 按优化档位设置参数（-o1 无优化 / -o2 高性价比 / -o3 完全）。
        cmd.args(rustc_opt_flags(opt));
        if use_lld {
            cmd.args(["-C", "linker=rust-lld"]);
        }
        let status = cmd.status().map_err(|e| format!("cannot run rustc: {e}"))?;
        Ok(status.success())
    };

    if run(false)? {
        return Ok(());
    }
    eprintln!("note: rustc failed with the default linker, retrying with the bundled rust-lld...");
    if run(true)? {
        return Ok(());
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

    // int-fast analysis: functions compilable natively to i64.
    let mut all_fns: std::collections::HashMap<String, &ast::Function> =
        std::collections::HashMap::new();
    for f in prog.functions() {
        all_fns.insert(f.name.clone(), f);
    }
    for (_, hp) in &loader.zero_headers {
        for f in hp.functions() {
            all_fns.insert(f.name.clone(), f);
        }
    }
    let int_fast = scope::int_fast::analyze(&all_fns, fns);

    let uses_win = loader.win_imported;
    Ok((
        codegen::generate(&prog, &loader, &file, &int_fast),
        uses_win,
    ))
}

/// Render an error; uses the file on disk for the source snippet.
fn render_error(err: &CompileError) -> String {
    let src = std::fs::read_to_string(&err.file).unwrap_or_default();
    diag::render_error(err, &src)
}
