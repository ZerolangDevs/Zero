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

    /// Truthiness for `&&`, `||`, `!` and conditions.
    pub fn to_bool(&self) -> bool {
        match self {
            ZVal::Int(n) => *n != 0,
            ZVal::Str(s) => !s.is_empty(),
            ZVal::Bool(b) => *b,
            ZVal::Nil => false,
        }
    }

    fn as_int(&self) -> Option<i64> {
        match self {
            ZVal::Int(n) => Some(*n),
            _ => None,
        }
    }

    /// `+`: numeric addition, or string concatenation for strings.
    pub fn add(&self, rhs: &ZVal) -> ZVal {
        match (self, rhs) {
            (ZVal::Int(a), ZVal::Int(b)) => ZVal::Int(a + b),
            (ZVal::Str(a), ZVal::Str(b)) => ZVal::Str(format!("{a}{b}")),
            (ZVal::Str(a), other) => ZVal::Str(format!("{a}{}", other.to_rust_string())),
            (other, ZVal::Str(b)) => ZVal::Str(format!("{}{b}", other.to_rust_string())),
            _ => ZVal::Nil,
        }
    }

    /// `-`, `*`, `/`, `%`: numeric only; non-numeric operands yield Nil.
    pub fn sub(&self, rhs: &ZVal) -> ZVal {
        match (self.as_int(), rhs.as_int()) {
            (Some(a), Some(b)) => ZVal::Int(a - b),
            _ => ZVal::Nil,
        }
    }

    pub fn mul(&self, rhs: &ZVal) -> ZVal {
        match (self.as_int(), rhs.as_int()) {
            (Some(a), Some(b)) => ZVal::Int(a * b),
            _ => ZVal::Nil,
        }
    }

    pub fn div(&self, rhs: &ZVal) -> ZVal {
        match (self.as_int(), rhs.as_int()) {
            (Some(_), Some(0)) => ZVal::Nil,
            (Some(a), Some(b)) => ZVal::Int(a / b),
            _ => ZVal::Nil,
        }
    }

    pub fn rem(&self, rhs: &ZVal) -> ZVal {
        match (self.as_int(), rhs.as_int()) {
            (Some(_), Some(0)) => ZVal::Nil,
            (Some(a), Some(b)) => ZVal::Int(a % b),
            _ => ZVal::Nil,
        }
    }

    /// Unary `-`.
    pub fn neg(&self) -> ZVal {
        match self.as_int() {
            Some(n) => ZVal::Int(-n),
            None => ZVal::Nil,
        }
    }

    fn cmp_val(&self, rhs: &ZVal) -> Option<std::cmp::Ordering> {
        match (self, rhs) {
            (ZVal::Int(a), ZVal::Int(b)) => Some(a.cmp(b)),
            (ZVal::Str(a), ZVal::Str(b)) => Some(a.cmp(b)),
            _ => None,
        }
    }

    /// Ordered comparisons return `ZVal::Bool(false)` for mixed/invalid types.
    pub fn lt(&self, rhs: &ZVal) -> ZVal {
        ZVal::Bool(self.cmp_val(rhs) == Some(std::cmp::Ordering::Less))
    }

    pub fn le(&self, rhs: &ZVal) -> ZVal {
        ZVal::Bool(self.cmp_val(rhs).map_or(false, |o| o != std::cmp::Ordering::Greater))
    }

    pub fn gt(&self, rhs: &ZVal) -> ZVal {
        ZVal::Bool(self.cmp_val(rhs) == Some(std::cmp::Ordering::Greater))
    }

    pub fn ge(&self, rhs: &ZVal) -> ZVal {
        ZVal::Bool(self.cmp_val(rhs).map_or(false, |o| o != std::cmp::Ordering::Less))
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

/// Declared Zero types, used by runtime type checks.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ZValType {
    Int,
    Str,
    Bool,
}

/// Verify a value against a declared type; on mismatch print an error and
/// exit. Returns the value unchanged when it matches.
pub fn __zero_check_type(v: &ZVal, ty: ZValType) -> ZVal {
    let ok = match ty {
        ZValType::Int => matches!(v, ZVal::Int(_)),
        ZValType::Str => matches!(v, ZVal::Str(_)),
        ZValType::Bool => matches!(v, ZVal::Bool(_)),
    };
    if ok {
        v.clone()
    } else {
        eprintln!("type error: expected {ty:?}, got {:?}", v);
        std::process::exit(1);
    }
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
