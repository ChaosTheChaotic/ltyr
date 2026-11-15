use std::{
    any::Any,
    io::{self, Read, Write},
    process::{Child, Command, ExitCode, Stdio},
    thread,
};

use crate::{
    fmt::{Fmt, render},
    utils::ImgFmt,
};

pub const CHILD_FLAG: &str = "render-child";
pub const CHILD_IMG_FLAG: &str = "render-child-img";

// First byte of the childs stdout says what the rest is.
const TAG_OK: u8 = 0;
const TAG_ERR: u8 = 1;

pub type RenderResult = Result<Vec<u8>, String>;

// Child side, stdin = source, stdout = TAG + (PNG bytes | error text)

pub fn run_child(fmt: &Fmt, img: &ImgFmt) -> ExitCode {
    let mut src = String::new();
    let result = match io::stdin().read_to_string(&mut src) {
        Err(e) => Err(format!("Failed to read input: {e}")),
        Ok(_) => std::panic::catch_unwind(|| render(fmt, &src, img))
            .unwrap_or_else(|p| Err(panic_message(&p))),
    };

    let mut out = io::stdout().lock();
    let written = match result {
        Ok(png) => out.write_all(&[TAG_OK]).and_then(|()| out.write_all(&png)),
        Err(msg) => out
            .write_all(&[TAG_ERR])
            .and_then(|()| out.write_all(msg.as_bytes())),
    }
    .and_then(|()| out.flush());

    if written.is_ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn panic_message(payload: &Box<dyn Any + Send>) -> String {
    let msg = payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".into());
    format!("Renderer panicked: {msg}")
}

pub struct Job {
    pub id: u64,
    child: Child,
}

impl Job {
    // Starts rendering src.
    // done called from bg thread with job id and result or after a kill with meaningless result
    // cmp ids and ignore stale
    pub fn spawn(
        id: u64,
        fmt: Fmt,
        img_fmt: ImgFmt,
        src: String,
        done: impl FnOnce(u64, RenderResult) + Send + 'static,
    ) -> io::Result<Self> {
        let mut child = Command::new(std::env::current_exe()?)
            .arg(format!("--{CHILD_FLAG}"))
            .arg(fmt.name())
            .arg(format!("--{CHILD_IMG_FLAG}"))
            .arg(img_fmt.name())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        let mut stdin = child.stdin.take().expect("stdin is piped");
        let mut stdout = child.stdout.take().expect("stdout is piped");

        // Big source must not block on a full pipe
        // Dropping stdin at end closes it which is childs EOF
        thread::spawn(move || {
            let _ = stdin.write_all(src.as_bytes());
        });

        thread::spawn(move || {
            let mut buf = Vec::new();
            let result = match stdout.read_to_end(&mut buf) {
                Ok(_) => decode(buf),
                Err(e) => Err(format!("Failed to read renderer output: {e}")),
            };
            done(id, result);
        });

        Ok(Self { id, child })
    }
}

impl Drop for Job {
    // Dropping a job cancels it. Also reaps the process, so no zombies
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn decode(mut buf: Vec<u8>) -> RenderResult {
    if buf.is_empty() {
        return Err("Renderer process exited without producing output".into());
    }
    match buf.remove(0) {
        TAG_OK => Ok(buf),
        TAG_ERR => Err(String::from_utf8_lossy(&buf).into_owned()),
        _ => Err("Renderer process produced malformed output".into()),
    }
}
