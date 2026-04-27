//! kcore hypervisor appliance console (Ratatui).

pub mod app;
pub mod inventory;
pub mod theme;
pub mod ui;

mod tty;

use std::io::{self, stdout};
use std::sync::mpsc;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::app::{AppState, Page};
use crate::inventory::load_snapshot;

/// CLI options (also used from tests).
#[derive(Debug, Clone, Default)]
pub struct Options {
    pub dev: bool,
    /// When set, make this path the controlling TTY (dup2 on Unix).
    pub tty: Option<String>,
}

/// Main event loop. Returns `Ok(())` on clean exit (dev only) or `Err` on I/O errors.
pub fn run(opts: Options) -> io::Result<()> {
    if let Some(ref p) = opts.tty {
        tty::attach_tty(p)?;
    }

    let mut stdout = stdout();
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen, crossterm::cursor::Hide)?;

    install_panic_hook(!opts.dev);

    if !opts.dev {
        let _ = ctrlc::set_handler(|| {
            // production: ignore SIGINT
        });
    }

    let (refresh_tx, refresh_rx) = mpsc::channel::<()>();
    let (snapshot_tx, snapshot_rx) = mpsc::channel();
    let _inventory_worker = std::thread::spawn(move || loop {
        let snapshot = load_snapshot();
        if snapshot_tx.send(snapshot).is_err() {
            break;
        }

        match refresh_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(()) | Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    });

    let mut app = AppState::new(opts.dev, crate::inventory::Snapshot::default());
    app.clamp_network_selection();
    app.clamp_storage_selection();
    if opts.dev {
        eprintln!("[kcore-console] dev mode: q quits, Ctrl+C exits (best effort)");
    }

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    loop {
        while let Ok(snapshot) = snapshot_rx.try_recv() {
            app.snapshot = snapshot;
            app.clamp_network_selection();
            app.clamp_storage_selection();
        }

        terminal.draw(|f| {
            use crate::ui::draw;
            draw(f, &app);
        })?;

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    if key.code == KeyCode::Char('c')
                        && key
                            .modifiers
                            .contains(crossterm::event::KeyModifiers::CONTROL)
                    {
                        if opts.dev {
                            break;
                        }
                    } else {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Char('Q') => {
                                if opts.dev {
                                    break;
                                }
                            }
                            KeyCode::Char('r') | KeyCode::Char('R') => {
                                let _ = refresh_tx.send(());
                            }
                            KeyCode::Tab | KeyCode::Right => {
                                app.page = app.page.next();
                            }
                            KeyCode::BackTab | KeyCode::Left => {
                                app.page = app.page.prev();
                            }
                            KeyCode::Char('1') => app.page = Page::Overview,
                            KeyCode::Char('2') => app.page = Page::Network,
                            KeyCode::Char('3') => app.page = Page::Storage,
                            KeyCode::Char('4') => app.page = Page::Diagnostics,
                            KeyCode::Char('5') => app.page = Page::Help,
                            KeyCode::Char('?') | KeyCode::Char('h') | KeyCode::Char('H') => {
                                app.page = Page::Help;
                            }
                            KeyCode::Esc => {
                                app.page = Page::Overview;
                            }
                            KeyCode::Down => {
                                if app.page == Page::Network {
                                    let n = app.snapshot.nics.len();
                                    if n > 0 {
                                        app.network_sel = (app.network_sel + 1) % n;
                                    }
                                } else if app.page == Page::Storage {
                                    let n = app.snapshot.disks.len();
                                    if n > 0 {
                                        app.storage_sel = (app.storage_sel + 1) % n;
                                    }
                                }
                            }
                            KeyCode::Up => {
                                if app.page == Page::Network {
                                    let n = app.snapshot.nics.len();
                                    if n > 0 {
                                        app.network_sel = (app.network_sel + n - 1) % n;
                                    }
                                } else if app.page == Page::Storage {
                                    let n = app.snapshot.disks.len();
                                    if n > 0 {
                                        app.storage_sel = (app.storage_sel + n - 1) % n;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        crossterm::cursor::Show
    )?;
    disable_raw_mode()?;
    Ok(())
}

fn install_panic_hook(_production: bool) {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = crossterm::execute!(io::stderr(), LeaveAlternateScreen, crossterm::cursor::Show);
        prev(info);
    }));
}
