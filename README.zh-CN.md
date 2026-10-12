# Zero

[English](README.md) | [中文](README.zh-CN.md)

一门为需要轻松快速编程的人打造的现代编程语言
(A modern programming language for people who need to program easily and quickly)

> **⚠️ 警告 —— 项目处于极其早期阶段**
>
> Zero 正处于非常早期的开发阶段。语言、编译器以及生成的代码**不稳定，不应被视为安全或可用于生产环境**。任何时候都可能发生破坏性变更，请勿将其用于任何重要的场景。

## Hello, Zero

```zero
// examples/hello_zero.zero
import io

func greetUser(name<string>): {
    print("Hello, {}!\n", name)
}

// 无需 main：顶层语句就是程序入口
version = "0.3"
greetUser("Zero")
print("Zero v{}\n", version)
print("1 + 2 * 3 = {}\n", 1 + 2 * 3)
```

```bash
cargo run --release -- examples/hello_zero.zero --build
./hello_zero                   # Windows：hello_zero.exe
```

输出：

```
Hello, Zero!
Zero v0.3
1 + 2 * 3 = 7
```

## 这是什么

Zero 是一门小型、动态、**默认无类型**的语言，它编译成 Rust 源码，再由 `rustc` 编译为原生可执行文件。它是一个"编程语言的底层框架"：大部分时候你能享受脚本语言的轻松，而在需要时又能随时下潜到原生 Rust 或操作系统层面。

- **无需 `main` 函数** —— 顶层语句就是程序入口（Python 风格脚本）。
- **动态变量** —— `name = value` 创建或覆盖变量，无需类型标注（`name = value<const>` 声明为不可变）。赋值会**更新最近存在的绑定**（跨作用域，因此循环可以累加），只有不存在时才创建新变量。
- **作用域机制** —— 默认处于 `highlevel` 作用域；支持显式的 `scope highlevel { ... }` / `scope lowlevel { ... }` 块，以及通过 `call_rust` 直接写原生 Rust。
- **三个内置底层钩子**：
  - `call_rust("...")` —— 在低层作用域内联一段原生 Rust 代码
  - `call_sys("命令")` —— 调用系统命令（shell / 文件流感知）
  - `import 头文件` —— 导入 `.zh`（递归编译）或 `.rs`（原样内联）头文件
- **运算符与表达式** —— 算术（支持 `int` 与 `float`）、比较、逻辑 `and` / `or`（短路求值）、一元 `-` / `!`、括号、布尔字面量，以及 `NULL` 值。
- **类型与转换** —— 参数和返回值可以标注类型（`int` / `float` / `string` / `bool`），也可以保持动态；`type_to<int>(x)` 可显式转换任意值（同样支持 `type_to<float>`、`type_to<string>`、`type_to<bool>`）。
- **`func` 关键字** —— `func add(a<int>, b<int>) -> int: a + b`。
- **复合赋值** —— `+=` / `-=` / `*=` 更新已存在的变量。
- **标准库 `io`** —— `print`、`input_s`、`input`、`set_stream`，以及 `streaming(from, to)` / `stop_streaming(from, to)` 用于后台桥接两条流（`"shell"`、`"net"` 或文件路径）。构建在 `io -> stream_io -> stream` 依赖链之上（shell / 文件 / 网络流）。
- **标准库 `math`** —— 包含两个头文件：`lowlevel_math`（原生 Rust，基本运算
  `zadd` / `zsub` / `zmul` / `zdiv` / `zrem` / `zneg`）和 `math`（用 Zero 编写，
  构建于 `lowlevel_math` 之上）提供高级函数：`abs`、`sign`、`max`、`min`、
  `clamp`、`pow`、`factorial`、`gcd`、`lcm`、`is_even`、`is_odd`、`digit_sum`，以及浮点辅助函数 `sqrt` / `floor` / `ceil` / `round`。
