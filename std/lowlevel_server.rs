// lowlevel_server.rs —— Flask 风格 HTTP 服务器（Zero 标准库，Rust 实现）
// 纯 std：TcpListener 实现的 HTTP/1.1 服务器 + 路由注册 + 请求上下文。
// 用法（经 Zero 语法库 `import server` 门控）：
//   server.get|post|put|delete(path, handler)
//   server.request(method, path, handler)   任意方法
//   server.start(port)              阻塞启动（类似 net.connect 的动词风格）
//   handler 内：server.query(key) / server.header(name) / server.param(key)
// handler 可写为 `func h()`（0 参）或 `func h(method, path, body)`（3 参）。
// 路径参数用 `{name}`（与 print 的 {} 格式一致）：/user/{id}
// 注意：本文件被平铺进生成的 crate，因此全部使用完整路径，避免 `use` 冲突。

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Mutex;

/// 路由处理器：`func h()`（0 参）或 `func h(method, path, body)`（3 参）。
pub enum ZRouteHandler {
    F0(fn() -> ZVal),
    F3(fn(ZVal, ZVal, ZVal) -> ZVal),
}

/// 一条已注册的路由。
struct ZRoute {
    method: String,
    segs: Vec<String>, // 路径分段；`<name>` 段为参数占位符
    handler: ZRouteHandler,
}

/// 当前请求上下文（handler 执行期间由请求线程填充）。
struct ZRequest {
    method: String,
    path: String, // 不含查询串
    query: Vec<(String, String)>,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    params: Vec<(String, String)>,
}

/// 路由表（全局）。
static ZROUTES: Mutex<Vec<ZRoute>> = Mutex::new(Vec::new());

/// 当前请求上下文（每线程一份，多线程并发安全）。
thread_local! {
    static ZCURRENT: std::cell::RefCell<Option<ZRequest>> =
        std::cell::RefCell::new(None);
}

/// 注册一条路由：`server_add(method, path, handler)`。
pub fn server_add(method: ZVal, path: ZVal, handler: ZRouteHandler) -> ZVal {
    let method = method.to_rust_string().to_uppercase();
    let pattern = path.to_rust_string();
    let segs: Vec<String> = pattern
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    ZROUTES
        .lock()
        .unwrap()
        .push(ZRoute { method, segs, handler });
    ZVal::Bool(true)
}

/// 启动 HTTP 服务器并阻塞（`server.start(port)`）。绑定失败返回 false。
pub fn server_start(port: ZVal) -> ZVal {
    let port: u16 = port.to_rust_string().parse().unwrap_or(8080);
    let listener = match TcpListener::bind(("0.0.0.0", port)) {
        Ok(l) => l,
        Err(_) => return ZVal::Bool(false),
    };
    for stream in listener.incoming() {
        if let Ok(s) = stream {
            std::thread::spawn(move || handle_conn(s));
        }
    }
    ZVal::Bool(true)
}

/// 当前请求的 query 参数（已做百分号解码），不存在返回 NULL。
pub fn server_query(key: ZVal) -> ZVal {
    let key = key.to_rust_string();
    ZCURRENT.with(|c| match c.borrow().as_ref() {
        Some(r) => match r.query.iter().find(|(k, _)| *k == key) {
            Some((_, v)) => ZVal::Str(v.clone()),
            None => ZVal::Nil,
        },
        None => ZVal::Nil,
    })
}

/// 当前请求的指定请求头（大小写不敏感），不存在返回 NULL。
pub fn server_header(name: ZVal) -> ZVal {
    let name = name.to_rust_string().to_lowercase();
    ZCURRENT.with(|c| match c.borrow().as_ref() {
        Some(r) => match r
            .headers
            .iter()
            .find(|(k, _)| k.to_lowercase() == name)
        {
            Some((_, v)) => ZVal::Str(v.clone()),
            None => ZVal::Nil,
        },
        None => ZVal::Nil,
    })
}

/// 当前请求的路径参数（`/user/<id>` 中的 id），不存在返回 NULL。
pub fn server_param(key: ZVal) -> ZVal {
    let key = key.to_rust_string();
    ZCURRENT.with(|c| match c.borrow().as_ref() {
        Some(r) => match r.params.iter().find(|(k, _)| *k == key) {
            Some((_, v)) => ZVal::Str(v.clone()),
            None => ZVal::Nil,
        },
        None => ZVal::Nil,
    })
}

