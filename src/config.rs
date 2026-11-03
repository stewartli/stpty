use std::{collections::HashMap, path::PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Clone)]
pub enum Action {
    TogglePopup,
    ToggleSplit,
    Quit,
    ScrollUp,
    ScrollDown,
    Send(String),
}

impl std::fmt::Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Action::TogglePopup => write!(f, "toggle popup"),
            Action::ToggleSplit => write!(f, "toggle vertical split"),
            Action::Quit => write!(f, "quit"),
            Action::ScrollUp => write!(f, "scroll R up"),
            Action::ScrollDown => write!(f, "scroll R down"),
            Action::Send(s) => write!(f, "send {:?} to R", s),
        }
    }
}

impl std::str::FromStr for Action {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "toggle_popup" => Ok(Self::TogglePopup),
            "toggle_split" => Ok(Self::ToggleSplit),
            "quit" => Ok(Self::Quit),
            "scroll_up" => Ok(Self::ScrollUp),
            "scroll_down" => Ok(Self::ScrollDown),
            other => Err(format!("unknown action {other:?}")),
        }
    }
}

pub struct Config {
    // (keycode, keymodifiers) => (label, action)
    pub keys: HashMap<(KeyCode, KeyModifiers), (String, Action)>,
    pub cmd: String,
    pub args: Vec<String>,
    pub popup_w: u16,
    pub popup_h: u16,
}

impl Config {
    fn new(prog: &str) -> crate::Res<Self> {
        let mut res = Self {
            keys: HashMap::new(),
            cmd: String::new(),
            args: vec![],
            popup_w: 80,
            popup_h: 70,
        };
        for (k, a) in [
            ("alt-p", Action::TogglePopup),
            ("alt-v", Action::ToggleSplit),
            ("alt-q", Action::Quit),
            ("alt-u", Action::ScrollUp),
            ("alt-d", Action::ScrollDown),
        ] {
            res.keys.insert(parse_key(k)?, (k.to_string(), a));
        }
        match prog {
            "r" => {
                res.cmd = String::from("R");
                res.args = vec!["--no-save".into(), "--quiet".into()];
            }
            _ => {
                res.cmd = String::from("python3");
            }
        }
        Ok(res)
    }
    pub fn load(x: Option<PathBuf>) -> crate::Res<Self> {
        let mut cfg = Self::new("r")?;
        // 1> default keymaps, 2> args file, 3> xdg path, 4> home dir,
        let text = match x {
            Some(p) => {
                Some(std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?)
            }
            None => {
                let mut candidates = vec![PathBuf::from("keymap.toml")];
                if let Some(d) = std::env::var_os("XDG_CONFIG_HOME") {
                    candidates.push(PathBuf::from(d).join("myter/keymap.toml"));
                }
                if let Some(h) = std::env::var_os("HOME") {
                    candidates.push(PathBuf::from(h).join(".config/myter/keymap.toml"));
                }
                candidates
                    .into_iter()
                    .find_map(|p| std::fs::read_to_string(p).ok())
            }
        };
        // set keymap.toml
        let Some(text) = text else { return Ok(cfg) };
        let table: toml::Table = text.parse()?;
        parse_toml(table, &mut cfg)?;
        Ok(cfg)
    }
    pub fn lookup(&self, k: &KeyEvent) -> Option<&Action> {
        let code = match k.code {
            KeyCode::Char(c) => KeyCode::Char(c.to_ascii_lowercase()),
            x => x,
        };
        let mut mods = k.modifiers;
        // shift is already folded into the char
        if matches!(code, KeyCode::Char(_)) {
            mods.remove(KeyModifiers::SHIFT);
        }
        self.keys.get(&(code, mods)).map(|(_, a)| a)
    }
}

fn parse_action(v: &toml::Value) -> crate::Res<Action> {
    if let Some(s) = v.as_str()
        && let Ok(res) = s.parse()
    {
        return Ok(res);
    }
    if let Some(t) = v.as_table()
        && let Some(text) = t.get("send").and_then(|v| v.as_str())
    {
        return Ok(Action::Send(text.to_string()));
    }
    Err("expected an action name or { send = \"...\" }".into())
}

