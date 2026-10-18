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

// ---- 原生 i64 变体（供编译器 int 快速路径调用）----
#[inline]
pub fn zadd_i64(a: i64, b: i64) -> i64 {
    a.wrapping_add(b)
}
#[inline]
pub fn zsub_i64(a: i64, b: i64) -> i64 {
    a.wrapping_sub(b)
}
#[inline]
pub fn zmul_i64(a: i64, b: i64) -> i64 {
    a.wrapping_mul(b)
}
#[inline]
pub fn zdiv_i64(a: i64, b: i64) -> i64 {
    if b == 0 {
        0
    } else {
        a.wrapping_div(b)
    }
}
#[inline]
pub fn zrem_i64(a: i64, b: i64) -> i64 {
    if b == 0 {
        0
    } else {
        a.wrapping_rem(b)
    }
}
#[inline]
pub fn zneg_i64(a: i64) -> i64 {
    a.wrapping_neg()
}

// ---- 高级函数：ZVal 封装（动态路径）----
#[inline]
pub fn zabs(a: ZVal) -> ZVal {
    match a.as_int() {
        Some(n) => ZVal::Int(n.abs()),
        None => ZVal::Nil,
    }
}
#[inline]
pub fn zabs_i64(n: i64) -> i64 {
    n.abs()
}

#[inline]
pub fn zsign(a: ZVal) -> ZVal {
    match a.as_int() {
        Some(n) => ZVal::Int(if n > 0 { 1 } else if n < 0 { -1 } else { 0 }),
        None => ZVal::Nil,
    }
}
#[inline]
pub fn zsign_i64(n: i64) -> i64 {
    if n > 0 {
        1
    } else if n < 0 {
        -1
    } else {
        0
    }
}

#[inline]
pub fn zmax(a: ZVal, b: ZVal) -> ZVal {
    match (a.as_int(), b.as_int()) {
        (Some(x), Some(y)) => ZVal::Int(x.max(y)),
        _ => ZVal::Nil,
    }
}
#[inline]
pub fn zmax_i64(a: i64, b: i64) -> i64 {
    a.max(b)
}

#[inline]
pub fn zmin(a: ZVal, b: ZVal) -> ZVal {
    match (a.as_int(), b.as_int()) {
        (Some(x), Some(y)) => ZVal::Int(x.min(y)),
        _ => ZVal::Nil,
    }
}
#[inline]
pub fn zmin_i64(a: i64, b: i64) -> i64 {
    a.min(b)
}

#[inline]
pub fn zclamp(a: ZVal, lo: ZVal, hi: ZVal) -> ZVal {
    match (a.as_int(), lo.as_int(), hi.as_int()) {
        (Some(x), Some(l), Some(h)) => ZVal::Int(x.max(l).min(h)),
        _ => ZVal::Nil,
    }
}
#[inline]
pub fn zclamp_i64(x: i64, lo: i64, hi: i64) -> i64 {
    x.max(lo).min(hi)
}

#[inline]
pub fn zpow(a: ZVal, b: ZVal) -> ZVal {
    match (a.as_int(), b.as_int()) {
        (Some(base), Some(exp)) => ZVal::Int(zpow_i64(base, exp)),
        _ => ZVal::Nil,
    }
}
#[inline]
pub fn zpow_i64(base: i64, exp: i64) -> i64 {
    if exp <= 0 {
        1
    } else {
        base.wrapping_pow(exp as u32)
    }
}

#[inline]
pub fn zfactorial(a: ZVal) -> ZVal {
    match a.as_int() {
        Some(n) => ZVal::Int(zfactorial_i64(n)),
        None => ZVal::Nil,
    }
}
#[inline]
pub fn zfactorial_i64(n: i64) -> i64 {
    if n <= 1 {
        1
    } else {
        (2..=n).fold(1i64, |acc, k| acc.wrapping_mul(k))
    }
}

#[inline]
pub fn zgcd(a: ZVal, b: ZVal) -> ZVal {
    match (a.as_int(), b.as_int()) {
        (Some(x), Some(y)) => ZVal::Int(zgcd_i64(x, y)),
        _ => ZVal::Nil,
    }
}
#[inline]
pub fn zgcd_i64(mut a: i64, mut b: i64) -> i64 {
    a = a.abs();
    b = b.abs();
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

#[inline]
pub fn zlcm(a: ZVal, b: ZVal) -> ZVal {
    match (a.as_int(), b.as_int()) {
        (Some(x), Some(y)) => ZVal::Int(zlcm_i64(x, y)),
        _ => ZVal::Nil,
    }
}
#[inline]
pub fn zlcm_i64(a: i64, b: i64) -> i64 {
    if a == 0 || b == 0 {
        0
    } else {
        let d = zgcd_i64(a, b);
        (a / d).wrapping_mul(b).abs()
    }
}

#[inline]
pub fn zis_even(a: ZVal) -> ZVal {
    match a.as_int() {
        Some(n) => ZVal::Bool(zis_even_i64(n) != 0),
        None => ZVal::Nil,
    }
}
#[inline]
pub fn zis_even_i64(n: i64) -> i64 {
    if n % 2 == 0 {
        1
    } else {
        0
    }
}

#[inline]
pub fn zis_odd(a: ZVal) -> ZVal {
    match a.as_int() {
        Some(n) => ZVal::Bool(zis_odd_i64(n) != 0),
        None => ZVal::Nil,
    }
}
#[inline]
pub fn zis_odd_i64(n: i64) -> i64 {
    if n % 2 != 0 {
        1
    } else {
        0
    }
}

#[inline]
pub fn zdigit_sum(a: ZVal) -> ZVal {
    match a.as_int() {
        Some(n) => ZVal::Int(zdigit_sum_i64(n)),
        None => ZVal::Nil,
    }
}
#[inline]
pub fn zdigit_sum_i64(mut n: i64) -> i64 {
    n = n.abs();
    let mut s = 0i64;
    while n != 0 {
        s += n % 10;
        n /= 10;
    }
    s
}
