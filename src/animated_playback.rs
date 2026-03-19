use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crossterm::{cursor, execute, terminal};

use crate::{
    cli, frames,
    media_render::{compute_output_dims, render_frame},
    terminal_utils::{console_wh, status_row_under_video_from_ansi},
};

static RUNNING: AtomicBool = AtomicBool::new(true);

struct TerminalSessionGuard;

impl Drop for TerminalSessionGuard {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), terminal::LeaveAlternateScreen, cursor::Show);
    }
}

pub fn render_animated(args: &cli::Args, bg: (u8, u8, u8), media_type: frames::MediaType) {
    let path = &args.image;

    ctrlc::set_handler(|| RUNNING.store(false, Ordering::Relaxed))
        .expect("Failed to set Ctrl+C handler");

    let mut stdout = io::BufWriter::new(io::stdout());

    execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide).ok();
    let _terminal_guard = TerminalSessionGuard;

    let mut cache: Vec<(String, u64, u64, u64)> = Vec::new();
    let mut last_status_row: Option<u64> = None;

    let mut loop_count: u64 = 0;
    let playback_start = Instant::now();

    'outer: loop {
        if !RUNNING.load(Ordering::Relaxed) {
            break;
        }

        loop_count += 1;
        let (cw, ch) = console_wh();
        let rch = ch.saturating_sub(args.reserve).max(1);

        let cache_stale = cache.is_empty()
            || cache
                .first()
                .map(|(_, w, h, _)| *w != cw || *h != rch)
                .unwrap_or(true);

        let mut rendered_frames: u64 = 0;
        let mut cached_frames: u64 = 0;
        let mut dropped_frames: u64 = 0;
        let mut frames_shown: u64 = 0;
        let mut display_start: Option<Instant> = None;

        if cache_stale {
            if !cache.is_empty() {
                execute!(stdout, terminal::Clear(terminal::ClearType::All)).ok();
            }
            cache.clear();

            let stream = match frames::open_stream(path, &media_type, args.prefetch) {
                Ok(stream) => stream,
                Err(e) => {
                    eprintln!("{}", e);
                    break 'outer;
                }
            };

            if args.verbose {
                if args.precompute {
                    let msg = format!("Precomputing frames at {}x{}...", cw, rch);
                    let max_cols = cw.saturating_sub(1) as usize;
                    let msg: String = msg.chars().take(max_cols).collect();
                    let banner = format!("\x1b[1;1H\x1b[2K{}", msg);
                    let raw = stdout.get_mut();
                    raw.write_all(banner.as_bytes()).ok();
                    raw.flush().ok();
                } else {
                    eprintln!("Decoding and rendering frames at {}x{}...", cw, rch);
                }
            }

            let decode_start = Instant::now();

            for frame_result in stream {
                if !RUNNING.load(Ordering::Relaxed) {
                    break 'outer;
                }

                let (new_cw, new_ch) = console_wh();
                let new_rch = new_ch.saturating_sub(args.reserve).max(1);
                if new_cw != cw || new_rch != rch {
                    execute!(stdout, terminal::Clear(terminal::ClearType::All)).ok();
                    cache.clear();
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
                let (out_w, out_h) = compute_output_dims(
                    frame.width,
                    frame.height,
                    cw,
                    rch,
                    args.cell_width,
                    args.cell_height,
                );
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
                let delay_ms = if args.fps_limit > 0 {
                    frame.delay_ms.max(1000 / args.fps_limit.max(1))
                } else {
                    frame.delay_ms
                };

                if !args.no_cache {
                    cache.push((ansi.clone(), cw, rch, delay_ms));
                }
                rendered_frames += 1;

                if !args.precompute {
                    frames_shown += 1;

                    let mut buf = Vec::with_capacity(ansi.len() + 256);
                    buf.extend_from_slice(b"\x1b[H");
                    buf.extend_from_slice(ansi.as_bytes());

                    if args.verbose {
                        let elapsed_s = display_start
                            .get_or_insert_with(Instant::now)
                            .elapsed()
                            .as_secs_f64();
                        let actual_fps = if elapsed_s > 0.0 {
                            frames_shown as f64 / elapsed_s
                        } else {
                            0.0
                        };
                        let target_fps =
                            if delay_ms > 0 { 1000.0 / delay_ms as f64 } else { 0.0 };
                        let status = format!(
                            "frame {} | loop {} | FPS: {:.1}/{:.1} | render: {:.1}ms | rendered: {} dropped: {}",
                            frames_shown,
                            loop_count,
                            actual_fps,
                            target_fps,
                            render_ms,
                            rendered_frames,
                            dropped_frames,
                        );
                        let max_cols = cw.saturating_sub(1) as usize;
                        let status: String = status.chars().take(max_cols).collect();
                        let status_row = (out_h + 1).min(ch.max(1));
                        last_status_row = Some(status_row);
                        let _ = write!(buf, "\x1b[{};1H\x1b[2K{}", status_row, status);
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

        let should_play_from_cache = args.precompute || !cache_stale;
        if should_play_from_cache && !cache.is_empty() {
            display_start = None;
            let frame_count = cache.len();
            for (i, (ansi, _, _, delay_ms)) in cache.iter().enumerate() {
                if !RUNNING.load(Ordering::Relaxed) {
                    break 'outer;
                }

                let start = Instant::now();

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
                    let elapsed_s = display_start
                        .get_or_insert_with(Instant::now)
                        .elapsed()
                        .as_secs_f64();
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
                    let status_row = status_row_under_video_from_ansi(ansi, ch);
                    last_status_row = Some(status_row);
                    let _ = write!(buf, "\x1b[{};1H\x1b[2K{}", status_row, status);
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

    if args.verbose {
        let total = playback_start.elapsed();
        let frame_count = cache.len() as u64;
        let total_frames = loop_count * frame_count;
        let (_, ch) = console_wh();
        let status_row = last_status_row.unwrap_or(ch).min(ch.max(1));
        let clear = format!("\x1b[{};1H\x1b[2K", status_row);
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
}
