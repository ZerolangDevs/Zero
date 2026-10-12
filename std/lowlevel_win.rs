// lowlevel_win.rs —— 底层窗口系统（Slint + Material Design，Rust 实现）
// 用 Slint 动态控件模型驱动；调用接口（__zero_win_*）与旧后端完全一致，
// Zero 语言层的 win.add / win.update / win.id.del / win.id.update 无需改动。

use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};

slint::slint! {
    import { Button, LineEdit, ComboBox, ListView, Switch, Slider, CheckBox, SpinBox, TextEdit, ProgressIndicator, GroupBox } from "std-widgets.slint";

    export struct ControlData {
        kind: string,
        id: string,
        x: length,
        y: length,
        w: length,
        h: length,
        text: string,
        items: [string],
        idx: int,
    }

    export component MainWindow inherits Window {
        in-out property <[ControlData]> controls;
        in-out property <string> win_title: "Zero";
        callback control_clicked(int);
        callback control_changed(int, string);
        pure callback input_text(int) -> string;
        title: root.win_title;
        background: #1e1e1e;
        Rectangle {
            width: 100%;
            height: 100%;
            for c in controls: Rectangle {
                x: c.x; y: c.y; width: c.w; height: c.h; clip: true;
                if (c.kind == "button") : Button {
                    width: 100%; height: 100%;
                    text: c.text;
                    clicked => { root.control_clicked(c.idx); }
                }
                if (c.kind == "text") : Text {
                    width: 100%; height: 100%;
                    text: c.text;
                    color: #f0f0f0;
                    vertical-alignment: center;
                    horizontal-alignment: center;
                }
                if (c.kind == "input") : LineEdit {
                    width: 100%; height: 100%;
                    text: root.input_text(c.idx);
                    placeholder-text: c.text;
                    edited(t) => { root.control_changed(c.idx, t); }
                }
                if (c.kind == "textedit") : TextEdit {
                    width: 100%; height: 100%;
                    text: root.input_text(c.idx);
                    edited(t) => { root.control_changed(c.idx, t); }
                }
                if (c.kind == "select") : ComboBox {
                    width: 100%; height: 100%;
                    model: c.items;
                    selected(s) => { root.control_clicked(c.idx); }
                }
                if (c.kind == "selects") : ListView {
                    width: 100%; height: 100%;
                    for item in c.items: Text {
                        text: item;
                        color: #f0f0f0;
                        vertical-alignment: center;
                    }
                }
                if (c.kind == "switch") : Switch {
                    width: 100%; height: 100%;
                    text: c.text;
                    toggled() => { root.control_clicked(c.idx); }
                }
                if (c.kind == "slider") : Slider {
                    width: 100%; height: 100%;
                    changed(v) => { root.control_clicked(c.idx); }
                }
                if (c.kind == "checkbox") : CheckBox {
                    width: 100%; height: 100%;
                    text: c.text;
                    toggled() => { root.control_clicked(c.idx); }
                }
                if (c.kind == "spinbox") : SpinBox {
                    width: 100%; height: 100%;
                    edited(v) => { root.control_clicked(c.idx); }
                }
                if (c.kind == "progress") : ProgressIndicator {
                    width: 100%; height: 100%;
                }
                if (c.kind == "group") : GroupBox {
                    width: 100%; height: 100%;
                    title: c.text;
                }
            }
        }
    }
}

/// 一个控件的运行时状态。
struct ZWinControl {
    id_str: String,
    idx: i32,
    kind: &'static str,
    onclick: Option<fn() -> ZVal>,
    into: Option<*mut ZVal>,
}

/// 一个窗口。
struct ZWin {
    id: i64,
    model: std::rc::Rc<VecModel<ControlData>>,
    controls: Vec<ZWinControl>,
    next_idx: i32,
}

// 裸指针（into 变量）让结构体默认不是 Send/Sync；运行时通过 Mutex 串行化。
unsafe impl Send for ZWinControl {}
unsafe impl Sync for ZWinControl {}
unsafe impl Send for ZWin {}
unsafe impl Sync for ZWin {}

fn windows() -> &'static std::sync::Mutex<Vec<ZWin>> {
    static W: std::sync::OnceLock<std::sync::Mutex<Vec<ZWin>>> = std::sync::OnceLock::new();
    W.get_or_init(|| std::sync::Mutex::new(Vec::new()))
}

/// 输入框/多行文本框的当前文本（按控件全局 idx 索引）。
fn input_texts() -> &'static std::sync::Mutex<HashMap<i32, String>> {
    static T: std::sync::OnceLock<std::sync::Mutex<HashMap<i32, String>>> = std::sync::OnceLock::new();
    T.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

static NEXT_WIN_ID: AtomicI64 = AtomicI64::new(1);