/// 处理单个连接：解析请求、匹配路由、调用 handler、写回响应。
fn handle_conn(stream: TcpStream) {
    let Ok(clone) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(clone);

    // 请求行：`METHOD PATH HTTP/1.1`
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }
    let mut parts = line.trim_end().split_whitespace();
    let (Some(method), Some(target)) = (parts.next(), parts.next()) else {
        return;
    };

    // 拆出路径与查询串。
    let (path, query_str) = match target.find('?') {
        Some(i) => (&target[..i], Some(&target[i + 1..])),
        None => (target, None),
    };
    let query = query_str.map(parse_query).unwrap_or_default();

    // 请求头 + Content-Length。
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut content_len: usize = 0;
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h).is_err() {
            return;
        }
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            let v = v.trim().to_string();
            if k.eq_ignore_ascii_case("content-length") {
                content_len = v.parse().unwrap_or(0);
            }
            headers.push((k.trim().to_string(), v));
        }
    }

    // 请求体（限制最大 10 MiB，防止恶意超大 Content-Length）。
    let mut body: Vec<u8> = Vec::new();
    if content_len > 0 && content_len <= 10 * 1024 * 1024 {
        body.resize(content_len, 0);
        if reader.read_exact(&mut body).is_err() {
            body.clear();
        }
    }

    // 路由匹配：方法 + 路径分段，`<name>` 段收集为路径参数。
    let req_segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let routes = ZROUTES.lock().unwrap();
    let mut matched: Option<(&ZRoute, Vec<(String, String)>)> = None;
    for route in routes.iter() {
        if route.method != method.to_uppercase() || route.segs.len() != req_segs.len() {
            continue;
        }
        let mut params: Vec<(String, String)> = Vec::new();
        let mut ok = true;
        for (rs, ps) in route.segs.iter().zip(req_segs.iter()) {
            if let Some(param) = rs.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
                params.push((param.to_string(), ps.to_string()));
            } else if rs != ps {
                ok = false;
                break;
            }
        }
        if ok {
            matched = Some((route, params));
            break;
        }
    }

    let (status, body_out) = match matched {
        Some((route, params)) => {
            // 供 3 参 handler 使用的正文文本（避免在 move 之后借用 body）。
            let body_text = String::from_utf8_lossy(&body).to_string();
            // 填充当前请求上下文（每线程），handler 结束后清空。
            ZCURRENT.with(|c| {
                *c.borrow_mut() = Some(ZRequest {
                    method: method.to_string(),
                    path: path.to_string(),
                    query,
                    headers,
                    body,
                    params,
                });
            });
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                match route.handler {
                    ZRouteHandler::F0(h) => h(),
                    ZRouteHandler::F3(h) => h(
                        ZVal::Str(method.to_string()),
                        ZVal::Str(path.to_string()),
                        ZVal::Str(body_text),
                    ),
                }
            }));
            ZCURRENT.with(|c| {
                *c.borrow_mut() = None;
            });
            match result {
                Ok(v) => (200u16, v.to_rust_string()),
                Err(_) => (500u16, "500 Internal Server Error".to_string()),
            }
        }
        None => (404u16, "404 Not Found".to_string()),
    };

    let _ = write_response(stream, status, &body_out);
}

/// 把响应写回连接。
fn write_response(mut stream: TcpStream, status: u16, body: &str) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "",
    };
    let mut out = Vec::new();
    out.extend_from_slice(
        format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .as_bytes(),
    );
    out.extend_from_slice(body.as_bytes());
    stream.write_all(&out)?;
    stream.flush()
}

/// 解析 query 字符串为键值对（百分号解码，`+` 视为空格）。
fn parse_query(q: &str) -> Vec<(String, String)> {
    q.split('&')
        .filter(|s| !s.is_empty())
        .filter_map(|pair| {
            let (k, v) = match pair.split_once('=') {
                Some((k, v)) => (k, v),
                None => (pair, ""),
            };
            Some((percent_decode(k), percent_decode(v)))
        })
        .collect()
}

/// 百分号解码。
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}