- **标准库 `strings`** —— 包含两个头文件：`lowlevel_strings`（原生 Rust：`zlen` / `zupper` / `zlower` / `ztrim` / `zcontains` / ……）和 `strings`（用 Zero 编写）提供 `len`、`upper`、`lower`、`trim`、`contains`、`startswith`、`endswith`、`char_at`、`substr`、`replace`、`repeat`、`reverse`、`index_of`、`count`、`is_empty`，以及 `capitalize`、`count_words`、`is_palindrome`、`truncate`、`remove_spaces`。
- **标准库 `sys`** —— 操作系统交互（`import sys`）：`path1 +<path> path2` 路径拼接运算符、`userPath()`（用户根目录）、`formatPath(path)` 路径规范化、`taskkill(target)` 结束进程（按映像名或 PID），构建于 `sys -> lowlevel_sys` 之上。
- **标准库 `network`** —— 网络（`import network`），全部通过 `net.*` 命名空间：裸 TCP `net.connect(host, port)` / `net.send(data)` / `net.recv()` / `net.close()`，以及 HTTP 客户端（仅 `http://`）：`net.get(url)`、`net.post(url, data)`、`net.put(url, data)`、`net.delete(url)`、`net.download(url, file)`、通用 `net.httpRequest(method, url, data)`。构建于 `network -> network_stream -> stream` 之上，可与 `io.streaming` 配合管道数据。
- **窗口库 `win`** —— 原生 GUI（`import win`，跨平台）：基于 **Slint** 并使用 **Material Design** 样式。用 `name<win> = { x=..., y=..., size=... }` 新建窗口，`win.update()` 驱动窗口，控件管理用 `win.add.button/text/input/select/selects/switch/slider/checkbox/spinbox/textedit/progress/group(...)`（命名参数 `x` / `y` / `size` / `id` / `items` / `onclick`，输入框还有 `into=(var)`）以及 `win.<id>.del()` / `win.<id>.update(...)`。`zeroc --build` 会把这类程序作为带 Slint 的 Cargo 工程来编译。
- **控制流（`import control`）** —— `if` / `else_if` / `else`、`while`、`for x in a..b`、`each x in ...`、`switch`、`try` / `catch`，以及循环内的 `break` / `continue`。范围也可以写成流式 `for i in from(x).to(y).step(s)` —— 调用顺序任意，`from` 默认 0、`step` 默认 1，顾头不顾尾。控制体以 `:` 引导，支持 `{ ... }` 块、单语句、或**不用花括号的缩进块**。
- **标准库 `sleep`** —— `import sleep` 提供 `sleep(ms)`，按毫秒暂停。

## 控制流

```zero
// examples/control_demo.zero
import io
import control

sum = 0
for i in 1..5:                  // Python 风格：冒号 + 缩进
    sum = sum + i
print("sum 1..5 = {}\n", sum)

if sum > 10: print("big")       // 单语句
else: print("small")

while sum > 0:
    sum = sum - 1
    print("sum = {}\n", sum)

each ch in "Zero":              // 遍历字符串字符
    print("char: {}\n", ch)

switch sum:
    case 0: print("zero\n")
    default: print("other\n")

try:                            // 捕获 Rust panic
    call_rust("panic!(\"boom\");")
catch e:
    print("caught: {}\n", e)
```

## 贡献规范

欢迎一切贡献，请遵守以下约定。

**代码风格**

- Zero 代码（`*.zero`、`*.zh`）的变量名和函数名使用 **camelCase**（例如 `greetUser`、`userName`）。编译器对此大小写不敏感：它只是把标识符原样透传给生成的 Rust 代码。
- Rust 代码（编译器本身、`std/` 头文件）遵循 `rustfmt` 与标准 Rust 惯例（`snake_case`）。
- 保持改动小而聚焦，并为改动补充单元测试。

**提交信息**

请使用常规提交格式：

```
<type>(<scope>): <subject>
```

示例：

```
feat(parser): add string interpolation
fix(codegen): keep short-circuit in and/or
docs(readme): rewrite landing section
test(scope): cover const shadowing
refactor(import): split stdlib loader
```

- `<type>`：`feat`（新功能）、`fix`（修复）、`docs`（文档）、`test`（测试）、`refactor`（重构）、`perf`（性能）、`chore`（杂务）……
- `<scope>`：你所改动的模块（`lexer`、`parser`、`scope`、`import`、`codegen`、`io`、`readme`……）。
- `<subject>`：祈使句、小写、结尾不加句号。

**发起 PR 之前**

- `cargo fmt` 与 `cargo test` 必须通过。
- 为你的改动添加或更新单元测试。
- 当语言表层发生变更时，同步更新 `examples/` 与本 README。

