use std::io::{self, Write};

use crate::{cli, terminal_utils::console_wh};
use aski::{Renderer, compute_output_dims};

pub fn render_static(args: &cli::Args) -> io::Result<()> {
    let img = image::open(&args.image)
        .map_err(|e| io::Error::other(format!("Failed to open '{}': {e}", args.image.display())))?
        .to_rgba8();
    let (iw, ih) = img.dimensions();
    let (cw, ch) = console_wh();
    let rch = ch.saturating_sub(args.reserve).max(1);
    let (out_w, out_h) = compute_output_dims(iw, ih, cw, rch, args.cell_width, args.cell_height);
    if args.verbose {
        eprintln!("Image: {iw}x{ih}px | terminal: {cw}x{ch} | output: {out_w}x{out_h}");
    }
    let mut renderer = Renderer::new(iw, ih, out_w, out_h, args.background, args.opaque)
        .map_err(io::Error::other)?;
    let output = renderer.render(img.as_raw()).map_err(io::Error::other)?;
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    stdout.write_all(output.as_bytes())?;
    // Always put the shell prompt on a fresh line, and report flush errors.
    stdout.write_all(b"\n")?;
    stdout.flush()
}
