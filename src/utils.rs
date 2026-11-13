use std::io::Write;

pub fn display_img(data: &[u8]) -> Result<(), String> {
    let cmd = kitty_graphics_protocol::Command::builder()
        .action(kitty_graphics_protocol::Action::TransmitAndDisplay)
        .format(kitty_graphics_protocol::ImageFormat::Png)
        .quiet(2)
        .unicode_placeholder(0, 0)
        .build();

    let chunks = cmd.serialize_chunked(&data).unwrap(); // This doesent seem to return the Err variant

    let mut stdout = std::io::stdout().lock();
    for chunk in chunks {
        stdout.write_all(chunk.as_bytes()).map_err(|e| e.to_string())?;
    }
    stdout.flush().map_err(|e| e.to_string())?;
    println!(); // Ensure trailing newline
    Ok(())
}

