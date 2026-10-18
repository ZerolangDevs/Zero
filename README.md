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
cargo run --release -- examples/hello_zero.zero
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
- **Operators & expressions** — arithmetic (Int and Float), comparison,
  logical `and`/`or` (short-circuiting), unary `-`/`!`, parentheses, boolean
  literals, and the `NULL` value.
- **Types & conversion** — parameters and return values may be typed
  (`int` / `float` / `string` / `bool`) or left dynamic, and
  `type_to<int>(x)` converts any value explicitly (`type_to<float>`,
  `type_to<string>`, `type_to<bool>` too).
- **`func` keyword** — `func add(a<int>, b<int>) -> int: a + b`.
- **Compound assignment** — `+=` / `-=` / `*=` update an existing variable.
- **Standard library `io`** — `print`, `input_s`, `input`, `set_stream`,
  plus `streaming(from, to)` / `stop_streaming(from, to)` to bridge two
  streams (`"shell"`, `"net"`, or a file path) in the background. Built on
  the `io -> stream_io -> stream` dependency chain (shell / file / net
  streams).
- **Standard library `math`** — two headers: `lowlevel_math` (raw Rust,
  basic operations `zadd` / `zsub` / `zmul` / `zdiv` / `zrem` / `zneg`) and
  `math` (written in Zero, built on `lowlevel_math`) providing advanced
  functions: `abs`, `sign`, `max`, `min`, `clamp`, `pow`, `factorial`,
  `gcd`, `lcm`, `is_even`, `is_odd`, `digit_sum`, plus float helpers
  `sqrt` / `floor` / `ceil` / `round`.
- **Standard library `strings`** — two headers: `lowlevel_strings` (raw
  Rust: `zlen` / `zupper` / `zlower` / `ztrim` / `zcontains` / ...) and
  `strings` (written in Zero) providing `len`, `upper`, `lower`, `trim`,
  `contains`, `startswith`, `endswith`, `char_at`, `substr`, `replace`,
  `repeat`, `reverse`, `index_of`, `count`, `is_empty`, plus `capitalize`,
  `count_words`, `is_palindrome`, `truncate`, `remove_spaces`.
- **Standard library `sys`** — OS interaction (`import sys`): the
  `path1 +<path> path2` path-join operator, `userPath()` (home directory),
  `formatPath(path)` path normalization, and `taskkill(target)` (by image
  name or PID), built on `sys -> lowlevel_sys`.
- **Standard library `network`** — networking (`import network`), all
  behind the `net.*` namespace: raw TCP `net.connect(host, port)` /
  `net.send(data)` / `net.recv()` / `net.close()`, plus a cross-platform
  HTTP client supporting both `http://` and `https://` (TLS via rustls /
  `ureq`): `net.get(url)`, `net.post(url, data)`, `net.put(url, data)`,
  `net.delete(url)`, `net.download(url, file)`, and the generic
  `net.httpRequest(method, url, data)`. Built on
  `network -> network_stream -> stream`; combine with `io.streaming` to pipe
  data to/from files or the terminal. Programs using `network` are compiled
  as a Cargo project with the `ureq` dependency (Windows / Linux / macOS).
- **Window library `win`** — native GUI (`import win`, cross-platform):
  built on **Slint** with the **Material Design** style. Create a window
  with `name<win> = { x=..., y=..., size=... }`, drive it with
  `win.update()`, and manage controls with `win.add.button/text/input/
  select/selects/switch/slider/checkbox/spinbox/textedit/progress/group(...)`
  (named args `x` / `y` / `size` / `id` / `items` / `onclick`, and
  `into=(var)` for inputs) plus `win.<id>.del()` / `win.<id>.update(...)`.
  `zeroc --build` compiles such programs as a Cargo project with Slint.
