// lowlevel_strings.rs —— 底层字符串运算（Zero 标准库最底层，Rust 实现）
// 提供基础字符串操作；`strings` 头文件（Zero 实现）在此基础上构建高级函数。

/// 字符串长度（按字符数）。
pub fn zlen(a: ZVal) -> ZVal {
    match &a {
        ZVal::Str(s) => ZVal::Int(s.chars().count() as i64),
        _ => ZVal::Nil,
    }
}

/// 转大写。
pub fn zupper(a: ZVal) -> ZVal {
    ZVal::Str(a.to_rust_string().to_uppercase())
}

/// 转小写。
pub fn zlower(a: ZVal) -> ZVal {
    ZVal::Str(a.to_rust_string().to_lowercase())
}

/// 去除首尾空白。
pub fn ztrim(a: ZVal) -> ZVal {
    ZVal::Str(a.to_rust_string().trim().to_string())
}

/// 是否包含子串。
pub fn zcontains(a: ZVal, b: ZVal) -> ZVal {
    ZVal::Bool(a.to_rust_string().contains(&b.to_rust_string()))
}

/// 是否以某前缀开头。
pub fn zstartswith(a: ZVal, b: ZVal) -> ZVal {
    ZVal::Bool(a.to_rust_string().starts_with(&b.to_rust_string()))
}

/// 是否以某后缀结尾。
pub fn zendswith(a: ZVal, b: ZVal) -> ZVal {
    ZVal::Bool(a.to_rust_string().ends_with(&b.to_rust_string()))
}

/// 是否为空字符串。
pub fn zis_empty(a: ZVal) -> ZVal {
    ZVal::Bool(a.to_rust_string().is_empty())
}

/// 取第 i 个字符（越界返回 NULL）。
pub fn zchar_at(a: ZVal, i: ZVal) -> ZVal {
    match (&a, i.as_int()) {
        (ZVal::Str(s), Some(idx)) => match s.chars().nth(idx.max(0) as usize) {
            Some(c) => ZVal::Str(c.to_string()),
            None => ZVal::Nil,
        },
        _ => ZVal::Nil,
    }
}

/// 取子串：substr(s, start, len)（越界安全）。
pub fn zsubstr(a: ZVal, start: ZVal, len: ZVal) -> ZVal {
    match (&a, start.as_int(), len.as_int()) {
        (ZVal::Str(s), Some(st), Some(l)) => {
            let chars: Vec<char> = s.chars().collect();
            let st = if st < 0 { 0 } else { st as usize };
            let en = st.saturating_add(if l < 0 { 0 } else { l as usize }).min(chars.len());
            if st > chars.len() {
                return ZVal::Nil;
            }
            ZVal::Str(chars[st..en].iter().collect())
        }
        _ => ZVal::Nil,
    }
}

/// 替换所有出现的 from 为 to。
pub fn zreplace(a: ZVal, from: ZVal, to: ZVal) -> ZVal {
    ZVal::Str(
        a.to_rust_string()
            .replace(&from.to_rust_string(), &to.to_rust_string()),
    )
}

/// 重复字符串 n 次。
pub fn zrepeat(a: ZVal, n: ZVal) -> ZVal {
    match (&a, n.as_int()) {
        (ZVal::Str(s), Some(k)) if k > 0 => ZVal::Str(s.repeat(k as usize)),
        (ZVal::Str(_), Some(_)) => ZVal::Str(String::new()),
        _ => ZVal::Nil,
    }
}

/// 反转字符串。
pub fn zreverse(a: ZVal) -> ZVal {
    ZVal::Str(a.to_rust_string().chars().rev().collect())
}

/// 首次出现子串的字符下标（未找到返回 -1）。
pub fn zindex_of(a: ZVal, b: ZVal) -> ZVal {
    match (&a, &b) {
        (ZVal::Str(s), ZVal::Str(sub)) => match s.find(sub) {
            Some(idx) => ZVal::Int(s[..idx].chars().count() as i64),
            None => ZVal::Int(-1),
        },
        _ => ZVal::Nil,
    }
}

/// 统计子串出现次数。
pub fn zcount(a: ZVal, b: ZVal) -> ZVal {
    ZVal::Int(
        a.to_rust_string()
            .matches(&b.to_rust_string())
            .count() as i64,
    )
}
