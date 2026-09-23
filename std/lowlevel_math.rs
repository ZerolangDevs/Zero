// lowlevel_math.rs —— 底层数学运算（Zero 标准库最底层，Rust 实现）
// 仅提供基本运算：加、减、乘、除、取余、取负。
// `math` 头文件（Zero 实现）从这些基本运算构建高级函数。

pub fn zadd(a: ZVal, b: ZVal) -> ZVal {
    a.add(&b)
}

pub fn zsub(a: ZVal, b: ZVal) -> ZVal {
    a.sub(&b)
}

pub fn zmul(a: ZVal, b: ZVal) -> ZVal {
    a.mul(&b)
}

pub fn zdiv(a: ZVal, b: ZVal) -> ZVal {
    a.div(&b)
}

pub fn zrem(a: ZVal, b: ZVal) -> ZVal {
    a.rem(&b)
}

pub fn zneg(a: ZVal) -> ZVal {
    a.neg()
}

/// 平方根（返回 Float；负数返回 NULL）。
pub fn zsqrt(a: ZVal) -> ZVal {
    match a {
        ZVal::Float(f) if f >= 0.0 => ZVal::Float(f.sqrt()),
        ZVal::Int(n) if n >= 0 => ZVal::Float((n as f64).sqrt()),
        _ => ZVal::Nil,
    }
}

/// 向下取整（Float 保留 Float，Int 原样返回）。
pub fn zfloor(a: ZVal) -> ZVal {
    match a {
        ZVal::Float(f) => ZVal::Float(f.floor()),
        ZVal::Int(n) => ZVal::Int(n),
        _ => ZVal::Nil,
    }
}

/// 向上取整（Float 保留 Float，Int 原样返回）。
pub fn zceil(a: ZVal) -> ZVal {
    match a {
        ZVal::Float(f) => ZVal::Float(f.ceil()),
        ZVal::Int(n) => ZVal::Int(n),
        _ => ZVal::Nil,
    }
}

/// 四舍五入（Float 保留 Float，Int 原样返回）。
pub fn zround(a: ZVal) -> ZVal {
    match a {
        ZVal::Float(f) => ZVal::Float(f.round()),
        ZVal::Int(n) => ZVal::Int(n),
        _ => ZVal::Nil,
    }
}
