// network_stream.rs —— 底层网络（Zero 标准库，Rust 实现，依赖 stream）
// 1) 裸 TCP：connect / send / recv / close（独立的 zstream_net 连接）。
// 2) HTTP 客户端（基于 ureq，跨平台 TLS/rustls）：get / post / put /
//    delete / download / httpRequest。支持 http:// 与 https://，自动
//    处理重定向、gzip、chunked。使用 net 库的程序需经 cargo 构建
//    （编译器会自动为 native 构建生成含 ureq 依赖的 Cargo 工程）。

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
// 非 wasm（native）：基于 ureq（跨平台 TLS/rustls）。
// wasm32：通过 JS 宿主桥接（浏览器同步 XHR / Node 子进程 fetch），
//         由编译器生成的 `<stem>.js` 胶水提供 `zero` 导入模块。

#[cfg(not(target_arch = "wasm32"))]
mod zw_native {
    /// 全局 HTTP agent：复用连接与 TLS 会话缓存。
    pub fn agent() -> &'static ureq::Agent {
        static AGENT: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
        AGENT.get_or_init(|| {
            ureq::AgentBuilder::new()
                .timeout_connect(std::time::Duration::from_secs(15))
                .timeout_read(std::time::Duration::from_secs(30))
                .timeout_write(std::time::Duration::from_secs(30))
                .build()
        })
    }

    /// 发起一次 HTTP/HTTPS 请求，返回 (状态码, 正文)。
    /// 支持 `http://` 与 `https://`；自动处理 gzip、chunked 与重定向。
    pub fn request(method: &str, url: &str, data: &[u8]) -> Result<(u16, Vec<u8>), String> {
        let agent = agent();
        let result = if data.is_empty() {
            agent.request(method, url).call()
        } else {
            agent.request(method, url).send_bytes(data)
        };
        match result {
            Ok(resp) => {
                let status = resp.status();
                let mut body = Vec::new();
                std::io::Read::read_to_end(&mut resp.into_reader(), &mut body)
                    .map_err(|e| format!("read body: {e}"))?;
                Ok((status, body))
            }
            // 4xx/5xx 仍是有效响应：连同状态码与正文一起返回。
            Err(ureq::Error::Status(code, resp)) => {
                let mut body = Vec::new();
                let _ = std::io::Read::read_to_end(&mut resp.into_reader(), &mut body);
                Ok((code, body))
            }
            Err(e) => Err(e.to_string()),
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod zw_wasm {
    /// JS 宿主导入（见生成产物 `<stem>.js`）。
    /// `zero_net_http` 返回 HTTP 状态码（>=200），失败返回负数；
    /// 随后用 `zero_net_body_len` 查询正文长度，`zero_net_read` 拷贝正文。
    #[link(wasm_import_module = "zero")]
    extern "C" {
        fn zero_net_http(
            m: *const u8,
            ml: usize,
            u: *const u8,
            ul: usize,
            b: *const u8,
            bl: usize,
        ) -> i32;
        fn zero_net_body_len() -> i32;
        fn zero_net_read(dst: *mut u8, cap: usize) -> i32;
    }

    /// 通过 JS 宿主发起一次 HTTP/HTTPS 请求，返回 (状态码, 正文)。
    pub fn request(method: &str, url: &str, data: &[u8]) -> Result<(u16, Vec<u8>), String> {
        let rc = unsafe {
            zero_net_http(
                method.as_ptr(),
                method.len(),
                url.as_ptr(),
                url.len(),
                data.as_ptr(),
                data.len(),
            )
        };
        if rc < 0 {
            return Err(format!("host HTTP error ({rc})"));
        }
        let len = unsafe { zero_net_body_len() };
        if len < 0 {
            return Err("host HTTP body error".into());
        }
        let mut out = vec![0u8; len as usize];
        let n = unsafe { zero_net_read(out.as_mut_ptr(), out.len()) };
        out.truncate(if n > 0 { n as usize } else { 0 });
        Ok((rc as u16, out))
    }
}

/// 发起一次 HTTP/HTTPS 请求，返回 (状态码, 正文)。
fn http_request(method: &str, url: &str, data: &[u8]) -> Result<(u16, Vec<u8>), String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        zw_native::request(method, url, data)
    }
    #[cfg(target_arch = "wasm32")]
    {
        zw_wasm::request(method, url, data)
    }
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
