// network_stream.rs —— 底层网络（Zero 标准库，Rust 实现，依赖 stream）
// 1) 裸 TCP：connect / send / recv / close（独立的 zstream_net 连接）。
// 2) HTTP 客户端（纯 std，无第三方依赖）：get / post / put / delete /
//    download / httpRequest。仅支持 http://（无 TLS）；响应按
//    Content-Length 或 chunked 解码，正文以文本返回。

// ---- 裸 TCP ----

/// 连接 TCP 服务器并建立独立的网络流。
pub fn net_connect(host: ZVal, port: ZVal) -> ZVal {
    let addr = format!("{}:{}", host.to_rust_string(), port.to_rust_string());
    match std::net::TcpStream::connect(&addr) {
        Ok(s) => {
            let _ = s.set_nodelay(true);
            *zstream_net().lock().unwrap() = Some(s);
            ZVal::Bool(true)
        }
        Err(_) => ZVal::Bool(false),
    }
}

/// 向网络流发送一行数据。
pub fn net_send(data: ZVal) -> ZVal {
    let mut text = data.to_rust_string();
    text.push('\n');
    let mut net = zstream_net().lock().unwrap();
    match net.as_mut() {
        Some(s) => {
            let ok = std::io::Write::write_all(s, text.as_bytes()).is_ok()
                && std::io::Write::flush(s).is_ok();
            ZVal::Bool(ok)
        }
        None => ZVal::Bool(false),
    }
}

/// 从网络流读取一行。
pub fn net_recv() -> ZVal {
    let mut net = zstream_net().lock().unwrap();
    match net.as_mut() {
        Some(s) => ZVal::from_line(&zstream_tcp_read_line(s)),
        None => ZVal::Nil,
    }
}

/// 关闭网络流。
pub fn net_close() -> ZVal {
    *zstream_net().lock().unwrap() = None;
    ZVal::Bool(true)
}

// ---- HTTP ----

/// 在字节串中查找子串。
fn http_find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > hay.len() {
        return None;
    }
    (0..=hay.len() - needle.len()).find(|&i| &hay[i..i + needle.len()] == needle)
}

/// 读取响应头中指定字段的值（大小写不敏感）。
fn http_header<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    head.lines().find_map(|l| {
        let (k, v) = l.split_once(':')?;
        if k.trim().eq_ignore_ascii_case(name) {
            Some(v.trim())
        } else {
            None
        }
    })
}

/// 解码 chunked 传输编码的正文。
fn http_decode_chunked(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut rest = body;
    loop {
        let line_end = match rest.iter().position(|&b| b == b'\n') {
            Some(e) => e,
            None => break,
        };
        let size_line = String::from_utf8_lossy(&rest[..line_end]);
        let size = usize::from_str_radix(size_line.trim(), 16).unwrap_or(0);
        rest = &rest[line_end + 1..];
        if size == 0 {
            break;
        }
        if rest.len() < size {
            break;
        }
        out.extend_from_slice(&rest[..size]);
        rest = &rest[size..];
        rest = rest.strip_prefix(b"\r\n").unwrap_or(rest);
    }
    out
}

