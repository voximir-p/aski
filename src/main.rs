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

    let mut stdout = io::BufWriter::new(io::stdout());

    // Hide cursor for clean playback
    execute!(stdout, cursor::Hide).ok();

    // Cache of rendered frames: (ansi_string, render_w, render_h, delay_ms).
    // Built lazily during the first playback pass; reused on every subsequent
    // loop. Raw RGBA data is never kept beyond the single frame being rendered.
    let mut cache: Vec<(String, u64, u64, u64)> = Vec::new();

    let mut loop_count: u64 = 0;
    let playback_start = Instant::now();

    'outer: loop {
        if !RUNNING.load(Ordering::Relaxed) {
            break;
        }

        loop_count += 1;
        let loop_start = Instant::now();
        let (cw, ch) = console_wh();
        let rch = ch.saturating_sub(args.reserve).max(1);

        // The cache is stale when it is empty (first run) or was built at a
        // different terminal size (resize occurred between loops).
        let cache_stale = cache.is_empty()
            || cache
                .first()
                .map(|(_, w, h, _)| *w != cw || *h != rch)
                .unwrap_or(true);

        let mut rendered_frames: u64 = 0;
        let mut cached_frames: u64 = 0;
        let mut dropped_frames: u64 = 0;
        let mut frames_shown: u64 = 0;

        if cache_stale {
            if !cache.is_empty() {
                // Screen was built at the old size; clear it before redrawing.
                execute!(stdout, terminal::Clear(terminal::ClearType::All)).ok();
            }
            cache.clear();

            let stream = frames::open_stream(path, &media_type).unwrap_or_else(|e| {
                eprintln!("{}", e);
                std::process::exit(1);
            });

            if args.verbose {
                if args.precompute {
                    eprintln!("Precomputing frames at {}x{}...", cw, rch);
                } else {
                    eprintln!("Decoding and rendering frames at {}x{}...", cw, rch);
                }
            }

            let decode_start = Instant::now();

            for frame_result in stream {
                if !RUNNING.load(Ordering::Relaxed) {
                    break 'outer;
                }

                // If the terminal was resized while we were decoding/rendering,
                // throw away what we've built and restart at the new size.
                let (new_cw, new_ch) = console_wh();
                let new_rch = new_ch.saturating_sub(args.reserve).max(1);
                if new_cw != cw || new_rch != rch {
                    execute!(stdout, terminal::Clear(terminal::ClearType::All)).ok();
                    cache.clear();
                    // Dropping `stream` here kills the ffmpeg child process.
                    continue 'outer;
                }

                let frame = match frame_result {
                    Ok(f) => f,
                    Err(e) => {
                        eprintln!("Frame decode error: {}", e);
                        break 'outer;
                    }
                };

                let frame_start = Instant::now();
                let (out_w, out_h) = compute_output_dims(frame.width, frame.height, cw, rch);
                let render_start = Instant::now();
                let ansi = render_frame(
                    &frame.rgba,
                    frame.width,
                    frame.height,
                    out_w,
                    out_h,
                    bg,
                    args.opaque,
                );
                let render_ms = render_start.elapsed().as_secs_f64() * 1000.0;
                let delay_ms = frame.delay_ms;
                // `frame` (and its RGBA buffer) is dropped here.

                cache.push((ansi, cw, rch, delay_ms));
                rendered_frames += 1;

                // In non-precompute mode display each frame as it is rendered
                // so the first pass doubles as live playback.
                if !args.precompute {
                    frames_shown += 1;
                    let i = cache.len() - 1;
                    let ansi = &cache[i].0;

                    let mut buf = Vec::with_capacity(ansi.len() + 256);
                    buf.extend_from_slice(b"\x1b[H");
                    buf.extend_from_slice(ansi.as_bytes());

                    if args.verbose {
                        let elapsed_s = loop_start.elapsed().as_secs_f64();
                        let actual_fps = if elapsed_s > 0.0 {
                            frames_shown as f64 / elapsed_s
                        } else {
                            0.0
                        };
                        let target_fps =
                            if delay_ms > 0 { 1000.0 / delay_ms as f64 } else { 0.0 };
                        let status = format!(
                            "frame {} | loop {} | FPS: {:.1}/{:.1} | render: {:.1}ms | rendered: {} dropped: {}",
                            i + 1,
                            loop_count,
                            actual_fps,
                            target_fps,
                            render_ms,
                            rendered_frames,
                            dropped_frames,
                        );
                        let max_cols = cw.saturating_sub(1) as usize;
                        let status: String = status.chars().take(max_cols).collect();
                        let _ = write!(buf, "\x1b[{};1H\x1b[2K{}", ch, status);
                    }

                    let raw = stdout.get_mut();
                    raw.write_all(&buf).ok();
                    raw.flush().ok();

                    let elapsed = frame_start.elapsed();
                    let target = Duration::from_millis(delay_ms);
                    if elapsed < target {
                        std::thread::sleep(target - elapsed);
                    } else {
                        dropped_frames += 1;
                    }
                }
            }

            if args.verbose && args.precompute {
                let elapsed = decode_start.elapsed();
                let secs = elapsed.as_secs_f64();
                eprintln!(
                    "Precomputation complete: {} frames in {:.2}s ({:.1} frames/s)",
                    cache.len(),
                    secs,
                    if secs > 0.0 { cache.len() as f64 / secs } else { 0.0 },
                );
            }
        }

        // Play from the ANSI cache.
        // This covers: precompute mode (first loop), all subsequent loops, and
        // every loop after a resize-triggered rebuild.
        let should_play_from_cache = args.precompute || !cache_stale;
        if should_play_from_cache && !cache.is_empty() {
            let frame_count = cache.len();
            for (i, (ansi, _, _, delay_ms)) in cache.iter().enumerate() {
                if !RUNNING.load(Ordering::Relaxed) {
                    break 'outer;
                }

                let start = Instant::now();

                // Detect a resize mid-loop; restart the outer loop at the new size.
                let (cur_cw, cur_ch) = console_wh();
                let cur_rch = cur_ch.saturating_sub(args.reserve).max(1);
                if cur_cw != cw || cur_rch != rch {
                    execute!(stdout, terminal::Clear(terminal::ClearType::All)).ok();
                    cache.clear();
                    continue 'outer;
                }

                let delay_ms = *delay_ms;
                cached_frames += 1;
                frames_shown += 1;

                let mut buf = Vec::with_capacity(ansi.len() + 256);
                buf.extend_from_slice(b"\x1b[H");
                buf.extend_from_slice(ansi.as_bytes());

                if args.verbose {
                    let elapsed_s = loop_start.elapsed().as_secs_f64();
                    let actual_fps = if elapsed_s > 0.0 {
                        frames_shown as f64 / elapsed_s
                    } else {
                        0.0
                    };
                    let target_fps =
                        if delay_ms > 0 { 1000.0 / delay_ms as f64 } else { 0.0 };
                    let status = format!(
                        "frame {}/{} | loop {} | FPS: {:.1}/{:.1} | cache | cached: {} rendered: {} dropped: {}",
                        i + 1,
                        frame_count,
                        loop_count,
                        actual_fps,
                        target_fps,
                        cached_frames,
                        rendered_frames,
                        dropped_frames,
                    );
                    let max_cols = cw.saturating_sub(1) as usize;
                    let status: String = status.chars().take(max_cols).collect();
                    let _ = write!(buf, "\x1b[{};1H\x1b[2K{}", ch, status);
                }

                let raw = stdout.get_mut();
                raw.write_all(&buf).ok();
                raw.flush().ok();

                let elapsed = start.elapsed();
                let target = Duration::from_millis(delay_ms);
                if elapsed < target {
                    std::thread::sleep(target - elapsed);
                } else {
                    dropped_frames += 1;
                }
            }
        }

        if !args.loop_playback {
            break;
        }
    }

    // Verbose final summary
    if args.verbose {
        let total = playback_start.elapsed();
        let frame_count = cache.len() as u64;
        let total_frames = loop_count * frame_count;
        let (_, ch) = console_wh();
        let clear = format!("\x1b[{};1H\x1b[2K", ch);
        let raw = stdout.get_mut();
        raw.write_all(clear.as_bytes()).ok();
        raw.flush().ok();
        eprintln!(
            "Playback finished: {} loops, {} total frames in {:.2}s ({:.1} avg FPS)",
            loop_count,
            total_frames,
            total.as_secs_f64(),
            if total.as_secs_f64() > 0.0 {
                total_frames as f64 / total.as_secs_f64()
            } else {
                0.0
            }
        );
    }

    // Cleanup: show cursor
    execute!(stdout, cursor::Show).ok();
}
