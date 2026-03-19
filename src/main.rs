mod cli;
mod frames;
mod render;

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use crossterm::{cursor, execute, terminal};
use terminal_size::{Height, Width, terminal_size};

static RUNNING: AtomicBool = AtomicBool::new(true);

fn console_wh() -> (u64, u64) {
    if let Some((Width(w), Height(h))) = terminal_size() {
        (w as u64, h as u64)
    } else {
        (80, 24u64)
    }
}

fn compute_output_dims(iw: u32, ih: u32, tw: u64, th: u64) -> (u64, u64) {
    let h_from_w = ((tw as f64 * ih as f64 * 10.0) / (iw as f64 * 22.0))
        .round()
        .max(1.0) as u64;
    if h_from_w <= th {
        (tw, h_from_w)
    } else {
        let w_from_h = ((th as f64 * iw as f64 * 22.0) / (ih as f64 * 10.0))
            .round()
            .max(1.0)
            .min(tw as f64) as u64;
        (w_from_h, th)
    }
}

fn render_frame(
    rgba: &[u8],
    iw: u32,
    ih: u32,
    out_w: u64,
    out_h: u64,
    bg: (u8, u8, u8),
    opaque: bool,
) -> String {
    if opaque {
        render::render_opaque(rgba, iw, ih, out_w, out_h, bg.0, bg.1, bg.2)
    } else {
        render::render_alpha(rgba, iw, ih, out_w, out_h, bg.0, bg.1, bg.2)
    }
}

fn main() {
    let args = cli::parse_args();
    let path = &args.image;
    let bg = cli::parse_color(&args.background);

    match frames::detect_media_type(path) {
        frames::MediaType::Static => render_static(&args, bg),
        media_type => render_animated(&args, bg, media_type),
    }
}

