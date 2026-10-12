// lowlevel_sys.rs —— 底层系统交互（Zero 标准库最底层，Rust 实现）
// 提供路径拼接、路径格式化与进程结束；`sys` 头文件（Zero 实现）在此基础上封装。

/// 路径拼接：`path1 +<path> path2` 的底层实现，也是 `pathJoin` 函数的实现。
pub fn zpath_join(a: ZVal, b: ZVal) -> ZVal {
    let left = a.to_rust_string();
    let right = b.to_rust_string();
    if left.is_empty() {
        return ZVal::Str(right);
    }
    if right.is_empty() {
        return ZVal::Str(left);
    }
    let sep = std::path::MAIN_SEPARATOR;
    let left_ends_sep = left.ends_with('/') || left.ends_with('\\');
    let right_starts_sep = right.starts_with('/') || right.starts_with('\\');
    let joined = if left_ends_sep || right_starts_sep {
        format!("{left}{right}")
    } else {
        format!("{left}{sep}{right}")
    };
    ZVal::Str(joined)
}

/// 规范化路径：统一分隔符为平台默认、折叠重复分隔符、去掉结尾分隔符
/// （保留根路径，如 `/` 与 Windows 驱动器根 `C:\`）。
pub fn zformat_path(a: ZVal) -> ZVal {
    let s = a.to_rust_string();
    let sep = std::path::MAIN_SEPARATOR;
    let mut out = String::new();
    let mut prev_sep = false;
    for ch in s.chars() {
        if ch == '/' || ch == '\\' {
            if !prev_sep {
                out.push(sep);
            }
            prev_sep = true;
        } else {
            out.push(ch);
            prev_sep = false;
        }
    }
    let mut trimmed = out.trim_end_matches(sep).to_string();
    if trimmed.is_empty() {
        // 纯分隔符路径：保留为根（"/"）
        trimmed.push(sep);
    } else if trimmed.len() == 2
        && trimmed.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && trimmed.ends_with(':')
    {
        // Windows 驱动器根："C:" -> "C:\"
        trimmed.push(sep);
    }
    ZVal::Str(trimmed)
}

/// 当前用户的根目录（Windows: USERPROFILE，其他: HOME），取不到返回 NULL。
pub fn zuser_path() -> ZVal {
    #[cfg(windows)]
    let home = std::env::var("USERPROFILE").ok();
    #[cfg(not(windows))]
    let home = std::env::var("HOME").ok();
    match home {
        Some(p) => ZVal::Str(p),
        None => ZVal::Nil,
    }
}

/// 结束进程/任务：字符串按映像名（Windows: taskkill /IM，其他: pkill），
/// 整数按 PID（Windows: taskkill /PID，其他: kill -9）。成功返回 true。
pub fn ztaskkill(a: ZVal) -> ZVal {
    match &a {
        ZVal::Int(pid) => {
            #[cfg(windows)]
            {
                let ok = std::process::Command::new("taskkill")
                    .args(["/F", "/PID", &pid.to_string()])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);
                ZVal::Bool(ok)
            }
            #[cfg(not(windows))]
            {
                let ok = std::process::Command::new("kill")
                    .args(["-9", &pid.to_string()])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);
                ZVal::Bool(ok)
            }
        }
        ZVal::Str(name) => {
            #[cfg(windows)]
            {
                let ok = std::process::Command::new("taskkill")
                    .args(["/F", "/IM", name])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);
                ZVal::Bool(ok)
            }
            #[cfg(not(windows))]
            {
                let ok = std::process::Command::new("pkill")
                    .args(["-9", name])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);
                ZVal::Bool(ok)
            }
        }
        _ => ZVal::Bool(false),
    }
}
