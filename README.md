# Zero

[English](README.md) | [中文](README.zh-CN.md)

A modern programming language for people who need to program easily and quickly

> **⚠️ WARNING — EXTREMELY EARLY STAGE**
>
> Zero is in very early development. The language, the compiler and the
> generated code are **unstable and should not be considered safe or
> production-ready**. Expect breaking changes at any time, and do not rely
> on it for anything important yet.

## Hello, Zero

```zero
// examples/hello_zero.zero
import io

func greetUser(name<string>): {
    print("Hello, {}!\n", name)
}

// No `main`: top-level statements are the entry point.
version = "0.3"
greetUser("Zero")
print("Zero v{}\n", version)
print("1 + 2 * 3 = {}\n", 1 + 2 * 3)
```

```bash
cargo run --release -- examples/hello_zero.zero --build
./hello_zero                   # Windows: hello_zero.exe
```

Output:

```
Hello, Zero!
Zero v0.3
1 + 2 * 3 = 7
```

## What it is

Zero is a small, dynamic, **untyped-by-default** language that compiles to
Rust source code, which `rustc` then turns into a native binary. It is a
"low-level framework" for a language: you get the ease of a scripting
language most of the time, and the ability to dive down into raw Rust or
the OS whenever you need it.

- **No `main` function** — top-level statements are the program entry
  point (Python-style scripts).
- **Dynamic variables** — `name = value` creates or overwrites a variable;
  no type annotations required (`name = value<const>` makes it immutable).
  Assignment updates the nearest existing binding across scopes (so loops
  can accumulate), creating a new variable only when none exists.
- **Scope mechanism** — default `highlevel` scope, explicit `scope highlevel
  { ... }` / `scope lowlevel { ... }` blocks, and raw Rust via `call_rust`.
- **Three built-in low-level hooks**:
  - `call_rust("...")` — inline raw Rust in a low-level scope
  - `call_sys("command")` — run a system command (shell/file aware)
  - `import header` — include `.zh` (compiled) / `.rs` (raw) headers
- **Operators & expressions** — arithmetic, comparison, logical `and`/`or`
  (short-circuiting), unary `-`/`!`, parentheses, boolean literals.
- **`func` keyword with optional types** — `func add(a<int>, b<int>) -> int:
  a + b`; parameters and return values may be typed (`int` / `string` /
  `bool`) or left dynamic.
- **Standard library `io`** — `print`, `input_s`, `input`, `set_stream`
  built on the `io -> stream_io -> stream` dependency chain (shell / file
  streams).
- **Control flow (`import control`)** — `if` / `else_if` / `else`,
  `while`, `for x in a..b`, `each x in ...`, `switch` and `try` / `catch`.
  Bodies start with `:` and accept a `{ ... }` block, a single statement,
  or an indented block with no braces at all.

## Control flow

```zero
// examples/control_demo.zero
import io
import control

sum = 0
for i in 1..5:                  // Python-style: ':' + indentation
    sum = sum + i
print("sum 1..5 = {}\n", sum)

if sum > 10: print("big")       // single statement
else: print("small")

while sum > 0:
    sum = sum - 1
    print("sum = {}\n", sum)

each ch in "Zero":              // iterate over a string's characters
    print("char: {}\n", ch)

switch sum:
    case 0: print("zero\n")
    default: print("other\n")

try:                            // catch Rust panics
    call_rust("panic!(\"boom\");")
catch e:
    print("caught: {}\n", e)
```

## Contributing

All contributions are welcome. Please follow these conventions.

**Code style**

- Zero code (`*.zero`, `*.zh`) uses **camelCase** for variable and function
  names (`greetUser`, `userName`). The compiler is *case-insensitive* about
  this: it simply passes identifiers through to the generated Rust code.
- Rust code (the compiler itself, `std/` headers) follows `rustfmt` and
  standard Rust conventions (`snake_case`).
- Keep changes small, focused, and covered by unit tests.

**Commit messages**

Use the conventional format:

```
<type>(<scope>): <subject>
```

Examples:

```
feat(parser): add string interpolation
fix(codegen): keep short-circuit in and/or
docs(readme): rewrite landing section
test(scope): cover const shadowing
refactor(import): split stdlib loader
```

- `<type>`: `feat`, `fix`, `docs`, `test`, `refactor`, `perf`, `chore`, ...
- `<scope>`: the module you touched (`lexer`, `parser`, `scope`, `import`,
  `codegen`, `io`, `readme`, ...).
- `<subject>`: imperative mood, lowercase, no trailing period.

**Before opening a PR**

- `cargo fmt` and `cargo test` must pass.
- Add or update unit tests for your change.
- Update `examples/` and this README when the language surface changes.

## Comparison with other languages

Zero is not trying to replace anything — it is a bridge between the ease of
dynamic languages and the power of Rust.

| Aspect | Zero | Python | JavaScript | Rust | C |
| --- | --- | --- | --- | --- | --- |
| Typing | dynamic, optional annotations | dynamic | dynamic | static, strong | static, weak |
| Execution | compiled to Rust, then native | interpreted | interpreted / JIT | compiled native | compiled native |
| Memory safety | inherited from Rust (safe) | GC | GC | ownership / borrowing | manual |
| Low-level access | `call_rust` / `call_sys` / `lowlevel` | `ctypes` / `cffi` | FFI / WASM | first-class | first-class |
| Std library | `io` / `stream` (growing) | huge | huge | std + crates | libc |
| Learning curve | low | low | low–medium | high | medium–high |
| Best for | teaching, scripts, quick native tools | data / glue / general | web / UI | performance / systems | embedded / low-level |

Design trade-offs:

- Zero gives up Rust's static type safety for simplicity, but keeps an
  escape hatch: any function can `call_rust(...)` or open a `lowlevel`
  scope and drop down to real Rust, with full access to native performance
  and memory-safety guarantees.
- It is not a general-purpose language yet. It exists to explore a
  *low-level framework* for a language: scopes, headers, dynamic values,
  and a tiny standard library — enough to grow into something bigger.

## Build & usage

```bash
cargo build --release              # build the `zeroc` compiler
cargo run -- examples\hello.zero -o out.rs
rustc out.rs -o out                # then run ./out
```

- `zeroc <input.zero> [-o <output.rs>]` — without `-o`, generated Rust goes
  to stdout.
- `zeroc <input.zero> --build` — generate Rust, then compile it into an
  executable next to the `.rs` file by invoking `rustc` automatically. If
  the default linker is missing (e.g. no MSVC `link.exe`), it retries with
  the `rust-lld` linker bundled with the Rust toolchain.
- Windows note: this repo's `.cargo/config.toml` uses Rust's bundled
  `rust-lld` linker so the project builds even without MSVC `link.exe`.

## Project layout

```text
src/
  main.rs            CLI entry + compile pipeline
  diag/              diagnostics (span, error rendering)
  lexer/             tokenizer (token, raw lowlevel block scanner)
  ast/               syntax tree (expr, stmt)
  parser/            recursive-descent parser
  scope/             scope analysis + function/type checks
  import/            header loading (stdlib, loader)
  codegen/           Rust codegen (emit, runtime)
std/
  stream.rs          low-level stream abstraction (shell / file)
  stream_io.rs       IO stream wrapper
  io.rs              user-facing io: print / input_s / input / set_stream
examples/            runnable examples (hello_zero, func_typed, control_demo, ...)
```

## Roadmap

String & math stdlib modules, a module system, incremental compilation. Nothing here is scheduled — it is a list
of ideas, and contributions are welcome.

## License

[Mozilla Public License 2.0](LICENSE) (MPL-2.0).
