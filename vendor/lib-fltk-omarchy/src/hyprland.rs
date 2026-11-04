//! Small client for Hyprland's request socket. It only queries state and
//! dispatches actions on this process's own windows; it never edits the
//! user's Hyprland configuration.

use serde_json::Value;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

pub fn socket_path() -> Option<PathBuf> {
    let sig = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
    let dir = std::env::var_os("XDG_RUNTIME_DIR")?;
    let p = PathBuf::from(dir).join("hypr").join(sig).join(".socket.sock");
    p.exists().then_some(p)
}

pub fn available() -> bool {
    socket_path().is_some()
}

/// Raw request, e.g. `"j/clients"` or `"dispatch <lua>"`.
pub fn request(cmd: &str) -> Option<String> {
    let mut s = UnixStream::connect(socket_path()?).ok()?;
    s.set_read_timeout(Some(Duration::from_millis(500))).ok()?;
    s.write_all(cmd.as_bytes()).ok()?;
    let mut out = String::new();
    s.read_to_string(&mut out).ok()?;
    Some(out)
}

pub fn json(cmd: &str) -> Option<Value> {
    serde_json::from_str(&request(&format!("j/{cmd}"))?).ok()
}

/// Lua dispatch (Hyprland >= 0.55) with an optional legacy fallback.
pub fn dispatch(lua: &str, legacy: Option<&str>) -> bool {
    if request(&format!("dispatch {lua}")).is_some_and(|r| r.trim() == "ok") {
        return true;
    }
    legacy.is_some_and(|l| request(&format!("dispatch {l}")).is_some_and(|r| r.trim() == "ok"))
}

pub fn option_int(name: &str) -> Option<i64> {
    json(&format!("getoption {name}"))?.get("int")?.as_i64()
}

/// `general:gaps_out` is reported as CSS-like text; Omarchy uses the first value.
pub fn gaps_out() -> Option<i64> {
    let v = json("getoption general:gaps_out")?;
    if let Some(css) = v.get("css").and_then(Value::as_str) {
        return css.split_whitespace().next()?.parse::<f64>().ok().map(|f| f.round() as i64);
    }
    v.get("int")?.as_i64()
}

pub fn cursor() -> Option<(i32, i32)> {
    let v = json("cursorpos")?;
    Some((v.get("x")?.as_i64()? as i32, v.get("y")?.as_i64()? as i32))
}

#[derive(Debug, Clone)]
pub struct Client {
    pub address: String,
    pub at: (i32, i32),
    pub size: (i32, i32),
    pub floating: bool,
    pub workspace: String,
    /// Monitor id (see [`monitor_refresh_hz`]).
    pub monitor: i64,
}

/// Windows of this process, optionally filtered by title.
pub fn own_clients(title: Option<&str>) -> Vec<Client> {
    let pid = std::process::id() as i64;
    let Some(Value::Array(list)) = json("clients") else { return Vec::new() };
    list.iter()
        .filter(|c| c.get("pid").and_then(Value::as_i64) == Some(pid))
        .filter(|c| title.map_or(true, |t| c.get("title").and_then(Value::as_str) == Some(t)))
        .filter_map(|c| {
            let pair = |k: &str| -> Option<(i32, i32)> {
                let a = c.get(k)?.as_array()?;
                Some((a.first()?.as_i64()? as i32, a.get(1)?.as_i64()? as i32))
            };
            Some(Client {
                address: c.get("address")?.as_str()?.to_string(),
                at: pair("at")?,
                size: pair("size")?,
                floating: c.get("floating").and_then(Value::as_bool).unwrap_or(false),
                workspace: c.get("workspace")?.get("name")?.as_str()?.to_string(),
                monitor: c.get("monitor").and_then(Value::as_i64).unwrap_or(-1),
            })
        })
        .collect()
}

/// Refresh rate of monitor `id` (or of the focused monitor when `id` < 0).
pub fn monitor_refresh_hz(id: i64) -> Option<f64> {
    let Some(Value::Array(list)) = json("monitors") else { return None };
    let pick = list.iter().find(|m| if id >= 0 { m.get("id").and_then(Value::as_i64) == Some(id) } else { m.get("focused").and_then(Value::as_bool) == Some(true) });
    pick.or(list.first())?.get("refreshRate")?.as_f64().filter(|r| *r > 1.0)
}

/// Refresh rate of the monitor showing this process's window `title`
/// (any own window when None); None without Hyprland.
pub fn refresh_hz_for(title: Option<&str>) -> Option<f64> {
    let id = own_clients(title).first().map_or(-1, |c| c.monitor);
    monitor_refresh_hz(id)
}

fn quote(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn set_floating(address: &str, on: bool) -> bool {
    let action = if on { "enable" } else { "disable" };
    let legacy = format!("{} address:{address}", if on { "setfloating" } else { "settiled" });
    dispatch(&format!(r#"hl.dsp.window.float({{ window = "address:{}", action = "{action}" }})"#, quote(address)), Some(&legacy))
}

pub fn move_to(address: &str, x: i32, y: i32) -> bool {
    let legacy = format!("movewindowpixel exact {x} {y},address:{address}");
    dispatch(&format!(r#"hl.dsp.window.move({{ window = "address:{}", x = {x}, y = {y} }})"#, quote(address)), Some(&legacy))
}

pub fn resize_to(address: &str, w: i32, h: i32) -> bool {
    let legacy = format!("resizewindowpixel exact {w} {h},address:{address}");
    dispatch(&format!(r#"hl.dsp.window.resize({{ window = "address:{}", x = {w}, y = {h} }})"#, quote(address)), Some(&legacy))
}

pub fn center(address: &str) -> bool {
    dispatch(&format!(r#"hl.dsp.window.center({{ window = "address:{}" }})"#, quote(address)), None)
}
