# Zero

一个用 **Rust** 编写的编程语言底层框架（v0.3）。Zero 是一门"默认待在高级作用域、随时可以下潜到 Rust 底层"的动态类型语言：编译器把 Zero 源码翻译成 Rust 代码，再由 `rustc` 编译为可执行文件。

> A modren progamming language for people what need to progame easily and quickly

## 特性一览

- **编译器用 Rust 实现**：词法 → 语法 → 作用域 → 代码生成，全链路零第三方依赖。
- **作用域机制**：默认在 `highlevel` 作用域；`scope highlevel { ... }` 嵌套作用域（变量遮蔽/可见性）；`scope lowlevel { ... }` 直接书写原始 Rust。
- **动态变量，无类型机制**：`name = 值` 创建或赋值，`name` 读取。任意类型，无需类型标注；`name = 值<const>` 声明为不可变变量。
- **运算符与表达式求值**：算术、比较、逻辑（`and`/`or` 短路）、一元 `-`/`!`、布尔字面量、括号与优先级。
- **`func` 关键字 + 可选类型标注**：`func f(x<int>, y) -> string: code`；参数/返回类型（`int`/`string`/`bool`/`any`）编译期查字面量、运行时校验；冒号函数体支持单表达式（隐式返回）或 `{ ... }` 块。
- **函数参数与返回值**：`fn f(a, b) { return a }`，参数/返回值都是动态值。
- **标准库头文件（依赖链）**：`io` → `stream_io` → `stream`。
- **生成 Rust 代码**：输出自包含 `.rs` 文件，内置动态值运行时（`ZVal`）。

## 内置底层能力

| 函数 | 说明 |
| --- | --- |
| `call_rust("rust代码")` | 在低层作用域内联原始 Rust（可访问外层变量） |
| `call_sys("命令")` | 调用系统命令（Windows `cmd /C`，Unix `sh -c`），返回退出码 |
| `import 头文件` | 导入头文件：`.zh`/`.zero` 递归编译，`.rs` 原样内联，标准库头文件自动解析依赖 |

## 标准库 io

`io` 头文件处理输入输出，依赖 `stream_io`，`stream_io` 依赖 `stream`。

- `stream`：底层流抽象（shell 终端 / file 文件）。
- `stream_io`：封装一条"适用于 IO 的流"，提供格式化写入、读行、切换流。
- `io`：面向用户的四个函数：

| 函数 | 说明 |
| --- | --- |
| `print("xxx{}", 值...)` | 格式化输出到当前流，`{}` 依次被值替换 |
| `input_s()` | 从当前流读一行，返回读入的值（数字自动转整数） |
| `input("xxx{}", 值...)` | 打印提示后读入，返回读入的值 |
| `set_stream(type, 文件名可选)` | 切换流：`"shell"` 或 `"file"`，默认 shell |

```zero
// io_demo.zero
import io

fn main() {
    print("What is your name? ")
    name = input_s()
    print("Hello, {}!\n", name)

    age = input("How old are you? ")
    print("You are {} years old.\n", age)

    set_stream("file", "out.txt")
    print("written to file: {}\n", 42)
    set_stream("shell")
    print("back to shell\n")
}
```

## 语言速览

```zero
// 动态变量 + 函数参数/返回值 + 不可变变量
import io

fn describe(name, n) {
    print("{} is {} years old.\n", name, n)
    return name
}

fn main() {
    result = describe("Zero", 18)
    print("describe returned: {}\n", result)

    greeting = "Hello, Zero!"<const>  // 不可变变量
    print("{}\n", greeting)
    // greeting = "no"                // 编译错误：不能给 const 赋值

    count = 10                        // 默认可变
    count = 11                        // 可以重新赋值
    print("count = {}\n", count)
}
```

### 运算符与表达式求值

```zero
import io

fn main() {
    a = 10
    b = 3

    print("a + b = {}\n", a + b)        // 算术：13
    print("a / b = {}\n", a / b)        // 整除：3
    print("a % b = {}\n", a % b)        // 取余：1
    print("-a = {}\n", -a)              // 一元负号：-10

    print("a >= b: {}\n", a >= b)       // 比较 → Bool：true
    print("a and b: {}\n", a and b)   // 逻辑（非 0 即真）：true
    print("!a: {}\n", !a)               // 逻辑非：false

    greeting = "Hello, " + "Zero" + "!" // 字符串拼接
    print("{}\n", greeting)

    x = 1 + 2 * 3                        // 优先级：7
    print("(1 + 2) * 3 = {}\n", (1 + 2) * 3)  // 括号：9
}
```