/// 解析 `size`：字符串 `"400x300"`、整数（正方形）、浮点（正方形）。
fn __zero_win_size(v: Option<&ZVal>, def: (f32, f32)) -> (f32, f32) {
    match v {
        Some(ZVal::Str(s)) => {
            let parts: Vec<&str> = s.split(['x', 'X']).collect();
            if parts.len() == 2 {
                let w = parts[0].trim().parse::<f32>().unwrap_or(def.0);
                let h = parts[1].trim().parse::<f32>().unwrap_or(def.1);
                (w, h)
            } else {
                def
            }
        }
        Some(ZVal::Int(n)) => (*n as f32, *n as f32),
        Some(ZVal::Float(f)) => (*f as f32, *f as f32),
        _ => def,
    }
}

/// 把可选位置参数转成 f32（非数字视为缺省）。
fn __zero_win_pos(v: Option<&ZVal>, def: f32) -> f32 {
    match v {
        Some(ZVal::Int(n)) => *n as f32,
        Some(ZVal::Float(f)) => *f as f32,
        _ => def,
    }
}

/// 控件点击/选择/切换事件 -> 锁外调用用户回调。
fn dispatch_click(win_id: i64, idx: i32) {
    let cb = {
        let wins = windows().lock().unwrap();
        wins.iter()
            .find(|w| w.id == win_id)
            .and_then(|w| w.controls.iter().find(|c| c.idx == idx))
            .and_then(|c| c.onclick)
    };
    if let Some(cb) = cb {
        let _ = cb();
    }
}

/// 文本编辑事件：记录文本并触发回调。
fn dispatch_change(win_id: i64, idx: i32, text: &str) {
    input_texts().lock().unwrap().insert(idx, text.to_string());
    dispatch_click(win_id, idx);
}

/// 向窗口添加一个控件（推入动态模型）。
fn add_control(
    win_id: i64,
    kind: &'static str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    id_str: &str,
    text: &str,
    items: &[String],
    onclick: Option<fn() -> ZVal>,
    into: Option<*mut ZVal>,
) -> bool {
    let mut wins = windows().lock().unwrap();
    let Some(win) = wins.iter_mut().find(|w| w.id == win_id) else {
        return false;
    };
    let idx = win.next_idx;
    win.next_idx += 1;
    let data = ControlData {
        kind: kind.into(),
        id: id_str.into(),
        x: x.into(),
        y: y.into(),
        w: w.into(),
        h: h.into(),
        text: text.into(),
        items: ModelRc::new(VecModel::from(
            items
                .iter()
                .map(|s| SharedString::from(s.as_str()))
                .collect::<Vec<_>>(),
        )),
        idx: idx.into(),
    };
    win.model.push(data);
    win.controls.push(ZWinControl {
        id_str: id_str.to_string(),
        idx,
        kind,
        onclick,
        into,
    });
    true
}

/// 新建窗口：`name<win> = { x=..., y=..., size=... }`。
pub fn __zero_win_create(
    x: Option<&ZVal>,
    y: Option<&ZVal>,
    size: Option<&ZVal>,
    title: &str,
) -> ZVal {
    let (w, h) = __zero_win_size(size, (400.0, 300.0));
    let px = __zero_win_pos(x, 100.0);
    let py = __zero_win_pos(y, 100.0);
    match MainWindow::new() {
        Ok(ui) => {
            let id = NEXT_WIN_ID.fetch_add(1, Ordering::Relaxed);
            ui.set_win_title(SharedString::from(title));
            ui.window().set_size(slint::PhysicalSize::new(w as u32, h as u32));
            ui.window()
                .set_position(slint::PhysicalPosition::new(px as i32, py as i32));
            let model: std::rc::Rc<VecModel<ControlData>> = std::rc::Rc::new(VecModel::default());
            ui.set_controls(ModelRc::new(model.clone()));
            ui.on_control_clicked(move |idx| dispatch_click(id, idx));
            ui.on_control_changed(move |idx, t| dispatch_change(id, idx, &t.to_string()));
            ui.on_input_text(|idx| {
                SharedString::from(
                    input_texts().lock().unwrap().get(&idx).cloned().unwrap_or_default(),
                )
            });
            if ui.show().is_err() {
                return ZVal::Int(-1);
            }
            windows().lock().unwrap().push(ZWin {
                id,
                model,
                controls: Vec::new(),
                next_idx: 0,
            });
            ZVal::Int(id)
        }
        Err(_) => ZVal::Int(-1),
    }
}

