use std::io::Write;

use imagesize::blob_size;
use kitty_graphics_protocol::get_window_size;
use little_kitty::placeholder::placeholder_row;
use owo_colors::OwoColorize;

fn cols_rows_for_blob(data: &[u8]) -> Result<(u16, u16), String> {
    let imgsize = blob_size(data).map_err(|e| e.to_string())?;
    let ws = get_window_size().map_err(|e| e.to_string())?;

    if ws.cols == 0 || ws.rows == 0 || ws.width == 0 || ws.height == 0 {
        return Err("Terminal did not report pixel dimensions".into());
    }

    let cell_w = ws.width / ws.cols;
    let cell_h = ws.height / ws.rows;

    let imgw = u16::try_from(imgsize.width).map_err(|e| e.to_string())?;
    let imgh = u16::try_from(imgsize.height).map_err(|e| e.to_string())?;

    let cols = imgw.div_ceil(cell_w).max(1);
    let rows = imgh.div_ceil(cell_h).max(1);

    Ok((cols, rows))
}

fn uniq_id() -> u32 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};

    // Seeded from OS randomness
    let id = (RandomState::new().build_hasher().finish() as u32) & 0x00FF_FFFF;
    id.max(1) // ID 0 means no ID in the protocol, so it must never be used
}

pub fn display_img(data: &[u8]) -> Result<(), String> {
    let (c, r) = cols_rows_for_blob(data)?;
    let id = uniq_id();

    let cmd = kitty_graphics_protocol::Command::builder()
        .action(kitty_graphics_protocol::Action::TransmitAndDisplay)
        .format(kitty_graphics_protocol::ImageFormat::Png)
        .image_id(id)
        .quiet(2)
        .unicode_placeholder(c, r)
        .display_area(c as u32, r as u32)
        .build();

    let mut stdout = std::io::stdout().lock();
    for chunk in cmd.serialize_chunked(data).map_err(|e| e.to_string())? {
        stdout
            .write_all(chunk.as_bytes())
            .map_err(|e| e.to_string())?;
    }

    // The image ID is encoded as the fg color (id = R<<16 | G<<8 | B)
    let (red, green, blue) = ((id >> 16) as u8, (id >> 8) as u8, id as u8);
    for row in 0..r as usize {
        let line = placeholder_row(0, row, c as usize);
        writeln!(stdout, "{}", line.truecolor(red, green, blue)).map_err(|e| e.to_string())?;
    }
    stdout.flush().map_err(|e| e.to_string())?;
    Ok(())
}
