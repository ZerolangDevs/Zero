// stream.rs —— 底层流抽象（Zero 标准库最底层）
// 一条流：shell（终端）、file（文件）或 net（TCP 网络连接）。
// stream_io、io、network_stream 都建立在它之上；流桥接（streaming）也在此实现。
// 注意：本文件被平铺进生成的 crate，因此全部使用完整路径，避免 `use` 冲突。

/// 流类型：终端（shell）、文件（file）或网络（net）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ZStreamKind {
    Shell,
    File,
    Net,
}

/// 一条流。写入端与读取端共用一条端点（文件/网络流读写同一端点）。
pub struct ZStream {
    pub kind: ZStreamKind,
    pub name: String,
    writer: Option<std::fs::File>,
    reader: Option<std::io::BufReader<std::fs::File>>,
    net: Option<std::net::TcpStream>,
}

impl ZStream {
    /// 终端流：输出到 stdout，输入来自 stdin。
    pub fn new_shell() -> Self {
        ZStream {
            kind: ZStreamKind::Shell,
            name: "shell".to_string(),
            writer: None,
            reader: None,
            net: None,
        }
    }

    /// 文件流：输出追加写入，输入按行读取。
    pub fn new_file(path: &str) -> Self {
        let writer = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .ok();
        let reader = std::fs::File::open(path)
            .ok()
            .map(std::io::BufReader::new);
        ZStream {
            kind: ZStreamKind::File,
            name: path.to_string(),
            writer,
            reader,
            net: None,
        }
    }

    /// 网络流：连接 `host:port`。
    pub fn new_net(addr: &str) -> Option<Self> {
        let stream = std::net::TcpStream::connect(addr).ok()?;
        let _ = stream.set_nodelay(true);
        Some(ZStream {
            kind: ZStreamKind::Net,
            name: addr.to_string(),
            writer: None,
            reader: None,
            net: Some(stream),
        })
    }

    /// 向流写入一段文本。
    pub fn write(&mut self, text: &str) {
        match self.kind {
            ZStreamKind::Shell => {
                print!("{text}");
                let _ = std::io::Write::flush(&mut std::io::stdout());
            }
            ZStreamKind::File => {
                if let Some(f) = &mut self.writer {
                    let _ = std::io::Write::write_all(f, text.as_bytes());
                    let _ = std::io::Write::flush(f);
                }
            }
            ZStreamKind::Net => {
                if let Some(n) = &mut self.net {
                    let _ = std::io::Write::write_all(n, text.as_bytes());
                    let _ = std::io::Write::flush(n);
                }
            }
        }
    }

    /// 从流读取一行（去掉行尾换行），EOF 时返回空串。
    pub fn read_line(&mut self) -> String {
        let mut buf = String::new();
        match self.kind {
            ZStreamKind::Shell => {
                let stdin = std::io::stdin();
                let _ = std::io::BufRead::read_line(&mut stdin.lock(), &mut buf);
            }
            ZStreamKind::File => {
                if let Some(r) = &mut self.reader {
                    let _ = std::io::BufRead::read_line(r, &mut buf);
                }
            }
            ZStreamKind::Net => {
                // 逐字节读到换行。
                let mut byte = [0u8; 1];
                loop {
                    let n = match &mut self.net {
                        Some(s) => std::io::Read::read(s, &mut byte).unwrap_or(0),
                        None => 0,
                    };
                    if n == 0 {
                        break;
                    }
                    buf.push(byte[0] as char);
                    if byte[0] == b'\n' {
                        break;
                    }
                }
            }
        }
        buf.trim_end_matches(['\r', '\n']).to_string()
    }