- 算术只对整数有效（字符串仅 `+` 拼接）；非数字参与算术得 `nil`。
- 比较返回 `Bool`；不同类型比较得 `false`（`==`/`!=` 按值比较）。
- `and` / `or` 短路求值（右侧副作用不会被触发，当左侧已定值时）。
- `true` / `false` 为布尔字面量。

### func 关键字与类型标注

```zero
import io

// 单表达式体：冒号后直接是表达式，自动返回
func add(a<int>, b<int>) -> int: a + b

// 块体：冒号后是 { ... }，支持多语句和显式 return
func describe(name<string>, age<int>) -> string: {
    print("{} is {} years old\n", name, age)
    return name + " (" + age + ")"
}

fn main(): {          // main 也可以用 func 定义（fn 保留兼容）
    x = add(3, 4)
    print("add(3, 4) = {}\n", x)
    msg = describe("Zero", 18)
    print("describe returned: {}\n", msg)
}
```

- 参数类型标注**可选**：`x<int>`、`y`（动态）。返回类型可选：`-> int`。
- 类型：`int` / `string` / `bool`；显式写 `any`（如 `x<any>`、`-> any`）等同不标注（动态），不会生成运行时校验。
- **编译期**：字面量参数/返回值与标注类型不匹配 → 报错；变量参数在**运行时**校验（不匹配打印 `type error` 并退出）。
- 类型：`int` / `string` / `bool` / `any`（any 即不校验）。
- `return expr` / `return;`：显式返回值（块体里使用）；`-> type` 标注时返回值同样做类型校验。
- `fn` 旧语法（无冒号、块体）完全保留，参数类型标注对 `fn` 同样可用。

### 作用域机制

```zero
// scope_demo.zero —— 默认 highlevel 作用域
import stdio

fn main() {
    name = "Zero"

    // call_rust 处于隐式 lowlevel 作用域，可访问外层变量
    call_rust("println!(\"highlevel var: {}\", name);")

    // highlevel 嵌套作用域：内部遮蔽，退出后恢复
    scope highlevel {
        name = "inner Zero"
        call_rust("println!(\"inner scope var: {}\", name);")
    }

    call_rust("println!(\"back to outer var: {}\", name);")

    // 显式 lowlevel 作用域：直接写 Rust，作用域限定在本块内
    scope lowlevel {
        let x = 40;
        let y = 2;
        println!("lowlevel scope: x + y = {}", x + y);
    }
}
```

### 头文件导入

```zero
// main.zero
import stdio
import "headers/helpers"   // .zh 头文件：递归编译，函数可直接调用
import "headers/extra.rs"  // .rs 头文件：原样内联

fn main() {
    greet()        // 来自 helpers.zh
    rust_helper()  // 来自 extra.rs
}
```

内置头文件别名：`stdio`、`fs`、`path`、`process`、`env`、`time`、`collections`、`string`。
标准库头文件：`io`、`stream_io`、`stream`（内嵌在编译器里，自动注入依赖链）。

## 快速开始

```bash
cargo build --release              # 构建编译器 zeroc
cargo run -- examples\io_demo.zero -o out\io_demo.rs
rustc -C linker=rust-lld out\io_demo.rs -o out\io_demo.exe   # 本机无 MSVC link.exe 时
echo Ali 21 | out\io_demo.exe
```

如果机器上装有完整的 Visual Studio（含 MSVC 链接器），最后两步直接：

```bash
rustc out\io_demo.rs -o out\io_demo
echo Ali 21 | ./io_demo
```

> Windows 说明：本仓库的 `.cargo/config.toml` 把链接器设为 Rust 自带 `rust-lld`，以便在没有 MSVC `link.exe` 的环境也能构建；在有 MSVC 的机器上同样可用，也可自行删除该配置。

## 语言规范（v0.3）