fn render_static(args: &cli::Args, bg: (u8, u8, u8)) {
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
    let (out_w, out_h) = compute_output_dims(iw, ih, cw, rch);
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

fn render_animated(args: &cli::Args, bg: (u8, u8, u8), media_type: frames::MediaType) {
    let path = &args.image;

    ctrlc::set_handler(|| RUNNING.store(false, Ordering::Relaxed))
        .expect("Failed to set Ctrl+C handler");

    // Decode all source frames into RGBA
    if args.verbose {
        eprintln!("Decoding frames...");
    }
    let decoded = match media_type {
        frames::MediaType::Gif => frames::decode_gif(path),
        frames::MediaType::Video => frames::decode_video(path),
        frames::MediaType::Static => unreachable!(),
    };
    let decoded = decoded.unwrap_or_else(|e| {
        eprintln!("{}", e);
        std::process::exit(1);
    });
    if args.verbose {
        eprintln!("Decoded {} frames", decoded.len());
    }

    let frame_count = decoded.len();
    let mut stdout = io::BufWriter::new(io::stdout());

    // Hide cursor for clean playback
    execute!(stdout, cursor::Hide).ok();

    // Cache: each entry holds (rendered_ansi, terminal_w, terminal_h) it was rendered at
    let mut cache: Vec<Option<(String, u64, u64)>> = vec![None; frame_count];

    // Precompute all rendered frames upfront if requested
    if args.precompute {
        let (cw, ch) = console_wh();
        let rch = ch.saturating_sub(args.reserve).max(1);
        if args.verbose {
            eprintln!("Precomputing {} frames at {}x{}...", frame_count, cw, rch);
        }
        let precompute_start = Instant::now();
        for (i, frame) in decoded.iter().enumerate() {
            if !RUNNING.load(Ordering::Relaxed) {
                break;
            }
            let (out_w, out_h) = compute_output_dims(frame.width, frame.height, cw, rch);
            let ansi = render_frame(&frame.rgba, frame.width, frame.height, out_w, out_h, bg, args.opaque);
            cache[i] = Some((ansi, cw, rch));
        }
        if args.verbose {
            let elapsed = precompute_start.elapsed();
            eprintln!(
                "Precomputation complete in {:.2}s ({:.1} frames/s)",
                elapsed.as_secs_f64(),
                frame_count as f64 / elapsed.as_secs_f64()
            );
        }
    }

    let mut last_tw: u64 = 0;
    let mut last_th: u64 = 0;
    let mut loop_count: u64 = 0;
    let playback_start = Instant::now();
    let target_fps = if decoded[0].delay_ms > 0 {
        1000.0 / decoded[0].delay_ms as f64
    } else {
        0.0
    };

    // Playback loop
    loop {
        if !RUNNING.load(Ordering::Relaxed) {
            break;
        }

        loop_count += 1;
        let loop_start = Instant::now();
        let mut rendered_frames: u64 = 0;
        let mut cached_frames: u64 = 0;
        let mut dropped_frames: u64 = 0;
        let mut frames_shown: u64 = 0;

        for (i, frame) in decoded.iter().enumerate() {
            if !RUNNING.load(Ordering::Relaxed) {
                break;
            }

            let start = Instant::now();

            // Check current terminal size
            let (cw, ch) = console_wh();
            let rch = ch.saturating_sub(args.reserve).max(1);

            // On resize: clear screen and invalidate cache
            if cw != last_tw || rch != last_th {
                if last_tw != 0 {
                    execute!(stdout, terminal::Clear(terminal::ClearType::All)).ok();
                    for c in cache.iter_mut() {
                        *c = None;
                    }
                }
                last_tw = cw;
                last_th = rch;
            }

            // Render (or use cache)
            let needs_render = match &cache[i] {
                Some((_, tw, th)) => *tw != cw || *th != rch,
                None => true,
            };
            let render_start = Instant::now();
            if needs_render {
                let (out_w, out_h) = compute_output_dims(frame.width, frame.height, cw, rch);
                let ansi = render_frame(
                    &frame.rgba,
                    frame.width,
                    frame.height,
                    out_w,
                    out_h,
                    bg,
                    args.opaque,
                );
                cache[i] = Some((ansi, cw, rch));
                rendered_frames += 1;
            } else {
                cached_frames += 1;
            }
            let render_ms = render_start.elapsed().as_secs_f64() * 1000.0;

            let ansi = &cache[i].as_ref().unwrap().0;
            frames_shown += 1;

            // Build entire output into one buffer and write in a single call
            // to prevent partial flushes / terminal scrolling
            let mut buf = Vec::with_capacity(ansi.len() + 256);
            buf.extend_from_slice(b"\x1b[H"); // cursor home
            buf.extend_from_slice(ansi.as_bytes());

            if args.verbose {
                let elapsed_s = loop_start.elapsed().as_secs_f64();
                let actual_fps = if elapsed_s > 0.0 {
                    frames_shown as f64 / elapsed_s
                } else {
                    0.0
                };
                let src = if needs_render { "render" } else { "cache" };
                let status = format!(
                    "frame {}/{} | loop {} | FPS: {:.1}/{:.1} | {}: {:.1}ms | cached: {} rendered: {} dropped: {}",
                    i + 1,
                    frame_count,
                    loop_count,
                    actual_fps,
                    target_fps,
                    src,
                    render_ms,
                    cached_frames,
                    rendered_frames,
                    dropped_frames,
                );
                let max_cols = cw.saturating_sub(1) as usize;
                let status = if max_cols == 0 {
                    String::new()
                } else {
                    status.chars().take(max_cols).collect::<String>()
                };
                let _ = write!(
                    buf,
                    "\x1b[{};1H\x1b[2K{}",
                    ch,
                    status,
                );
            }

            // Write directly to raw stdout, bypassing BufWriter
            let raw = stdout.get_mut();
            raw.write_all(&buf).ok();
            raw.flush().ok();

            // Respect frame delay
            let elapsed = start.elapsed();
            let target = Duration::from_millis(frame.delay_ms);
            if elapsed < target {
                std::thread::sleep(target - elapsed);
            } else {
                dropped_frames += 1;
            }
        }

        if !args.loop_playback {
            break;
        }
    }

    // Verbose final summary
    if args.verbose {
        let total = playback_start.elapsed();
        let total_frames = loop_count * frame_count as u64;
        // Clear the stats bar
        let (_, ch) = console_wh();
        let clear = format!("\x1b[{};1H\x1b[2K", ch);
        let raw = stdout.get_mut();
        raw.write_all(clear.as_bytes()).ok();
        raw.flush().ok();
        eprintln!(
            "Playback finished: {} loops, {} total frames in {:.2}s ({:.1} avg FPS)",
            loop_count, total_frames, total.as_secs_f64(),
            if total.as_secs_f64() > 0.0 {
                total_frames as f64 / total.as_secs_f64()
            } else { 0.0 }
        );
    }

    // Cleanup: show cursor
    execute!(stdout, cursor::Show).ok();
}
