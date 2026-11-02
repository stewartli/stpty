use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

use std::{
    io::Write,
    num::NonZero,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

pub struct Pane {
    pub parser: Arc<Mutex<vt100::Parser>>,
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    size: (u16, u16), // rows, cols
}

impl Pane {
    pub fn spawn(
        cfg: &crate::Config,
        rows: u16,
        cols: u16,
        dirty: Arc<AtomicBool>,
    ) -> crate::Res<Self> {
        let rows = rows.max(1);
        let cols = cols.max(1);
        let pair = native_pty_system().openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let mut cmd = CommandBuilder::new(&cfg.cmd);
        cmd.args(&cfg.args);
        cmd.env("TERM", "xterm-256color");
        if let Ok(cwd) = std::env::current_dir() {
            cmd.cwd(cwd);
        }
        // so the reader sees EOF when the child exits
        let child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        let parser = Arc::new(Mutex::new(vt100::Parser::new(
            NonZero::new(rows).unwrap(),
            NonZero::new(cols).unwrap(),
            2000,
        )));

        let p = parser.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 16 * 1024];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        p.lock().unwrap().process(&buf[..n]);
                        dirty.store(true, Ordering::Release);
                    }
                }
            }
            dirty.store(true, Ordering::Release);
        });

        Ok(Self {
            parser,
            writer,
            master: pair.master,
            child,
            size: (rows, cols),
        })
    }
    pub fn resize(&mut self, rows: u16, cols: u16) {
        if rows == 0 || cols == 0 || self.size == (rows, cols) {
            return;
        }
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
        self.parser
            .lock()
            .unwrap()
            .set_size(NonZero::new(rows).unwrap(), NonZero::new(cols).unwrap());
        self.size = (rows, cols);
    }
    pub fn send(&mut self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        self.parser.lock().unwrap().screen_mut().set_scrollback(0);
        let _ = self.writer.write_all(bytes);
        let _ = self.writer.flush();
    }
    pub fn scroll(&mut self, up: bool) {
        let step = (self.size.0 / 2).max(1) as usize;
        let mut p = self.parser.lock().unwrap();
        let cur = p.screen().scrollback();
        p.screen_mut().set_scrollback(if up {
            cur + step
        } else {
            cur.saturating_sub(step)
        });
    }
    pub fn exited(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)))
    }
}

impl Drop for Pane {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}
