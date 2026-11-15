use std::{
    ffi::OsStr,
    fs, io,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
    thread,
    time::{Duration, Instant},
};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use notify::{EventKind, RecursiveMode, Watcher};

use crate::{
    Out,
    fmt::Fmt,
    term::{Body, Screen},
    utils::ImgFmt,
    watch_log,
    worker::{Job, RenderResult},
};

const DEBOUNCE: Duration = Duration::from_millis(50);
const REMOVE_GRACE: Duration = Duration::from_millis(500);

enum Msg {
    FsChanged,
    FsError(notify::Error),
    Term(Event),
    Rendered(u64, RenderResult),
}

struct WatchCtx {
    path: PathBuf,
    label: String,
    forced: Option<Fmt>,
    _watcher: Box<dyn Watcher>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
}

impl WatchCtx {
    fn new(path: PathBuf, forced: Option<Fmt>) -> Result<Self, String> {
        let name = path
            .file_name()
            .ok_or_else(|| format!("{} has no file name", path.display()))?
            .to_owned();
        // Watch the parent, not the file, an atomic save replaces the inode
        let dir = match path.parent() {
            Some(p) if !p.as_os_str().is_empty() => p,
            _ => Path::new("."),
        };

        let (tx, rx) = mpsc::channel();
        let fs_tx = tx.clone();
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            let msg = match res {
                Ok(ev) if concerns(&ev, &name) => Msg::FsChanged,
                Ok(_) => return,
                Err(e) => Msg::FsError(e),
            };
            let _ = fs_tx.send(msg);
        })
        .map_err(|e| e.to_string())?;
        watcher
            .watch(dir, RecursiveMode::NonRecursive)
            .map_err(|e| e.to_string())?;

        Ok(Self {
            label: path.display().to_string(),
            path,
            forced,
            _watcher: Box::new(watcher),
            tx,
            rx,
        })
    }
}

// What the loop delegates to, TuiHandler and StreamHandler implement this
trait Handler {
    fn img_fmt(&self) -> ImgFmt;
    fn watch_error(&mut self, ctx: &WatchCtx, err: &notify::Error);
    fn on_term(&mut self, _ev: &Event) -> Result<bool, String> {
        Ok(false)
    }
    fn on_render(&mut self, ctx: &WatchCtx, result: RenderResult, took: Duration);
    fn on_no_format(&mut self, ctx: &WatchCtx, err: &str);
    fn on_compiling(&mut self, ctx: &WatchCtx);
    fn on_spawn_error(&mut self, ctx: &WatchCtx, err: &io::Error);
    fn on_missing(&mut self, ctx: &WatchCtx);
    fn on_reappear(&mut self, ctx: &WatchCtx);
    fn on_read_error(&mut self, ctx: &WatchCtx, err: &io::Error);
    fn after_tick(&mut self) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(feature = "output")]
pub fn run(path: PathBuf, forced: Option<Fmt>, out: Option<Out>) -> Result<(), String> {
    if let Some(out) = out {
        return run_stream(path, forced, out);
    }
    run_tui(path, forced)
}

#[cfg(not(feature = "output"))]
pub fn run(path: PathBuf, forced: Option<Fmt>) -> Result<(), String> {
    run_tui(path, forced)
}

pub fn run_tui(path: PathBuf, forced: Option<Fmt>) -> Result<(), String> {
    let ctx = WatchCtx::new(path, forced)?;

    // Ctrl+C is delivered as a key event instead of SIGINT in raw mode.
    // Always get to run the cleanup in Screen::drop.
    let mut screen = Screen::enter().map_err(|e| e.to_string())?;

    let term_tx = ctx.tx.clone();
    thread::spawn(move || {
        while let Ok(ev) = event::read() {
            if term_tx.send(Msg::Term(ev)).is_err() {
                break;
            }
        }
    });

    let mut handler = TuiHandler {
        screen: &mut screen,
        dirty: true,
    };
    run_loop(ctx, &mut handler)
}

#[cfg(feature = "output")]
fn run_stream(path: PathBuf, forced: Option<Fmt>, out: Out) -> Result<(), String> {
    let ctx = WatchCtx::new(path, forced)?;
    watch_log!("watching {}", ctx.label);
    let mut handler = StreamHandler { out };
    run_loop(ctx, &mut handler)
}

