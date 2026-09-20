// stream_io.rs —— 在 stream 之上封装一条“适用于 IO 的流”（依赖 stream）
// 提供格式化写入、读行、切换流等 IO 语义，供 io 头文件调用。

/// 一条封装好的 IO 流（静态函数集合）。
pub struct ZStreamIO;

impl ZStreamIO {
    /// 按 `{}` 占位符格式化并写入当前流（`print` 的底层实现）。
    pub fn write(fmt: &str, args: &[ZVal]) {
        let text = __zero_format(fmt, args);
        zstream_global().lock().unwrap().write(&text);
    }

    /// 原样写入当前流，不做格式化。
    pub fn write_raw(text: &str) {
        zstream_global().lock().unwrap().write(text);
    }

    /// 从当前流读一行并解析为 ZVal（数字自动转 Int）。
    pub fn read_line() -> ZVal {
        let s = zstream_global().lock().unwrap().read_line();
        ZVal::from_line(&s)
    }

    /// 打印提示后读入（`input` 的底层实现）。
    pub fn prompt_read(fmt: &str, args: &[ZVal]) -> ZVal {
        Self::write(fmt, args);
        Self::read_line()
    }

    /// 切换当前流：`"shell"` 或 `"file"`（后跟文件名，可选）。
    pub fn set(kind: &str, name: Option<&str>) {
        let mut g = zstream_global().lock().unwrap();
        match kind {
            "file" => {
                let path = name.unwrap_or("out.txt");
                *g = ZStream::new_file(path);
            }
            _ => {
                *g = ZStream::new_shell();
            }
        }
    }
}
