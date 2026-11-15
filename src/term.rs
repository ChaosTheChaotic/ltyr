use std::io::{self, Write};

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    execute, queue,
    style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor},
    terminal::{
        self, BeginSynchronizedUpdate, Clear, ClearType, EndSynchronizedUpdate,
        EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
    },
};
use imagesize::blob_size;
use kitty_graphics_protocol::{Action, Command, ImageFormat};
use little_kitty::placeholder::placeholder_row;

fn image_id() -> u32 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};

    // Seeded from OS randomness
    let id = (RandomState::new().build_hasher().finish() as u32) & 0x00FF_FFFF;
    id.max(1) // ID 0 means no ID in the protocol, so it must never be used
}

// Size of the image in terminal cells, shrunk (aspect ratio kept) so it fits
// the terminals width, and its height too when max_rows is given
// Re-queried on every call, so it follows terminal resizes and font changes
fn fit_cells(png: &[u8], max_rows: Option<u16>) -> Result<(u16, u16), String> {
    let img = blob_size(png).map_err(|e| e.to_string())?;
    let ws = terminal::window_size().map_err(|e| e.to_string())?;

    if ws.columns == 0 || ws.rows == 0 || ws.width == 0 || ws.height == 0 {
        return Err("Terminal did not report pixel dimensions".into());
    }

    let cell_w = f64::from(ws.width) / f64::from(ws.columns);
    let cell_h = f64::from(ws.height) / f64::from(ws.rows);

    let natural_cols = (img.width as f64 / cell_w).ceil().max(1.0);
    let natural_rows = (img.height as f64 / cell_h).ceil().max(1.0);

    let mut scale = (f64::from(ws.columns) / natural_cols).min(1.0);
    if let Some(max_rows) = max_rows {
        scale = scale.min(f64::from(max_rows.max(1)) / natural_rows);
    }

    let cols = (natural_cols * scale).floor().max(1.0) as u16;
    let rows = (natural_rows * scale).floor().max(1.0) as u16;
    Ok((cols, rows))
}

// Transmit the image and create a virtual placement. Nothing is drawn until
// the placeholder cells are printed. Re-sending with the same id replaces
// the image, so cells already on screen show the new data
fn queue_image(buf: &mut Vec<u8>, id: u32, png: &[u8], cols: u16, rows: u16) -> Result<(), String> {
    let cmd = Command::builder()
        .action(Action::TransmitAndDisplay)
        .format(ImageFormat::Png)
        .image_id(id)
        .quiet(2)
        .unicode_placeholder(cols, rows)
        .display_area(u32::from(cols), u32::from(rows))
        .build();

    for chunk in cmd.serialize_chunked(png).map_err(|e| e.to_string())? {
        buf.extend_from_slice(chunk.as_bytes());
    }
    Ok(())
}

fn queue_placeholder_row(buf: &mut Vec<u8>, id: u32, row: u16, cols: u16) -> io::Result<()> {
    // The image ID is encoded as the fg color (id = R<<16 | G<<8 | B)
    let color = Color::Rgb {
        r: (id >> 16) as u8,
        g: (id >> 8) as u8,
        b: id as u8,
    };
    queue!(
        buf,
        SetForegroundColor(color),
        Print(placeholder_row(0, usize::from(row), usize::from(cols))),
        ResetColor
    )
}

// One shot output
pub fn display_img(png: &[u8]) -> Result<(), String> {
    let (cols, rows) = fit_cells(png, None)?;
    let id = image_id();

    let mut buf = Vec::new();
    queue_image(&mut buf, id, png, cols, rows)?;
    for row in 0..rows {
        queue_placeholder_row(&mut buf, id, row, cols).map_err(|e| e.to_string())?;
        writeln!(buf).map_err(|e| e.to_string())?;
    }

    let mut out = io::stdout().lock();
    out.write_all(&buf)
        .and_then(|()| out.flush())
        .map_err(|e| e.to_string())
}

pub enum Body {
    Empty,
    Image(Vec<u8>),
    Error(String),
}

// Owns the terminal while alive
// raw mode + alternate screen + hidden cursor
// Dropping it puts everything back, so just let it go out of scope
// do NOT call std::process::exit while it is alive, it skips destructors
// The last terminal row is a status line, everything above it is body.
pub struct Screen {
    body: Body,
    status: String,
    image_id: u32,
}