fn parse_key(s: &str) -> crate::Res<(KeyCode, KeyModifiers)> {
    let (mods_s, key) = if s == "-" {
        ("", "-")
    } else if let Some(m) = s.strip_suffix("--") {
        (m, "-") // e.g. "alt--"
    } else {
        s.rsplit_once('-').unwrap_or(("", s))
    };
    // key modifier
    let mut mods = KeyModifiers::empty();
    for m in mods_s.split('-').filter(|m| !m.is_empty()) {
        match m.to_ascii_lowercase().as_str() {
            "ctrl" | "c" => mods |= KeyModifiers::CONTROL,
            "alt" | "a" | "meta" => mods |= KeyModifiers::ALT,
            "shift" | "s" => mods |= KeyModifiers::SHIFT,
            other => return Err(format!("unknown modifier {other:?}").into()),
        }
    }
    // key code
    let lower = key.to_ascii_lowercase();
    let code = match lower.as_str() {
        "enter" | "return" => KeyCode::Enter,
        "esc" | "escape" => KeyCode::Esc,
        "tab" => KeyCode::Tab,
        "space" => KeyCode::Char(' '),
        "backspace" | "bs" => KeyCode::Backspace,
        "delete" | "del" => KeyCode::Delete,
        "insert" => KeyCode::Insert,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        k if k.len() >= 2 && k.starts_with('f') && k[1..].chars().all(|c| c.is_ascii_digit()) => {
            KeyCode::F(k[1..].parse()?)
        }
        _ if key.chars().count() == 1 => {
            KeyCode::Char(key.chars().next().unwrap().to_ascii_lowercase())
        }
        _ => return Err(format!("unknown key {key:?}").into()),
    };
    Ok((code, mods))
}

fn parse_toml(table: toml::Table, cfg: &mut Config) -> crate::Res<()> {
    if let Some(keys) = table.get("keys").and_then(|v| v.as_table()) {
        for (label, v) in keys {
            let action = parse_action(v).map_err(|e| format!("[keys] {label:?}: {e}"))?;
            let key = parse_key(label).map_err(|e| format!("[keys] {label:?}: {e}"))?;
            cfg.keys.insert(key, (label.clone(), action));
        }
    }
    if let Some(p) = table.get("popup").and_then(|v| v.as_table()) {
        let pct = |name: &str, dflt: u16| {
            p.get(name)
                .and_then(|v| v.as_integer())
                .map(|n| n.clamp(10, 100) as u16)
                .unwrap_or(dflt)
        };
        cfg.popup_w = pct("width", cfg.popup_w);
        cfg.popup_h = pct("height", cfg.popup_h);
    }
    if let Some(r) = table.get("r").and_then(|v| v.as_table()) {
        if let Some(c) = r.get("command").and_then(|v| v.as_str()) {
            cfg.cmd = c.to_string();
        }
        if let Some(a) = r.get("args").and_then(|v| v.as_array()) {
            cfg.args = a
                .iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect();
        }
    }
    Ok(())
}

pub fn key_to_bytes(k: KeyEvent, app_cursor: bool) -> Vec<u8> {
    let shift = k.modifiers.contains(KeyModifiers::SHIFT);
    let alt = k.modifiers.contains(KeyModifiers::ALT);
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    // xterm modifier parameter: 1 + shift(1) + alt(2) + ctrl(4)
    let m = 1 + shift as u8 + 2 * alt as u8 + 4 * ctrl as u8;

    let csi = |fin: char| -> Vec<u8> {
        let s = if m > 1 {
            format!("\x1b[1;{m}{fin}")
        } else if app_cursor {
            format!("\x1bO{fin}")
        } else {
            format!("\x1b[{fin}")
        };
        s.into_bytes()
    };
    let tilde = |n: u8| -> Vec<u8> {
        let s = if m > 1 {
            format!("\x1b[{n};{m}~")
        } else {
            format!("\x1b[{n}~")
        };
        s.into_bytes()
    };

    match k.code {
        KeyCode::Up => csi('A'),
        KeyCode::Down => csi('B'),
        KeyCode::Right => csi('C'),
        KeyCode::Left => csi('D'),
        KeyCode::Home => csi('H'),
        KeyCode::End => csi('F'),
        KeyCode::Insert => tilde(2),
        KeyCode::Delete => tilde(3),
        KeyCode::PageUp => tilde(5),
        KeyCode::PageDown => tilde(6),
        KeyCode::BackTab => b"\x1b[Z".to_vec(),
        KeyCode::F(n @ 1..=4) => {
            let fin = ['P', 'Q', 'R', 'S'][(n - 1) as usize];
            let s = if m > 1 {
                format!("\x1b[1;{m}{fin}")
            } else {
                format!("\x1bO{fin}")
            };
            s.into_bytes()
        }
        KeyCode::F(n @ 5..=12) => {
            let code = [15, 17, 18, 19, 20, 21, 23, 24][(n - 5) as usize];
            tilde(code)
        }
        KeyCode::F(_) => Vec::new(),
        other => {
            let mut out = Vec::new();
            if alt {
                out.push(0x1b);
            }
            match other {
                KeyCode::Char(c) if ctrl && c.is_ascii() => {
                    out.push((c.to_ascii_lowercase() as u8) & 0x1f)
                }
                KeyCode::Char(c) => {
                    let mut b = [0u8; 4];
                    out.extend_from_slice(c.encode_utf8(&mut b).as_bytes());
                }
                KeyCode::Enter => out.push(b'\r'),
                KeyCode::Tab => out.push(b'\t'),
                KeyCode::Backspace => out.push(0x7f),
                KeyCode::Esc => out.push(0x1b),
                _ => {}
            }
            out
        }
    }
}
