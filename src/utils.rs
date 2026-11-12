use std::io::Write;

pub fn display_img(data: Vec<u8>) -> std::io::Result<()> {
    let cmd = kitty_graphics_protocol::Command::builder()
        .action(kitty_graphics_protocol::Action::TransmitAndDisplay)
        .format(kitty_graphics_protocol::ImageFormat::Png)
        .quiet(2)
        .build();

    let chunks = cmd.serialize_chunked(&data).unwrap(); // This doesent seem to return the Err variant

    let mut stdout = std::io::stdout().lock();
    for chunk in chunks {
        stdout.write_all(chunk.as_bytes())?;
    }
    stdout.flush()?;
    println!(); // Ensure trailing newline
    Ok(())
}

