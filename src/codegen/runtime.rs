//! Runtime sources injected into generated code.

pub(crate) const ZVAL_RUNTIME: &str = r#"// === Zero dynamic value runtime (compiler-injected) ===
#[derive(Clone, Debug, PartialEq)]
pub enum ZVal {
    Int(i64),
    Str(String),
    Bool(bool),
    Nil,
}

impl ZVal {
    pub fn to_rust_string(&self) -> String {
        match self {
            ZVal::Int(n) => n.to_string(),
            ZVal::Str(s) => s.clone(),
            ZVal::Bool(b) => b.to_string(),
            ZVal::Nil => "nil".to_string(),
        }
    }

    /// Parse one line of input into a value (numbers become Int).
    pub fn from_line(s: &str) -> ZVal {
        let t = s.trim();
        if let Ok(n) = t.parse::<i64>() {
            return ZVal::Int(n);
        }
        match t {
            "true" => return ZVal::Bool(true),
            "false" => return ZVal::Bool(false),
            _ => {}
        }
        ZVal::Str(s.to_string())
    }
}

impl std::fmt::Display for ZVal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_rust_string())
    }
}

impl std::process::Termination for ZVal {
    fn report(self) -> std::process::ExitCode {
        std::process::ExitCode::SUCCESS
    }
}

/// Replace `{}` placeholders with the string form of each argument.
pub fn __zero_format(fmt: &str, args: &[ZVal]) -> String {
    let mut out = String::new();
    let mut rest = fmt;
    let mut iter = args.iter();
    while let Some(pos) = rest.find("{}") {
        out.push_str(&rest[..pos]);
        match iter.next() {
            Some(v) => out.push_str(&v.to_rust_string()),
            None => out.push_str("{}"),
        }
        rest = &rest[pos + 2..];
    }
    out.push_str(rest);
    out
}
"#;

/// Runtime support: `call_sys` bridges to the platform shell.
pub(crate) fn preamble() -> String {
    "// === Zero runtime helpers ===\n\
     #[cfg(windows)]\n\
     fn __zero_sys(command: &str) -> i32 {\n\
     \x20   let status = std::process::Command::new(\"cmd\")\n\
     \x20       .args([\"/C\", command])\n\
     \x20       .status()\n\
     \x20       .expect(\"__zero_sys: failed to run system command\");\n\
     \x20   status.code().unwrap_or(-1)\n\
     }\n\
     \n\
     #[cfg(not(windows))]\n\
     fn __zero_sys(command: &str) -> i32 {\n\
     \x20   let status = std::process::Command::new(\"sh\")\n\
     \x20       .args([\"-c\", command])\n\
     \x20       .status()\n\
     \x20       .expect(\"__zero_sys: failed to run system command\");\n\
     \x20   status.code().unwrap_or(-1)\n\
     }"
    .to_string()
}
