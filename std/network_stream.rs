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

/// 发起一次 HTTP/1.1 请求，返回 (状态码, 正文)。仅支持 `http://`。
fn http_request(method: &str, url: &str, data: &[u8]) -> Result<(u16, Vec<u8>), String> {
    let rest = url
        .strip_prefix("http://")
        .ok_or("only http:// URLs are supported (no https/tls)")?;
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