/// 发起一次 HTTP/1.1 请求，返回 (状态码, 正文)。支持 `http://` 与 `https://`
/// （HTTPS 在 Windows 上通过系统 WinHTTP/SChannel 实现 TLS）。
fn http_request(method: &str, url: &str, data: &[u8]) -> Result<(u16, Vec<u8>), String> {
    if url.starts_with("https://") {
        return https_request(method, url, data);
    }
    let rest = url
        .strip_prefix("http://")
        .ok_or("only http:// / https:// URLs are supported")?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.rfind(':') {
        Some(i) if authority[i + 1..].bytes().all(|b| b.is_ascii_digit()) => {
            let port = authority[i + 1..].parse::<u16>().unwrap_or(80);
            (&authority[..i], port)
        }
        _ => (authority, 80),
    };
    let path = if path.is_empty() { "/" } else { path };

    let mut stream =
        std::net::TcpStream::connect((host, port)).map_err(|e| format!("connect: {e}"))?;
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(15)));

    let mut req = format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n");
    if !data.is_empty() {
        req.push_str(&format!("Content-Length: {}\r\n", data.len()));
    }
    req.push_str("\r\n");
    std::io::Write::write_all(&mut stream, req.as_bytes())
        .map_err(|e| format!("write request: {e}"))?;
    if !data.is_empty() {
        std::io::Write::write_all(&mut stream, data).map_err(|e| format!("write body: {e}"))?;
    }
    std::io::Write::flush(&mut stream).map_err(|e| format!("flush: {e}"))?;

    let mut all = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        match std::io::Read::read(&mut stream, &mut buf) {
            Ok(0) => break,
            Ok(n) => all.extend_from_slice(&buf[..n]),
            Err(_) => break,
        }
    }

    let sep = http_find(&all, b"\r\n\r\n").ok_or("malformed response")?;
    let head = &all[..sep];
    let body = &all[sep + 4..];
    let head_text = String::from_utf8_lossy(head);
    let status: u16 = head_text
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let clen = http_header(&head_text, "content-length")
        .and_then(|v| v.trim().parse::<usize>().ok());
    let chunked = http_header(&head_text, "transfer-encoding")
        .map(|v| v.to_lowercase().contains("chunked"))
        .unwrap_or(false);

    let body_vec = if let Some(n) = clen {
        body[..n.min(body.len())].to_vec()
    } else if chunked {
        http_decode_chunked(body)
    } else {
        body.to_vec()
    };
    Ok((status, body_vec))
}

/// 通用请求：`httpRequest(method, url, data)`。返回正文文本（失败返回 NULL）。
pub fn http_request_fn(method: ZVal, url: ZVal, data: ZVal) -> ZVal {
    let m = method.to_rust_string().to_uppercase();
    let d = data.to_rust_string();
    match http_request(&m, &url.to_rust_string(), d.as_bytes()) {
        Ok((_, body)) => ZVal::Str(String::from_utf8_lossy(&body).to_string()),
        Err(_) => ZVal::Nil,
    }
}

/// GET：返回正文文本（失败返回 NULL）。
pub fn http_get(url: ZVal) -> ZVal {
    match http_request("GET", &url.to_rust_string(), &[]) {
        Ok((_, body)) => ZVal::Str(String::from_utf8_lossy(&body).to_string()),
        Err(_) => ZVal::Nil,
    }
}

/// POST：提交数据，返回正文文本（失败返回 NULL）。
pub fn http_post(url: ZVal, data: ZVal) -> ZVal {
    let d = data.to_rust_string();
    match http_request("POST", &url.to_rust_string(), d.as_bytes()) {
        Ok((_, body)) => ZVal::Str(String::from_utf8_lossy(&body).to_string()),
        Err(_) => ZVal::Nil,
    }
}

/// PUT：提交数据，返回正文文本（失败返回 NULL）。
pub fn http_put(url: ZVal, data: ZVal) -> ZVal {
    let d = data.to_rust_string();
    match http_request("PUT", &url.to_rust_string(), d.as_bytes()) {
        Ok((_, body)) => ZVal::Str(String::from_utf8_lossy(&body).to_string()),
        Err(_) => ZVal::Nil,
    }
}

/// DELETE：返回正文文本（失败返回 NULL）。
pub fn http_delete(url: ZVal) -> ZVal {
    match http_request("DELETE", &url.to_rust_string(), &[]) {
        Ok((_, body)) => ZVal::Str(String::from_utf8_lossy(&body).to_string()),
        Err(_) => ZVal::Nil,
    }
}

