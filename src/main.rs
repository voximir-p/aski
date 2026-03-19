mod cli;
mod render;

use std::io::{self, Write};
use terminal_size::{Height, Width, terminal_size};

fn console_wh() -> (u64, u64) {
    if let Some((Width(w), Height(h))) = terminal_size() {
        (w as u64, h as u64)
    } else {
        eprintln!("Failed to get terminal size, using default 80x24");
        (80, 24u64)
    }
}

fn main() {
    let args = cli::parse_args();
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
    // Cell aspect ratio: 10 wide × 22 tall
    let h_from_w = ((cw as f64 * ih as f64 * 10.0) / (iw as f64 * 22.0))
        .round()
        .max(1.0) as u64;

    let (out_w, out_h) = if h_from_w <= rch {
        (cw, h_from_w)
    } else {
        let w_from_h = ((rch as f64 * iw as f64 * 22.0) / (ih as f64 * 10.0))
            .round()
            .max(1.0)
            .min(cw as f64) as u64;
        (w_from_h, rch)
    };

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

    let (bg_r, bg_g, bg_b) = cli::parse_color(&args.background);

    let px = img.as_raw();

    let out = if args.opaque {
        render::render_opaque(px, iw, ih, out_w, out_h, bg_r, bg_g, bg_b)
    } else {
        render::render_alpha(px, iw, ih, out_w, out_h, bg_r, bg_g, bg_b)
    };

    let mut stdout = io::BufWriter::new(io::stdout());
    stdout.write_all(out.as_bytes()).unwrap();
}