## 与其他语言的对比

Zero 无意取代任何语言，它是一座桥梁：连接动态语言的轻松与 Rust 的强大。

| 方面 | Zero | Python | JavaScript | Rust | C |
| --- | --- | --- | --- | --- | --- |
| 类型系统 | 动态，可选标注 | 动态 | 动态 | 静态强类型 | 静态弱类型 |
| 执行方式 | 编译为 Rust，再原生编译 | 解释执行 | 解释 / JIT | 原生编译 | 原生编译 |
| 内存安全 | 继承 Rust（安全） | GC | GC | 所有权 / 借用 | 手动管理 |
| 底层访问 | `call_rust` / `call_sys` / `lowlevel` | `ctypes` / `cffi` | FFI / WASM | 一等公民 | 一等公民 |
| 标准库 | `io` / `stream`（成长中） | 庞大 | 庞大 | std + crates | libc |
| 学习曲线 | 低 | 低 | 低–中 | 高 | 中–高 |
| 最适合 | 教学、脚本、快速原生工具 | 数据 / 胶水 / 通用 | Web / UI | 性能 / 系统 | 嵌入式 / 底层 |

设计取舍：

- Zero 以简单性换取了 Rust 的静态类型安全，但保留了逃生通道：任何函数都可以调用 `call_rust(...)` 或打开 `lowlevel` 作用域，下潜到真正的 Rust，完整使用原生的性能与内存安全保证。
- 它目前还不是一门通用语言。它的存在是为了探索"语言的底层框架"：作用域、头文件、动态值，以及一个足够小的标准库——足以成长为一个更大的东西。

## 构建与使用

```bash
cargo build --release              # 构建 zeroc 编译器
cargo run -- examples\hello.zero -o out.rs
rustc out.rs -o out                # 然后运行 ./out
```

- `zeroc <input.zero> [-o <output.rs>]` —— 不带 `-o` 时，生成的 Rust 代码输出到标准输出。
- `zeroc <input.zero> --build` —— 生成 Rust 代码后自动调用 `rustc` 编译为可执行文件（与 `.rs` 文件同目录）。如果默认链接器缺失（例如没有 MSVC `link.exe`），会自动改用 Rust 工具链自带的 `rust-lld` 链接器重试。
- Windows 说明：本仓库的 `.cargo/config.toml` 使用 Rust 自带的 `rust-lld` 链接器，因此即使没有 MSVC `link.exe` 也能构建。

## 项目结构

```text
src/
  main.rs            命令行入口 + 编译管线
  diag/              诊断（源码区间、错误渲染）
  lexer/             词法分析（token、lowlevel 原始块扫描）
  ast/               语法树（表达式、语句）
  parser/            递归下降解析器
  scope/             作用域分析 + 函数 / 类型检查
  import/            头文件加载（标准库、加载器）
  codegen/           Rust 代码生成（输出辅助、运行时）
std/
  stream.rs          底层流抽象（shell / 文件）
  stream_io.rs       IO 流封装
  io.rs              面向用户的 io：print / input_s / input / set_stream
  lowlevel_math.rs   底层数学：原生 Rust 基本运算
  math.zh            数学库（Zero 实现）：基于 lowlevel_math 的高级函数
  lowlevel_strings.rs 底层字符串操作：原生 Rust
  strings.zh         字符串库（Zero 实现）：基于 lowlevel_strings 的高级函数
  lowlevel_sys.rs    底层系统交互：原生 Rust（路径拼接/结束进程）
  sys.zh             系统库（Zero 实现）：+<path> 运算符、formatPath、taskkill
  lowlevel_win.rs    底层窗口后端（Slint + Material Design，Rust）
  lowlevel_sleep.rs  底层睡眠：原生 Rust
  sleep.zh           睡眠库（Zero 实现）：sleep(ms)
  network_stream.rs  底层 TCP 网络：原生 Rust
  network.zh         网络库（Zero 实现）：connect / send / recv / close
examples/            可运行示例（hello_zero、func_typed、control_demo……）
```

## 路线图

模块系统、增量编译、编辑器工具链。以上都还没有排期——它们只是一些想法清单，欢迎贡献。

## 许可证

[Mozilla Public License 2.0](LICENSE)（MPL-2.0）。
