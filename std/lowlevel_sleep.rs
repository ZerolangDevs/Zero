// lowlevel_sleep.rs —— 底层睡眠（Zero 标准库最底层，Rust 实现）
// 仅提供 zsleep：按毫秒休眠。

/// 休眠 `ms` 毫秒（整数或浮点，负数视为 0）。返回 NULL。
pub fn zsleep(ms: ZVal) -> ZVal {
    let d = match &ms {
        ZVal::Int(n) => (*n).max(0) as u64,
        ZVal::Float(f) => (*f).max(0.0) as u64,
        _ => 0,
    };
    std::thread::sleep(std::time::Duration::from_millis(d));
    ZVal::Nil
}