fn run_loop<H: Handler>(ctx: WatchCtx, handler: &mut H) -> Result<(), String> {
    let mut job: Option<Job> = None;
    let mut next_id: u64 = 0;
    let mut started = Instant::now();
    let mut wake_at = Some(Instant::now()); // render once at startup
    let mut gone_since: Option<Instant> = None;

    loop {
        let first = match wake_at {
            Some(t) => match ctx
                .rx
                .recv_timeout(t.saturating_duration_since(Instant::now()))
            {
                Ok(msg) => Some(msg),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return Ok(()),
            },
            None => match ctx.rx.recv() {
                Ok(msg) => Some(msg),
                Err(_) => return Ok(()),
            },
        };

        for msg in first.into_iter().chain(ctx.rx.try_iter()) {
            match msg {
                Msg::FsChanged => wake_at = Some(Instant::now() + DEBOUNCE),
                Msg::FsError(e) => handler.watch_error(&ctx, &e),
                Msg::Term(ev) => {
                    if handler.on_term(&ev)? {
                        return Ok(());
                    }
                }
                Msg::Rendered(id, result) => {
                    // Result of a job we already cancelled
                    if job.as_ref().map(|j| j.id) != Some(id) {
                        continue;
                    }
                    job = None;
                    let took = started.elapsed();
                    handler.on_render(&ctx, result, took);
                }
            }
        }

        if wake_at.is_some_and(|t| Instant::now() >= t) {
            wake_at = None;
            // Anything in flight is stale now. Kill it before doing more work.
            drop(job.take());

            match fs::read_to_string(&ctx.path) {
                Ok(src) => {
                    // Tell handler file is back if it was missing
                    if gone_since.take().is_some() {
                        handler.on_reappear(&ctx);
                    }
                    started = Instant::now();
                    match Fmt::resolve(ctx.forced, &src, Some(ctx.path.as_path())) {
                        Err(e) => handler.on_no_format(&ctx, &e),
                        Ok(fmt) => {
                            next_id += 1;
                            let done_tx = ctx.tx.clone();
                            let spawned =
                                Job::spawn(next_id, fmt, handler.img_fmt(), src, move |id, res| {
                                    let _ = done_tx.send(Msg::Rendered(id, res));
                                });
                            match spawned {
                                Ok(j) => {
                                    job = Some(j);
                                    handler.on_compiling(&ctx);
                                }
                                Err(e) => handler.on_spawn_error(&ctx, &e),
                            }
                        }
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    // First detection vs later wake
                    let was_missing = gone_since.is_some();
                    let since = *gone_since.get_or_insert_with(Instant::now);
                    if since.elapsed() >= REMOVE_GRACE {
                        return Err(format!("Watched file {} was removed", ctx.label));
                    }
                    if !was_missing {
                        handler.on_missing(&ctx);
                    }
                    wake_at = Some(since + REMOVE_GRACE);
                }
                Err(e) => handler.on_read_error(&ctx, &e),
            }
        }

        handler.after_tick()?;
    }
}

struct TuiHandler<'a> {
    screen: &'a mut Screen,
    dirty: bool,
}

