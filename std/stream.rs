// stream.rs —— 底层流抽象（Zero 标准库最底层）
// 一条流：shell（终端）或 file（文件）。stream_io 与 io 都建立在它之上。

use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::sync::{Mutex, OnceLock};

/// 流类型：终端（shell）或文件（file）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ZStreamKind {
    Shell,
    File,
}

/// 一条流。写入端与读取端共用一条路径（文件流读写同一文件）。
pub struct ZStream {
    pub kind: ZStreamKind,
    pub name: String,
    writer: Option<File>,
    reader: Option<BufReader<File>>,
}

impl ZStream {
    /// 终端流：输出到 stdout，输入来自 stdin。
    pub fn new_shell() -> Self {
        ZStream {
            kind: ZStreamKind::Shell,
            name: "shell".to_string(),
            writer: None,
            reader: None,
        }
    }

    /// 文件流：输出追加写入，输入按行读取。
    pub fn new_file(path: &str) -> Self {
        let writer = OpenOptions::new().create(true).append(true).open(path).ok();
        let reader = File::open(path).ok().map(BufReader::new);
        ZStream {
            kind: ZStreamKind::File,
            name: path.to_string(),
            writer,
            reader,
        }
    }

    /// 向流写入一段文本。
    pub fn write(&mut self, text: &str) {
        match self.kind {
            ZStreamKind::Shell => {
                print!("{text}");
                let _ = io::stdout().flush();
            }
            ZStreamKind::File => {
                if let Some(f) = &mut self.writer {
                    let _ = f.write_all(text.as_bytes());
                    let _ = f.flush();
                }
            }
        }
    }

    /// 从流读取一行（去掉行尾换行），EOF 时返回空串。
    pub fn read_line(&mut self) -> String {
        let mut buf = String::new();
        match self.kind {
            ZStreamKind::Shell => {
                let _ = io::stdin().read_line(&mut buf);
            }
            ZStreamKind::File => {
                if let Some(r) = &mut self.reader {
                    let _ = r.read_line(&mut buf);
                }
            }
        }
        buf.trim_end_matches(['\r', '\n']).to_string()
    }
}

/// 全局当前流（默认 shell）。编译器把 `set_stream` 映射到这里。
pub fn zstream_global() -> &'static Mutex<ZStream> {
    static STREAM: OnceLock<Mutex<ZStream>> = OnceLock::new();
    STREAM.get_or_init(|| Mutex::new(ZStream::new_shell()))
}
