use std::{
    io::Write,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use crossterm::{
    cursor::{Hide, Show},
    event::{self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyEvent, KeyEventKind},
    execute, queue,
    style::{Attribute, ResetColor, SetAttribute},
    terminal::{
        self, BeginSynchronizedUpdate, Clear, ClearType, EndSynchronizedUpdate,
        EnterAlternateScreen, LeaveAlternateScreen,
    },
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Main,
    Popup,
    VSplit,
}

pub struct App {
    cfg: crate::Config,
    view: View,
    pane: Option<crate::Pane>,
    error: Option<String>,
    status: String,
    dirty: Arc<AtomicBool>,
    quit: bool,
}

impl App {
    pub fn new(cfg: crate::Config) -> Self {
        Self {
            cfg,
            view: View::Main,
            pane: None,
            error: None,
            status: String::new(),
            dirty: Arc::new(AtomicBool::new(false)),
            quit: false,
        }
    }
    fn toggle(&mut self, target: View) {
        self.view = if self.view == target {
            View::Main
        } else {
            target
        };
        // manually record status and error
        if self.view != View::Main && self.pane.is_none() {
            self.error = None;
        }
        self.status.clear();
    }
    fn run_action(&mut self, action: crate::Action) {
        match action {
            crate::Action::TogglePopup => self.toggle(View::Popup),
            crate::Action::ToggleSplit => self.toggle(View::VSplit),
            crate::Action::Quit => self.quit = true,
            crate::Action::ScrollUp | crate::Action::ScrollDown => {
                if self.view != View::Main
                    && let Some(p) = &mut self.pane
                {
                    p.scroll(matches!(action, crate::Action::ScrollUp));
                }
            }
            crate::Action::Send(text) => {
                if self.view != View::Main
                    && let Some(p) = &mut self.pane
                {
                    p.send(text.as_bytes());
                }
            }
        }
    }
    fn on_key(&mut self, k: KeyEvent) {
        if let Some(action) = self.cfg.lookup(&k).cloned() {
            self.run_action(action);
            return;
        }
        if self.view != View::Main
            && let Some(p) = &mut self.pane
        {
            let app_cursor = p.parser.lock().unwrap().screen().application_cursor();
            p.send(&crate::key_to_bytes(k, app_cursor));
        }
    }
    fn on_paste(&mut self, text: String) {
        if self.view == View::Main {
            return;
        }
        if let Some(p) = &mut self.pane {
            let bracketed = p.parser.lock().unwrap().screen().bracketed_paste();
            if bracketed {
                p.send(format!("\x1b[200~{text}\x1b[201~").as_bytes());
            } else {
                p.send(text.replace('\n', "\r").as_bytes());
            }
        }
    }
    fn draw_main(&self, out: &mut Vec<u8>, r: crate::Rect) -> std::io::Result<()> {
        crate::draw_box(out, r, "myter")?;
        if r.w < 6 || r.h < 4 {
            return Ok(());
        }
        let (x, w) = (r.x + 2, r.w - 4);
        let mut y = r.y + 1;
        let max_y = r.y + r.h - 1;

        let mut line = |out: &mut Vec<u8>, s: &str| -> std::io::Result<()> {
            if y < max_y {
                crate::put_text(out, x, y, w, s)?;
                y += 1;
            }
            Ok(())
        };

        line(out, "Interactive R in a popup or vertical split.")?;
        line(out, "")?;
        let mut binds: Vec<(&String, &crate::Action)> =
            self.cfg.keys.values().map(|(l, a)| (l, a)).collect();
        binds.sort_by(|a, b| a.0.cmp(b.0));
        for (label, action) in binds {
            line(out, &format!("{label:<12} {}", action))?;
        }
        line(out, "")?;
        line(out, "While R is visible every other key goes to R.")?;
        if !self.status.is_empty() {
            line(out, "")?;
            line(out, &self.status)?;
        }
        Ok(())
    }
    fn draw_pane(&mut self, out: &mut Vec<u8>, r: crate::Rect) -> std::io::Result<()> {
        crate::draw_box(
            out,
            r,
            &format!("{} (alt-p popup, alt-v split)", self.cfg.cmd),
        )?;
        if r.w < 4 || r.h < 3 {
            return Ok(());
        }
        let inner = crate::Rect {
            x: r.x + 1,
            y: r.y + 1,
            w: r.w - 2,
            h: r.h - 2,
        };

        if self.pane.is_none() && self.error.is_none() {
            match crate::Pane::spawn(&self.cfg, inner.h, inner.w, self.dirty.clone()) {
                Ok(p) => self.pane = Some(p),
                Err(e) => self.error = Some(e.to_string()),
            }
        }
        match &mut self.pane {
            Some(p) => {
                p.resize(inner.h, inner.w);
                let parser = p.parser.lock().unwrap();
                crate::blit(out, parser.screen(), inner)?;
            }
            None => {
                crate::fill(out, inner)?;
                if let Some(err) = &self.error {
                    let msg = format!("failed to start {:?}: {err}", self.cfg.cmd);
                    crate::put_text(out, inner.x, inner.y, inner.w, &msg)?;
                }
            }
        }
        Ok(())
    }
    fn render(&mut self) -> std::io::Result<()> {
        let (cols, rows) = terminal::size()?;
        let full = crate::Rect {
            x: 0,
            y: 0,
            w: cols,
            h: rows,
        };
        let mut buf: Vec<u8> = Vec::with_capacity(32 * 1024);

        queue!(
            buf,
            BeginSynchronizedUpdate,
            Hide,
            SetAttribute(Attribute::Reset),
            ResetColor,
            Clear(ClearType::All)
        )?;

        match self.view {
            View::Main => self.draw_main(&mut buf, full)?,
            View::Popup => {
                self.draw_main(&mut buf, full)?;
                let r = crate::popup_rect(cols, rows, self.cfg.popup_w, self.cfg.popup_h);
                self.draw_pane(&mut buf, r)?;
            }
            View::VSplit => {
                let (l, r) = crate::split_rects(cols, rows);
                self.draw_main(&mut buf, l)?;
                self.draw_pane(&mut buf, r)?;
            }
        }

        queue!(buf, EndSynchronizedUpdate)?;
        let mut so = std::io::stdout().lock();
        so.write_all(&buf)?;
        so.flush()
    }
    pub fn run(&mut self) -> crate::Res<()> {
        let mut need_draw = true;
        while !self.quit {
            if let Some(p) = &mut self.pane
                && p.exited()
            {
                self.pane = None;
                self.view = View::Main;
                self.status = "R exited".into();
                need_draw = true;
            }

            if need_draw || self.dirty.swap(false, Ordering::AcqRel) {
                self.render()?;
                need_draw = false;
            }

            if event::poll(Duration::from_millis(20))? {
                loop {
                    match event::read()? {
                        Event::Key(k) if k.kind != KeyEventKind::Release => self.on_key(k),
                        Event::Paste(s) => self.on_paste(s),
                        _ => {}
                    }
                    if !event::poll(Duration::ZERO)? {
                        break;
                    }
                }
                // keys, resizes, and mouse noise all just trigger a repaint
                need_draw = true;
            }
        }
        Ok(())
    }
}

pub struct TermGuard;

impl TermGuard {
    pub fn new() -> std::io::Result<Self> {
        terminal::enable_raw_mode()?;
        execute!(
            std::io::stdout(),
            EnterAlternateScreen,
            EnableBracketedPaste,
            Hide
        )?;
        Ok(Self)
    }
}

impl Drop for TermGuard {
    fn drop(&mut self) {
        let _ = execute!(
            std::io::stdout(),
            Show,
            DisableBracketedPaste,
            LeaveAlternateScreen
        );
        let _ = terminal::disable_raw_mode();
    }
}