- **Control flow (`import control`)** — `if` / `else_if` / `else`,
  `while`, `for x in a..b`, `each x in ...`, `switch`, `try` / `catch`,
  and `break` / `continue` inside loops. Ranges can also be written fluently
  as `for i in from(x).to(y).step(s)` — the calls are order-free, `from`
  defaults to 0 and `step` to 1, and the end is exclusive. Bodies start with
  `:` and accept a `{ ... }` block, a single statement, or an indented block
  with no braces at all.
- **Standard library `sleep`** — `import sleep` provides `sleep(ms)` to
  pause for a given number of milliseconds.

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
| Std library | `io` / `stream` / `math` (growing) |
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
cargo build --release              # build the `zero` compiler
cargo run -- examples\hello.zero   # build a native executable (default -o2)
cargo run -- examples\hello.zero -rust -o hello.rs   # Rust source only
cargo run -- examples\hello.zero -wasm -o hello.wasm # WebAssembly
cargo run -- examples\hello.zero -o3   # full optimization (LTO)
```

- `zero <input.zero>` — default: compile and build a native executable for
  the current platform (`rustc`, or a Cargo project with Slint for `win`
  programs).
- Optimization levels (default `-o2`):
  - `-o1` no optimization (fastest build);
  - `-o2` high value optimization (`opt-level=2` + strip / release + thin
    LTO for `win`);
  - `-o3` full optimization (`opt-level=3` + `lto=fat` + single codegen
    unit).
- `zero <input.zero> -rust [-o out.rs]` — only generate Rust source
  (stdout if no `-o`).
- `zero <input.zero> -wasm [-o out.wasm]` — generate Rust, compile it to
  WebAssembly, and emit an `<out>.js` runtime glue (`rustup target add
  wasm32-unknown-unknown` may be needed). The glue bridges `print` to the
  host console and `net.*` HTTP to `fetch`/XHR, so `network` programs work
  in wasm too. Run with `node out.js` or load `out.wasm` in a browser.
- If the default linker is missing (e.g. no MSVC `link.exe`), it retries
  with the bundled `rust-lld`.
- Windows note: this repo's `.cargo/config.toml` uses Rust's bundled
  `rust-lld` linker so the project builds even without MSVC `link.exe`.

## Project layout

```text
src/
  main.rs            CLI entry + compile pipeline (root package `zeroc`)
crates/              workspace of library crates
  zeroc-diag/        diagnostics (span, error rendering)
  zeroc-lexer/       tokenizer (token, raw lowlevel block scanner)
  zeroc-ast/         syntax tree (expr, stmt)
  zeroc-parser/      recursive-descent parser
  zeroc-scope/       scope analysis + function/type checks
  zeroc-import/      header loading (stdlib, loader)
  zeroc-codegen/     Rust codegen (emit, runtime)
std/
  stream.rs          low-level stream abstraction (shell / file)
  stream_io.rs       IO stream wrapper
  io.rs              user-facing io: print / input_s / input / set_stream
  lowlevel_math.rs   low-level math: basic arithmetic in raw Rust
  math.zh            math library in Zero: advanced functions on lowlevel_math
  lowlevel_strings.rs low-level string ops in raw Rust
  strings.zh         string library in Zero: advanced functions on lowlevel_strings
  lowlevel_sys.rs    low-level OS interaction in raw Rust (path join/kill)
  sys.zh             sys library in Zero: +<path> operator, formatPath, taskkill
  lowlevel_win.rs    low-level window backend (Slint + Material Design, Rust)
  lowlevel_sleep.rs  low-level sleep in raw Rust
  sleep.zh           sleep library in Zero: sleep(ms)
  network_stream.rs  low-level networking: raw TCP + HTTP/HTTPS (ureq/rustls)
  network.zh         network library in Zero: connect / send / recv / close
examples/            runnable examples (hello_zero, func_typed, control_demo, ...)
```

## Roadmap

A module system, incremental compilation, and editor tooling. Nothing here is
scheduled — it is a list of ideas, and contributions are welcome.

## License

[Mozilla Public License 2.0](LICENSE) (MPL-2.0).