    /// 流桥接：读一块数据（阻塞），EOF/错误返回 0。
    fn read_chunk(&mut self, buf: &mut [u8]) -> usize {
        match self.kind {
            ZStreamKind::Shell => std::io::Read::read(&mut std::io::stdin(), buf).unwrap_or(0),
            ZStreamKind::File => self
                .reader
                .as_mut()
                .map(|r| std::io::Read::read(r, buf).unwrap_or(0))
                .unwrap_or(0),
            ZStreamKind::Net => self
                .net
                .as_mut()
                .map(|n| std::io::Read::read(n, buf).unwrap_or(0))
                .unwrap_or(0),
        }
    }

    /// 流桥接：写一块数据。
    fn write_bytes(&mut self, data: &[u8]) {
        match self.kind {
            ZStreamKind::Shell => {
                let _ = std::io::Write::write_all(&mut std::io::stdout(), data);
                let _ = std::io::Write::flush(&mut std::io::stdout());
            }
            ZStreamKind::File => {
                if let Some(f) = &mut self.writer {
                    let _ = std::io::Write::write_all(f, data);
                    let _ = std::io::Write::flush(f);
                }
            }
            ZStreamKind::Net => {
                if let Some(n) = &mut self.net {
                    let _ = std::io::Write::write_all(n, data);
                    let _ = std::io::Write::flush(n);
                }
            }
        }
    }
}

/// 全局当前流（默认 shell）。`set_stream` 映射到这里。
pub fn zstream_global() -> &'static std::sync::Mutex<ZStream> {
    static STREAM: std::sync::OnceLock<std::sync::Mutex<ZStream>> = std::sync::OnceLock::new();
    STREAM.get_or_init(|| std::sync::Mutex::new(ZStream::new_shell()))
}

/// 网络库的独立连接（与全局 io 流分开，不干扰 print/input）。
pub fn zstream_net() -> &'static std::sync::Mutex<Option<std::net::TcpStream>> {
    static NET: std::sync::OnceLock<std::sync::Mutex<Option<std::net::TcpStream>>> =
        std::sync::OnceLock::new();
    NET.get_or_init(|| std::sync::Mutex::new(None))
}

/// 从 TCP 流读取一行（去掉行尾换行），EOF 返回空串。
pub fn zstream_tcp_read_line(s: &mut std::net::TcpStream) -> String {
    let mut buf = String::new();
    let mut byte = [0u8; 1];
    loop {
        let n = std::io::Read::read(s, &mut byte).unwrap_or(0);
        if n == 0 {
            break;
        }
        buf.push(byte[0] as char);
        if byte[0] == b'\n' {
            break;
        }
    }
    buf.trim_end_matches(['\r', '\n']).to_string()
}

// ---- 流桥接（streaming / stop_streaming）----

/// 桥接端点：shell、net（克隆当前网络流）或文件路径。
enum BridgeEndpoint {
    ShellRead,
    ShellWrite,
    NetRead(std::net::TcpStream),
    NetWrite(std::net::TcpStream),
    FileRead(std::io::BufReader<std::fs::File>),
    FileWrite(std::fs::File),
}

fn bridge_endpoint(name: &str, read: bool) -> Option<BridgeEndpoint> {
    match name {
        "shell" => Some(if read {
            BridgeEndpoint::ShellRead
        } else {
            BridgeEndpoint::ShellWrite
        }),
        "net" => {
            let n = match zstream_net().lock().unwrap().as_ref() {
                Some(s) => s.try_clone().ok()?,
                None => zstream_global().lock().unwrap().net.as_ref()?.try_clone().ok()?,
            };
            // 短读超时：让桥接循环能周期性检查停止标志，
            // 否则阻塞中的读会让 stop_streaming 失效。
            let _ = n.set_read_timeout(Some(std::time::Duration::from_millis(100)));
            Some(if read {
                BridgeEndpoint::NetRead(n)
            } else {
                BridgeEndpoint::NetWrite(n)
            })
        }
        path => {
            if read {
                std::fs::File::open(path)
                    .ok()
                    .map(std::io::BufReader::new)
                    .map(BridgeEndpoint::FileRead)
            } else {
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .ok()
                    .map(BridgeEndpoint::FileWrite)
            }
        }
    }
}

