//! Runtime sources injected into generated code.

pub(crate) const ZVAL_RUNTIME: &str = r#"// === Zero dynamic value runtime (compiler-injected) ===
use std::any::Any;

#[derive(Clone, Debug)]
pub enum ZVal {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Nil,
}

/// Numeric equality across Int/Float (`1 == 1.0` is true); anything else
/// compares by variant.
impl PartialEq for ZVal {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ZVal::Int(a), ZVal::Int(b)) => a == b,
            (ZVal::Float(a), ZVal::Float(b)) => a == b,
            (ZVal::Int(a), ZVal::Float(b)) => *a as f64 == *b,
            (ZVal::Float(a), ZVal::Int(b)) => *a == *b as f64,
            (ZVal::Str(a), ZVal::Str(b)) => a == b,
            (ZVal::Bool(a), ZVal::Bool(b)) => a == b,
            (ZVal::Nil, ZVal::Nil) => true,
            _ => false,
        }
    }
}

impl ZVal {
    pub fn to_rust_string(&self) -> String {
        match self {
            ZVal::Int(n) => n.to_string(),
            ZVal::Float(f) => f.to_string(),
            ZVal::Str(s) => s.clone(),
            ZVal::Bool(b) => b.to_string(),
            ZVal::Nil => "nil".to_string(),
        }
    }

    /// Parse one line of input into a value (numbers become Int/Float).
    pub fn from_line(s: &str) -> ZVal {
        let t = s.trim();
        if let Ok(n) = t.parse::<i64>() {
            return ZVal::Int(n);
        }
        if let Ok(f) = t.parse::<f64>() {
            return ZVal::Float(f);
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
            ZVal::Float(f) => *f != 0.0,
            ZVal::Str(s) => !s.is_empty(),
            ZVal::Bool(b) => *b,
            ZVal::Nil => false,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            ZVal::Int(n) => Some(*n),
            _ => None,
        }
    }

    /// Numeric view of a value: `(number, is_float)`. Int is promoted to
    /// f64 so mixed arithmetic works.
    fn as_num(&self) -> Option<(f64, bool)> {
        match self {
            ZVal::Int(n) => Some((*n as f64, false)),
            ZVal::Float(f) => Some((*f, true)),
            _ => None,
        }
    }

    /// `+`: numeric addition (Int stays Int unless a float is involved), or
    /// string concatenation for strings.
    pub fn add(&self, rhs: &ZVal) -> ZVal {
        match (self, rhs) {
            (ZVal::Int(a), ZVal::Int(b)) => ZVal::Int(a + b),
            (ZVal::Float(a), ZVal::Float(b)) => ZVal::Float(a + b),
            (ZVal::Int(a), ZVal::Float(b)) => ZVal::Float(*a as f64 + b),
            (ZVal::Float(a), ZVal::Int(b)) => ZVal::Float(a + *b as f64),
            (ZVal::Str(a), ZVal::Str(b)) => ZVal::Str(format!("{a}{b}")),
            (ZVal::Str(a), other) => ZVal::Str(format!("{a}{}", other.to_rust_string())),
            (other, ZVal::Str(b)) => ZVal::Str(format!("{}{b}", other.to_rust_string())),
            _ => ZVal::Nil,
        }
    }

    /// `-`: numeric only; non-numeric operands yield Nil.
    pub fn sub(&self, rhs: &ZVal) -> ZVal {
        match (self.as_num(), rhs.as_num()) {
            (Some((a, af)), Some((b, bf))) => {
                if !af && !bf {
                    ZVal::Int(a as i64 - b as i64)
                } else {
                    ZVal::Float(a - b)
                }
            }
            _ => ZVal::Nil,
        }
    }

    /// `*`: numeric only; non-numeric operands yield Nil.
    pub fn mul(&self, rhs: &ZVal) -> ZVal {
        match (self.as_num(), rhs.as_num()) {
            (Some((a, af)), Some((b, bf))) => {
                if !af && !bf {
                    ZVal::Int(a as i64 * b as i64)
                } else {
                    ZVal::Float(a * b)
                }
            }
            _ => ZVal::Nil,
        }
    }