/// 下载：GET 并把正文保存到文件。成功返回 true。
pub fn http_download(url: ZVal, file: ZVal) -> ZVal {
    match http_request("GET", &url.to_rust_string(), &[]) {
        Ok((_, body)) => ZVal::Bool(
            std::fs::write(file.to_rust_string(), &body).is_ok(),
        ),
        Err(_) => ZVal::Bool(false),
    }
}


// ---- HTTPS：Windows WinHTTP（动态加载，系统 TLS/SChannel）----

/// UTF-16 宽字符串（以 NUL 结尾）。
fn https_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
mod zw_http {
    use std::ffi::c_void;

    pub type HINTERNET = *mut c_void;
    pub type BOOL = i32;

    #[link(name = "kernel32")]
    extern "system" {
        fn LoadLibraryW(name: *const u16) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const i8) -> *mut c_void;
    }

    type FnOpen =
        unsafe extern "system" fn(*const u16, u32, *const u16, *const u16, u32) -> HINTERNET;
    type FnConnect = unsafe extern "system" fn(HINTERNET, *const u16, u16, u32) -> HINTERNET;
    type FnOpenRequest = unsafe extern "system" fn(
        HINTERNET,
        *const u16,
        *const u16,
        *const u16,
        *const u16,
        *mut *const u16,
        u32,
    ) -> HINTERNET;
    type FnSendRequest = unsafe extern "system" fn(
        HINTERNET,
        *const u16,
        u32,
        *mut c_void,
        u32,
        u32,
        usize,
    ) -> BOOL;
    type FnReceiveResponse = unsafe extern "system" fn(HINTERNET, *mut c_void) -> BOOL;
    type FnQueryHeaders =
        unsafe extern "system" fn(HINTERNET, u32, *const u16, *mut c_void, *mut u32, *mut u32) -> BOOL;
    type FnReadData = unsafe extern "system" fn(HINTERNET, *mut c_void, u32, *mut u32) -> BOOL;
    type FnClose = unsafe extern "system" fn(HINTERNET) -> BOOL;

    pub struct WinHttp {
        pub open: FnOpen,
        pub connect: FnConnect,
        pub open_request: FnOpenRequest,
        pub send: FnSendRequest,
        pub receive: FnReceiveResponse,
        pub query: FnQueryHeaders,
        pub read: FnReadData,
        pub close: FnClose,
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// 动态加载 winhttp.dll 并解析函数指针（避免链接期依赖）。
    pub fn load() -> Option<&'static WinHttp> {
        static W: std::sync::OnceLock<Option<WinHttp>> = std::sync::OnceLock::new();
        W.get_or_init(|| {
            let lib = unsafe { LoadLibraryW(wide("winhttp.dll").as_ptr()) };
            if lib.is_null() {
                return None;
            }
            let sym = |name: &[u8]| unsafe { GetProcAddress(lib, name.as_ptr() as *const i8) };
            let ptrs = [
                sym(b"WinHttpOpen\0"),
                sym(b"WinHttpConnect\0"),
                sym(b"WinHttpOpenRequest\0"),
                sym(b"WinHttpSendRequest\0"),
                sym(b"WinHttpReceiveResponse\0"),
                sym(b"WinHttpQueryHeaders\0"),
                sym(b"WinHttpReadData\0"),
                sym(b"WinHttpCloseHandle\0"),
            ];
            if ptrs.iter().any(|p| p.is_null()) {
                return None; // 符号缺失：拒绝使用，避免调用空指针
            }
            Some(WinHttp {
                open: unsafe { std::mem::transmute(ptrs[0]) },
                connect: unsafe { std::mem::transmute(ptrs[1]) },
                open_request: unsafe { std::mem::transmute(ptrs[2]) },
                send: unsafe { std::mem::transmute(ptrs[3]) },
                receive: unsafe { std::mem::transmute(ptrs[4]) },
                query: unsafe { std::mem::transmute(ptrs[5]) },
                read: unsafe { std::mem::transmute(ptrs[6]) },
                close: unsafe { std::mem::transmute(ptrs[7]) },
            })
        })
        .as_ref()
    }

    /// 通过 WinHTTP 发起一次 HTTPS 请求，返回 (状态码, 正文)。
    pub fn request(
        w: &WinHttp,
        method: &str,
        url: &str,
        data: &[u8],
    ) -> Result<(u16, Vec<u8>), String> {
        let rest = url.strip_prefix("https://").ok_or("not an https url")?;
        let (authority, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };
        let (host, port) = match authority.rfind(':') {
            Some(i) if authority[i + 1..].bytes().all(|b| b.is_ascii_digit()) => {
                let port = authority[i + 1..].parse::<u16>().unwrap_or(443);
                (&authority[..i], port)
            }
            _ => (authority, 443),
        };
        let path = if path.is_empty() { "/" } else { path };

        let host_w = wide(host);
        let path_w = wide(path);
        let verb_w = wide(method);
        let version_w = wide("HTTP/1.1");

        unsafe {
            // WINHTTP_ACCESS_TYPE_NO_PROXY = 1
            let session = (w.open)(
                wide("Zero").as_ptr(),
                1,
                std::ptr::null(),
                std::ptr::null(),
                0,
            );
            if session.is_null() {
                return Err("WinHttpOpen failed".into());
            }
            let conn = (w.connect)(session, host_w.as_ptr(), port, 0);
            if conn.is_null() {
                (w.close)(session);
                return Err("WinHttpConnect failed".into());
            }
            // WINHTTP_FLAG_SECURE = 0x00800000
            let req = (w.open_request)(
                conn,
                verb_w.as_ptr(),
                path_w.as_ptr(),
                version_w.as_ptr(),
                std::ptr::null(),
                std::ptr::null_mut(),
                0x0080_0000,
            );
            if req.is_null() {
                (w.close)(conn);
                (w.close)(session);
                return Err("WinHttpOpenRequest failed".into());
            }
            let sent = (w.send)(
                req,
                std::ptr::null(),
                0,
                data.as_ptr() as *mut c_void,
                data.len() as u32,
                data.len() as u32,
                0,
            );
            if sent == 0 {
                (w.close)(req);
                (w.close)(conn);
                (w.close)(session);
                return Err("WinHttpSendRequest failed".into());
            }
            if (w.receive)(req, std::ptr::null_mut()) == 0 {
                (w.close)(req);
                (w.close)(conn);
                (w.close)(session);
                return Err("WinHttpReceiveResponse failed".into());
            }
            // WINHTTP_QUERY_STATUS_CODE = 19
            let mut status: u32 = 0;
            let mut len = 4u32;
            let mut index = 0u32;
            (w.query)(
                req,
                19,
                std::ptr::null(),
                &mut status as *mut u32 as *mut c_void,
                &mut len,
                &mut index,
            );
            let mut body = Vec::new();
            let mut buf = [0u8; 8192];
            loop {
                let mut read = 0u32;
                let r = (w.read)(req, buf.as_mut_ptr() as *mut c_void, 8192, &mut read);
                if r == 0 || read == 0 {
                    break;
                }
                body.extend_from_slice(&buf[..read as usize]);
            }
            (w.close)(req);
            (w.close)(conn);
            (w.close)(session);
            Ok((status as u16, body))
        }
    }
}

/// 发起 HTTPS 请求（Windows 上经 WinHTTP 系统 TLS；其他平台不支持）。
fn https_request(method: &str, url: &str, data: &[u8]) -> Result<(u16, Vec<u8>), String> {
    #[cfg(windows)]
    {
        let w = zw_http::load().ok_or("cannot load winhttp.dll")?;
        zw_http::request(w, method, url, data)
    }
    #[cfg(not(windows))]
    {
        let _ = (method, url, data);
        Err("https requires Windows (winhttp)".to_string())
    }
}