/// 通用控件添加入口：`window.add.<kind>(...)`。
pub fn __zero_win_add(
    kind: &'static str,
    win: &ZVal,
    x: Option<&ZVal>,
    y: Option<&ZVal>,
    size: Option<&ZVal>,
    id: &str,
    items: &str,
    onclick: Option<fn() -> ZVal>,
    into: Option<&mut ZVal>,
) -> ZVal {
    let (w, h) = __zero_win_size(size, (120.0, 40.0));
    let px = __zero_win_pos(x, 10.0);
    let py = __zero_win_pos(y, 10.0);
    let wid = win.as_int().unwrap_or(-1);
    let item_list: Vec<String> = items
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let ok = add_control(
        wid,
        kind,
        px,
        py,
        w,
        h,
        id,
        id,
        &item_list,
        onclick,
        into.map(|r| r as *mut ZVal),
    );
    ZVal::Bool(ok)
}

/// 更新窗口：泵送事件（非阻塞）+ 把输入框内容同步到 into 变量。
pub fn __zero_win_update(win: &ZVal) -> ZVal {
    let id = win.as_int().unwrap_or(-1);
    // 事件泵：短暂运行 Slint 事件循环（10ms）后返回，保持非阻塞语义。
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(10));
        let _ = slint::quit_event_loop();
    });
    let _ = slint::run_event_loop();
    // 同步输入框/多行文本内容到 into 变量。
    let texts = input_texts().lock().unwrap();
    let wins = windows().lock().unwrap();
    if let Some(win) = wins.iter().find(|w| w.id == id) {
        for c in &win.controls {
            if c.kind == "input" || c.kind == "textedit" {
                if let Some(ptr) = c.into {
                    let text = texts.get(&c.idx).cloned().unwrap_or_default();
                    unsafe {
                        *ptr = ZVal::Str(text);
                    }
                }
            }
        }
    }
    ZVal::Nil
}

/// 删除控件：`window.<id>.del()`。
pub fn __zero_win_del(win: &ZVal, id_str: &str) -> ZVal {
    let id = win.as_int().unwrap_or(-1);
    let mut wins = windows().lock().unwrap();
    let Some(win) = wins.iter_mut().find(|w| w.id == id) else {
        return ZVal::Bool(false);
    };
    let mut pos = None;
    for i in 0..win.model.row_count() {
        if let Some(row) = win.model.row_data(i) {
            if row.id.as_str() == id_str {
                pos = Some(i);
                break;
            }
        }
    }
    let Some(pos) = pos else {
        return ZVal::Bool(false);
    };
    win.model.remove(pos);
    if let Some(cp) = win.controls.iter().position(|c| c.id_str == id_str) {
        win.controls.remove(cp);
    }
    ZVal::Bool(true)
}

/// 修改控件：`window.<id>.update(...)`，所有参数可选。
pub fn __zero_win_update_control(
    win: &ZVal,
    id_str: &str,
    x: Option<&ZVal>,
    y: Option<&ZVal>,
    size: Option<&ZVal>,
    new_id: Option<&str>,
    onclick: Option<fn() -> ZVal>,
    into: Option<&mut ZVal>,
) -> ZVal {
    let id = win.as_int().unwrap_or(-1);
    let mut wins = windows().lock().unwrap();
    let Some(win) = wins.iter_mut().find(|w| w.id == id) else {
        return ZVal::Bool(false);
    };
    let Some(ci) = win.controls.iter().position(|c| c.id_str == id_str) else {
        return ZVal::Bool(false);
    };
    let mut pos = None;
    for i in 0..win.model.row_count() {
        if let Some(row) = win.model.row_data(i) {
            if row.id.as_str() == id_str {
                pos = Some(i);
                break;
            }
        }
    }
    let Some(pos) = pos else {
        return ZVal::Bool(false);
    };
    let Some(mut data) = win.model.row_data(pos) else {
        return ZVal::Bool(false);
    };
    if let Some(v) = x.and_then(|v| __zero_win_num(v)) {
        data.x = v.into();
    }
    if let Some(v) = y.and_then(|v| __zero_win_num(v)) {
        data.y = v.into();
    }
    if let Some(sz) = size {
        let (w, h) = __zero_win_size(Some(sz), (data.w as f32, data.h as f32));
        data.w = w.into();
        data.h = h.into();
    }
    if let Some(nid) = new_id {
        data.id = nid.into();
        data.text = nid.into();
        win.controls[ci].id_str = nid.to_string();
    }
    win.model.set_row_data(pos, data);
    if let Some(cb) = onclick {
        win.controls[ci].onclick = Some(cb);
    }
    if into.is_some() {
        win.controls[ci].into = into.map(|r| r as *mut ZVal);
    }
    ZVal::Bool(true)
}

/// 数值参数（整数/浮点）转 f32。
fn __zero_win_num(v: &ZVal) -> Option<f32> {
    match v {
        ZVal::Int(n) => Some(*n as f32),
        ZVal::Float(f) => Some(*f as f32),
        _ => None,
    }
}