```text
program    := item*
item       := import | function
import     := "import" (IDENT ("." IDENT | INT)* | STRING) [";"]
function   := ("fn" | "func") IDENT "(" [param ("," param)*] ")" ["->" type] func_body
param      := IDENT ["<" type ">"]
type       := "int" | "string" | "bool" | "any"
func_body  := block                              // fn 或 func 的块体
            | ":" block                          // func：冒号 + 块
            | ":" expr                           // func：冒号 + 单表达式（隐式返回）
block      := "{" stmt* "}"
stmt       := assign | return | scope | expr [终止符]
assign     := IDENT "=" expr
return     := "return" [expr]
scope      := "scope" ("highlevel" | "lowlevel") block
expr       := unary (binop unary)*
unary      := ("-" | "!") unary | primary
primary    := INT | STRING | "true" | "false" | IDENT | IDENT "(" [expr ("," expr)*] ")" | "(" expr ")"
binop      := "+" | "-" | "*" | "/" | "%"
            | "==" | "!=" | "<" | ">" | "<=" | ">=" | "and" | "or"
```

运算符优先级（低 → 高）：`or` < `and` < `==` `!=` < `<` `>` `<=` `>=` < `+` `-` < `*` `/` `%` < 一元 `-` `!` < 主表达式；全部左结合，`and`/`or` 短路求值。

- 语句终止符：`;`、换行、或块结束前的 `}`（三者皆可）。
- 注释：`// ...` 与 `/* ... */`。
- 字符串转义：`\"` `\\` `\n` `\r` `\t` `\0`。
- 变量为动态值（`ZVal`）：整数、字符串、布尔、nil；赋值即创建，无类型标注。
- 不可变变量：`name = expr<const>`，之后在**同一作用域**内重新赋值会编译报错；
  默认可变（`name = expr` 可反复赋值）；内层作用域可创建同名新变量（遮蔽），不影响外层 const。

## 项目结构

```text
src/
  main.rs            CLI 入口 + 编译管线（parse → load → analyze → codegen）
  diag/              诊断：span.rs（源码区间）、error.rs（错误类型与渲染）
  lexer/             词法：token.rs（Token 定义）、raw.rs（lowlevel 原始块扫描）、mod.rs（Lexer）
  ast/               AST：expr.rs（表达式）、stmt.rs（语句）、mod.rs（程序/函数/块）
  parser/            语法：mod.rs（递归下降解析）
  scope/             作用域：callable.rs（函数表/内置/io 函数元数据）、mod.rs（ScopeAnalyzer）
  import/            头文件：stdlib.rs（标准库定义与内嵌源码）、mod.rs（Loader）
  codegen/           代码生成：emit.rs（转义/缩进辅助）、runtime.rs（ZVal/__zero_sys 运行时）、mod.rs（Codegen）
std/
  stream.rs     底层流抽象（shell / file）
  stream_io.rs  IO 流封装（格式化写入、读行、切换）
  io.rs         用户 IO 函数（print / input_s / input / set_stream）
examples/
  hello.zero              三个内置底层函数
  scope_demo.zero         作用域机制
  func_demo.zero          动态变量 + 函数参数/返回值
  const_demo.zero         不可变变量 <const>
  ops_demo.zero           运算符与表达式求值
  func_typed.zero        func 关键字 + 类型标注
  io_demo.zero            io 头文件交互演示
  io_file.zero            set_stream 文件流
  headers_demo/           .zh / .rs 头文件导入
  io_header_demo/         头文件内部 import io
```

## 设计原则

遵循 KISS / DRY / YAGNI：只实现当前明确需要的特性；标准库按 `io → stream_io → stream` 单向依赖分层；每个模块单一职责。

## 已知限制（v0.3）

- 算术仅支持整数与字符串 `+` 拼接；其他类型参与算术得 `nil`（不报错）。
- 运行时类型校验失败会打印 `type error` 并退出（显式标注是硬约束）。
- `<const>` 仅对当前作用域内的首次声明有效；不能把已存在的可变变量重新声明为 const。
- `call_rust` 内联代码中引用 Zero 变量时，变量是 `ZVal` 动态值（可用 `.to_rust_string()` 取字符串）。
- 用户函数返回动态值；函数体末尾未显式 `return` 时自动返回 `nil`。
- `.rs` 头文件的函数名通过轻量 `fn` 扫描注册，极端字符串场景可能出现误注册（无害）。
- 变量名不能与 Rust 关键字冲突（如 `type`、`match`）。

## 路线图（未实现，勿提前实现）

类型断言与转换函数、多文件模块系统、`std` 数学/字符串标准库、控制流（`if`/`while`）、增量编译。
