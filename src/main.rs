use myter::{App, Config, TermGuard};

fn main() -> myter::Res<()> {
    let cfg = Config::load(std::env::args_os().nth(1).map(std::path::PathBuf::from))?;
    let mut app = App::new(cfg);
    let _guard = TermGuard::new()?;
    app.run()
}
