// io.rs —— 面向用户的输入输出函数（依赖 stream_io -> stream）
// 提供 print / input_s / input / set_stream 四个函数。

pub fn print(fmt: &str, args: &[ZVal]) {
    ZStreamIO::write(fmt, args);
}

pub fn input_s() -> ZVal {
    ZStreamIO::read_line()
}

pub fn input(fmt: &str, args: &[ZVal]) -> ZVal {
    ZStreamIO::prompt_read(fmt, args)
}

pub fn set_stream(kind: &str, name: Option<&str>) {
    ZStreamIO::set(kind, name)
}

/// 启动流桥接：后台把 `from` 的数据复制到 `to`，直到 EOF 或被
/// `stop_streaming` 停止。端点为 `"shell"`、`"net"` 或文件路径。
pub fn streaming(from: ZVal, to: ZVal) -> ZVal {
    ZVal::Bool(zstream_start_bridge(
        &from.to_rust_string(),
        &to.to_rust_string(),
    ))
}

/// 停止 `streaming` 启动的桥接。
pub fn stop_streaming(from: ZVal, to: ZVal) -> ZVal {
    ZVal::Bool(zstream_stop_bridge(
        &from.to_rust_string(),
        &to.to_rust_string(),
    ))
}