    /// `/`: integer division for two Ints, float division otherwise.
    /// Division by zero yields Nil.
    pub fn div(&self, rhs: &ZVal) -> ZVal {
        match (self.as_num(), rhs.as_num()) {
            (Some((_, _)), Some((b, _))) if b == 0.0 => ZVal::Nil,
            (Some((a, af)), Some((b, bf))) => {
                if !af && !bf {
                    ZVal::Int(a as i64 / b as i64)
                } else {
                    ZVal::Float(a / b)
                }
            }
            _ => ZVal::Nil,
        }
    }

    /// `%`: remainder; integer for two Ints, float otherwise. Zero divisor
    /// yields Nil.
    pub fn rem(&self, rhs: &ZVal) -> ZVal {
        match (self.as_num(), rhs.as_num()) {
            (Some((_, _)), Some((b, _))) if b == 0.0 => ZVal::Nil,
            (Some((a, af)), Some((b, bf))) => {
                if !af && !bf {
                    ZVal::Int(a as i64 % b as i64)
                } else {
                    ZVal::Float(a % b)
                }
            }
            _ => ZVal::Nil,
        }
    }

    /// Unary `-`.
    pub fn neg(&self) -> ZVal {
        match self {
            ZVal::Int(n) => ZVal::Int(-n),
            ZVal::Float(f) => ZVal::Float(-f),
            _ => ZVal::Nil,
        }
    }

    fn cmp_val(&self, rhs: &ZVal) -> Option<std::cmp::Ordering> {
        match (self, rhs) {
            (ZVal::Int(a), ZVal::Int(b)) => Some(a.cmp(b)),
            (ZVal::Float(a), ZVal::Float(b)) => a.partial_cmp(b),
            (ZVal::Int(a), ZVal::Float(b)) => (*a as f64).partial_cmp(b),
            (ZVal::Float(a), ZVal::Int(b)) => a.partial_cmp(&(*b as f64)),
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
    Float,
    Str,
    Bool,
}

/// Verify a value against a declared type; on mismatch print an error and
/// exit. Returns the value unchanged when it matches.
pub fn __zero_check_type(v: &ZVal, ty: ZValType) -> ZVal {
    let ok = match ty {
        ZValType::Int => matches!(v, ZVal::Int(_)),
        ZValType::Float => matches!(v, ZVal::Float(_)),
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

// --- type_to<T> conversions ---

/// Convert any value to an integer (`type_to<int>(x)`).
pub fn __zero_to_int(v: &ZVal) -> ZVal {
    match v {
        ZVal::Int(n) => ZVal::Int(*n),
        ZVal::Float(f) => ZVal::Int(*f as i64),
        ZVal::Str(s) => match s.trim().parse::<i64>() {
            Ok(n) => ZVal::Int(n),
            Err(_) => ZVal::Nil,
        },
        ZVal::Bool(b) => ZVal::Int(if *b { 1 } else { 0 }),
        ZVal::Nil => ZVal::Nil,
    }
}

/// Convert any value to a float (`type_to<float>(x)`).
pub fn __zero_to_float(v: &ZVal) -> ZVal {
    match v {
        ZVal::Int(n) => ZVal::Float(*n as f64),
        ZVal::Float(f) => ZVal::Float(*f),
        ZVal::Str(s) => match s.trim().parse::<f64>() {
            Ok(f) => ZVal::Float(f),
            Err(_) => ZVal::Nil,
        },
        ZVal::Bool(b) => ZVal::Float(if *b { 1.0 } else { 0.0 }),
        ZVal::Nil => ZVal::Nil,
    }
}

/// Convert any value to its string form (`type_to<string>(x)`).
pub fn __zero_to_str(v: &ZVal) -> ZVal {
    ZVal::Str(v.to_rust_string())
}

/// Convert any value to a boolean (`type_to<bool>(x)`).
pub fn __zero_to_bool(v: &ZVal) -> ZVal {
    ZVal::Bool(v.to_bool())
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