impl Handler for TuiHandler<'_> {
    fn img_fmt(&self) -> ImgFmt {
        ImgFmt::Png
    }

    fn watch_error(&mut self, ctx: &WatchCtx, e: &notify::Error) {
        self.screen
            .set_status(format!("{} — watch error: {e}", ctx.label));
        self.dirty = true;
    }

    fn on_term(&mut self, ev: &Event) -> Result<bool, String> {
        match ev {
            Event::Key(k) if k.kind == KeyEventKind::Press && is_quit(k) => Ok(true),
            Event::Resize(..) => {
                self.dirty = true;
                Ok(false)
            }
            _ => Ok(false),
        }
    }

    fn on_render(&mut self, ctx: &WatchCtx, result: RenderResult, took: Duration) {
        match result {
            Ok(png) => {
                self.screen.set_body(Body::Image(png));
                self.screen
                    .set_status(format!("{} — rendered in {took:.0?}", ctx.label));
            }
            Err(e) => {
                self.screen.set_body(Body::Error(e));
                self.screen
                    .set_status(format!("{} — failed after {took:.0?}", ctx.label));
            }
        }
        self.dirty = true;
    }

    fn on_no_format(&mut self, ctx: &WatchCtx, e: &str) {
        self.screen.set_body(Body::Error(e.to_owned()));
        self.screen.set_status(format!("{} — no format", ctx.label));
        self.dirty = true;
    }

    fn on_compiling(&mut self, ctx: &WatchCtx) {
        self.screen
            .set_status(format!("{} — compiling…", ctx.label));
    }

    fn on_spawn_error(&mut self, ctx: &WatchCtx, e: &io::Error) {
        self.screen
            .set_body(Body::Error(format!("Failed to start renderer: {e}")));
        self.screen.set_status(format!("{} — error", ctx.label));
        self.dirty = true;
    }

    fn on_missing(&mut self, ctx: &WatchCtx) {
        self.screen.set_status(format!("{} — missing…", ctx.label));
        self.dirty = true;
    }

    fn on_reappear(&mut self, _ctx: &WatchCtx) {
        // Status is updated by on_compiling/on_render, which will follow
    }

    fn on_read_error(&mut self, ctx: &WatchCtx, e: &io::Error) {
        self.screen
            .set_body(Body::Error(format!("Cannot read {}: {e}", ctx.label)));
        self.screen.set_status(format!("{} — error", ctx.label));
        self.dirty = true;
    }

    fn after_tick(&mut self) -> Result<(), String> {
        if self.dirty {
            self.screen.draw().map_err(|e| e.to_string())?;
            self.dirty = false;
        }
        Ok(())
    }
}

#[cfg(feature = "output")]
struct StreamHandler {
    out: Out,
}

#[cfg(feature = "output")]
impl Handler for StreamHandler {
    fn img_fmt(&self) -> ImgFmt {
        self.out.fmt
    }

    fn watch_error(&mut self, ctx: &WatchCtx, e: &notify::Error) {
        watch_log!("{}: watcher error: {e}", ctx.label);
    }

    fn on_render(&mut self, ctx: &WatchCtx, result: RenderResult, took: Duration) {
        match result {
            Ok(bytes) => match write_out(&self.out.path, &bytes) {
                Ok(()) => watch_log!(
                    "{}: rendered in {took:.0?} -> {}",
                    ctx.label,
                    self.out.path.display()
                ),
                Err(e) => watch_log!(
                    "{}: write to {} failed: {e}",
                    ctx.label,
                    self.out.path.display()
                ),
            },
            Err(e) => {
                watch_log!("{}: error: {e}", ctx.label);
                watch_log!("{}: failed after {took:.0?}", ctx.label);
            }
        }
    }

    fn on_no_format(&mut self, ctx: &WatchCtx, e: &str) {
        watch_log!("{}: {e}", ctx.label);
    }

    fn on_compiling(&mut self, ctx: &WatchCtx) {
        watch_log!("{}: compiling…", ctx.label);
    }

    fn on_spawn_error(&mut self, ctx: &WatchCtx, e: &io::Error) {
        watch_log!("{}: failed to start renderer: {e}", ctx.label);
    }

    fn on_missing(&mut self, ctx: &WatchCtx) {
        watch_log!("{}: missing…", ctx.label);
    }

    fn on_reappear(&mut self, ctx: &WatchCtx) {
        watch_log!("{}: reappeared", ctx.label);
    }

    fn on_read_error(&mut self, ctx: &WatchCtx, e: &io::Error) {
        watch_log!("{}: cannot read: {e}", ctx.label);
    }
}

#[cfg(feature = "output")]
fn write_out(path: &Path, data: &[u8]) -> io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    f.write_all(data)?;
    f.flush()
}

#[inline]
fn concerns(ev: &notify::Event, name: &OsStr) -> bool {
    !matches!(ev.kind, EventKind::Access(_))
        && (ev.paths.is_empty() || ev.paths.iter().any(|p| p.file_name() == Some(name)))
}

#[inline]
fn is_quit(k: &KeyEvent) -> bool {
    matches!(k.code, KeyCode::Char('q') | KeyCode::Esc)
        || (k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL))
}
