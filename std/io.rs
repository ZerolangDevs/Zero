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
