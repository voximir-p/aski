use std::io::{self, Write};

use crate::{
    cli,
    media_render::{compute_output_dims, render_frame},
    terminal_utils::console_wh,
};

pub fn render_static(args: &cli::Args, bg: (u8, u8, u8)) {
    let path = &args.image;
    let img = image::open(path)
        .unwrap_or_else(|e| {
            eprintln!("Failed to open '{}': {}", path.display(), e);
            std::process::exit(1);
        })
        .to_rgba8();

    let (iw, ih) = img.dimensions();
    let (cw, ch) = console_wh();
    let rch = ch.saturating_sub(args.reserve).max(1);
    let (out_w, out_h) = compute_output_dims(iw, ih, cw, rch, args.cell_width, args.cell_height);
    let cells = (out_w * out_h) as usize;

    if args.verbose {
        eprintln!("Image size     {}x{} (px)", iw, ih);
        eprintln!("Terminal size  {}x{}", cw, ch);
        eprintln!("Reserved size  {}x{}", cw, rch);
        eprintln!("Output size    {}x{}", out_w, out_h);
        eprintln!("Cell count     {}", cells);
    }

    if (cells == 0) || (out_w > iw as u64) || (out_h > ih as u64) {
        eprintln!("Terminal too small to render image");
        std::process::exit(1);
    }

    let px = img.as_raw();
    let out = render_frame(px, iw, ih, out_w, out_h, bg, args.opaque);

    let mut stdout = io::BufWriter::new(io::stdout());
    stdout.write_all(out.as_bytes()).unwrap();
}