impl Screen {
    pub fn enter() -> io::Result<Self> {
        install_panic_hook();
        enable_raw_mode()?;
        // From here on Drop undoes whatever got done, even if a later step fails.
        let screen = Self {
            body: Body::Empty,
            status: String::new(),
            image_id: image_id(),
        };
        execute!(io::stdout(), EnterAlternateScreen, Hide)?;
        Ok(screen)
    }

    pub fn set_body(&mut self, body: Body) {
        self.body = body;
    }

    pub fn set_status(&mut self, status: impl Into<String>) {
        self.status = status.into();
    }

    // Repaint in place
    // Every row is overwritten from the top (no full-screen clear first)
    // The frame is wrapped in a synchronized update so terminals that support it show it atomically, without flicker
    // Also the right thing to call after a resize.
    pub fn draw(&mut self) -> io::Result<()> {
        let size = terminal::window_size()?;
        let (cols, rows) = (size.columns, size.rows);
        let body_rows = rows.saturating_sub(1);

        let mut buf = Vec::new();
        queue!(buf, BeginSynchronizedUpdate)?;

        let used = match &self.body {
            Body::Empty => 0,
            Body::Error(msg) => queue_text(&mut buf, msg, cols, body_rows, Color::Red)?,
            Body::Image(png) => {
                let mark = buf.len();
                match self.queue_image_rows(&mut buf, png, body_rows) {
                    Ok(used) => used,
                    Err(msg) => {
                        buf.truncate(mark);
                        queue_text(&mut buf, &msg, cols, body_rows, Color::Red)?
                    }
                }
            }
        };

        // Wipe whatever the previous frame left below the content (this also clears the status row, which is repainted right after)
        if used < rows {
            queue!(buf, MoveTo(0, used), Clear(ClearType::FromCursorDown))?;
        }
        if rows > 0 {
            let line: String = self.status.chars().take(usize::from(cols)).collect();
            queue!(
                buf,
                MoveTo(0, rows - 1),
                SetAttribute(Attribute::Dim),
                Print(line),
                SetAttribute(Attribute::NormalIntensity)
            )?;
        }
        queue!(buf, EndSynchronizedUpdate)?;

        let mut out = io::stdout().lock();
        out.write_all(&buf)?;
        out.flush()
    }

    fn queue_image_rows(
        &self,
        buf: &mut Vec<u8>,
        png: &[u8],
        max_rows: u16,
    ) -> Result<u16, String> {
        let (cols, rows) = fit_cells(png, Some(max_rows))?;
        queue!(buf, MoveTo(0, 0)).map_err(|e| e.to_string())?;
        queue_image(buf, self.image_id, png, cols, rows)?;
        for row in 0..rows {
            // Clear the row before writing it
            // Clearing after a full-width row would hit the pending-wrap cell and eat its last character
            queue!(buf, MoveTo(0, row), Clear(ClearType::CurrentLine))
                .map_err(|e| e.to_string())?;
            queue_placeholder_row(buf, self.image_id, row, cols).map_err(|e| e.to_string())?;
        }
        Ok(rows)
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        restore();
    }
}

fn restore() {
    let _ = execute!(
        io::stdout(),
        EndSynchronizedUpdate,
        Show,
        LeaveAlternateScreen
    );
    let _ = disable_raw_mode();
}

// Panic messages must be printed on the normal screen in a sane terminal
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore();
        previous(info);
    }));
}

// Writes text line by line from the top, truncating (not wrapping) long
// lines so row accounting stays exact and LaTeXs caret lines stay aligned
// Returns how many rows it used
fn queue_text(
    buf: &mut Vec<u8>,
    text: &str,
    cols: u16,
    max_rows: u16,
    color: Color,
) -> io::Result<u16> {
    let mut y = 0;
    for line in text.lines().take(usize::from(max_rows)) {
        let line: String = line
            .replace('\t', "    ")
            .chars()
            .take(usize::from(cols))
            .collect();
        queue!(
            buf,
            MoveTo(0, y),
            Clear(ClearType::CurrentLine),
            SetForegroundColor(color),
            Print(line),
            ResetColor
        )?;
        y += 1;
    }
    Ok(y)
}