/// 桥读结果：数据 / 真 EOF（`Ok(0)`）/ 暂时无数据（网络超时，继续轮询）。
enum BridgeRead {
    Data(usize),
    Eof,
    WouldBlock,
}

fn bridge_read(ep: &mut BridgeEndpoint, buf: &mut [u8]) -> BridgeRead {
    let res = match ep {
        BridgeEndpoint::ShellRead => std::io::Read::read(&mut std::io::stdin(), buf),
        BridgeEndpoint::NetRead(n) => std::io::Read::read(n, buf),
        BridgeEndpoint::FileRead(f) => std::io::Read::read(f, buf),
        _ => return BridgeRead::Eof,
    };
    match res {
        Ok(0) => BridgeRead::Eof,
        Ok(k) => BridgeRead::Data(k),
        Err(e)
            if e.kind() == std::io::ErrorKind::WouldBlock
                || e.kind() == std::io::ErrorKind::TimedOut =>
        {
            BridgeRead::WouldBlock
        }
        Err(_) => BridgeRead::Eof,
    }
}

fn bridge_write(ep: &mut BridgeEndpoint, data: &[u8]) {
    match ep {
        BridgeEndpoint::ShellWrite => {
            let _ = std::io::Write::write_all(&mut std::io::stdout(), data);
            let _ = std::io::Write::flush(&mut std::io::stdout());
        }
        BridgeEndpoint::NetWrite(n) => {
            let _ = std::io::Write::write_all(n, data);
        }
        BridgeEndpoint::FileWrite(f) => {
            let _ = std::io::Write::write_all(f, data);
        }
        _ => {}
    }
}

/// 运行中的桥接：`(from, to)` -> 停止标志。
fn bridges() -> &'static std::sync::Mutex<
    std::collections::HashMap<(String, String), std::sync::Arc<std::sync::atomic::AtomicBool>>,
> {
    static B: std::sync::OnceLock<
        std::sync::Mutex<
            std::collections::HashMap<
                (String, String),
                std::sync::Arc<std::sync::atomic::AtomicBool>,
            >,
        >,
    > = std::sync::OnceLock::new();
    B.get_or_init(|| {
        std::sync::Mutex::new(std::collections::HashMap::new())
    })
}

/// 启动流桥接：后台线程把 `from` 的数据持续复制到 `to`，直到 EOF 或
/// `zstream_stop_bridge`。端点可为 `"shell"`、`"net"` 或文件路径。
pub fn zstream_start_bridge(from: &str, to: &str) -> bool {
    let key = (from.to_string(), to.to_string());
    let mut reg = bridges().lock().unwrap();
    if reg.contains_key(&key) {
        return false; // 已在桥接
    }
    let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    reg.insert(key.clone(), flag.clone());
    let from = from.to_string();
    let to = to.to_string();
    std::thread::spawn(move || {
        let mut r = bridge_endpoint(&from, true);
        let mut w = bridge_endpoint(&to, false);
        if let (Some(r), Some(w)) = (r.as_mut(), w.as_mut()) {
            let mut buf = [0u8; 4096];
            while flag.load(std::sync::atomic::Ordering::Relaxed) {
                match bridge_read(r, &mut buf) {
                    BridgeRead::Data(n) => bridge_write(w, &buf[..n]),
                    BridgeRead::Eof => break,
                    BridgeRead::WouldBlock => {} // 网络暂时无数据：继续轮询停止标志
                }
            }
        }
        bridges().lock().unwrap().remove(&key);
    });
    true
}

/// 停止流桥接（返回是否存在该桥接）。
pub fn zstream_stop_bridge(from: &str, to: &str) -> bool {
    let key = (from.to_string(), to.to_string());
    match bridges().lock().unwrap().get(&key) {
        Some(flag) => {
            flag.store(false, std::sync::atomic::Ordering::Relaxed);
            true
        }
        None => false,
    }
}
