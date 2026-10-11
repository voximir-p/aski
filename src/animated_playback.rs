use std::io::{self, IsTerminal, Write};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use aski::{MediaType, Renderer, compute_output_dims, frames};
use crossterm::{cursor, execute, style, terminal};

use crate::{cli, terminal_utils::console_wh};

struct TerminalSession {
    active: bool,
}

impl TerminalSession {
    fn enter() -> io::Result<Self> {
        let session = Self {
            active: io::stdout().is_terminal(),
        };
        if session.active {
            execute!(
                io::stdout(),
                terminal::EnterAlternateScreen,
                cursor::Hide,
                terminal::DisableLineWrap,
                terminal::Clear(terminal::ClearType::All)
            )?;
        }
        Ok(session)
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        if self.active {
            let _ = execute!(
                io::stdout(),
                style::ResetColor,
                terminal::EnableLineWrap,
                cursor::Show,
                terminal::LeaveAlternateScreen
            );
        }
    }
}

struct RenderedFrame<T = String> {
    ansi: T,
    height: u64,
    delay: Duration,
}

#[derive(Default)]
struct PlaybackStats {
    shown: u64,
    rendered: u64,
    cached: u64,
    late: u64,
    render_ms: f64,
    loop_number: u64,
    from_cache: bool,
    started: Option<Instant>,
    last_presented: Option<Instant>,
}

impl PlaybackStats {
    fn fps(&self) -> f64 {
        match (self.started, self.last_presented) {
            (Some(start), Some(last)) if last > start => {
                self.shown.saturating_sub(1) as f64 / last.duration_since(start).as_secs_f64()
            }
            _ => 0.0,
        }
    }
}

// Add the CURRENT frame's duration to the previous boundary. This matters for
// variable-delay GIFs. If rendering falls behind, reset in O(1) rather than
// repeatedly advancing a deadline or claiming that frames have been dropped.
fn frame_deadline(previous: Option<Instant>, now: Instant, delay: Duration) -> (Instant, bool) {
    let deadline = previous.unwrap_or(now).checked_add(delay).unwrap_or(now);
    (deadline.max(now), deadline < now)
}

fn wait_for_frame(deadline: &mut Option<Instant>, delay: Duration, cancelled: &AtomicBool) -> bool {
    let (next, late) = frame_deadline(*deadline, Instant::now(), delay);
    *deadline = Some(next);
    while !cancelled.load(Ordering::Relaxed) {
        let remaining = next.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        std::thread::sleep(remaining.min(Duration::from_millis(25)));
    }
    late
}

fn available_rows(args: &cli::Args, terminal_height: u64) -> u64 {
    terminal_height
        .saturating_sub(args.reserve.max(u64::from(args.verbose)))
        .max(1)
}

fn status_line(stats: &PlaybackStats, delay: Duration, width: u64, verbose: bool) -> String {
    let mode = if stats.from_cache { "CACHE" } else { "PLAY" };
    let columns = width.saturating_sub(1) as usize;
    let basic = format!(
        "{mode}  frame {}  |  loop {}  |  Ctrl+C stop",
        stats.shown, stats.loop_number
    );
    if verbose {
        let fps = 1.0 / delay.as_secs_f64();
        let detailed = format!(
            "{mode}  {}  |  {:.1}/{fps:.1} FPS  |  {:.1}ms render  |  {} rendered / {} cached / {} late  |  Ctrl+C stop",
            stats.shown,
            stats.fps(),
            stats.render_ms,
            stats.rendered,
            stats.cached,
            stats.late
        );
        let compact = format!(
            "{mode}  {}  |  {:.1}/{fps:.1} FPS  |  {} late  |  Ctrl+C stop",
            stats.shown,
            stats.fps(),
            stats.late
        );
        for candidate in [detailed, compact] {
            if candidate.len() <= columns {
                return candidate;
            }
        }
    }
    if basic.len() <= columns {
        return basic;
    }
    let compact = format!("{mode}  {}  |  Ctrl+C stop", stats.shown);
    if compact.len() <= columns {
        return compact;
    }
    let minimal = format!("{mode}  Ctrl+C");
    minimal[..minimal.len().min(columns)].to_owned()
}

fn show_progress(
    output: &mut impl Write,
    width: u64,
    frames: u64,
    bytes: usize,
    elapsed: Duration,
) -> io::Result<()> {
    let columns = width.saturating_sub(1) as usize;
    let detailed = format!(
        "PREPARING  {frames} frames  |  {:.1} MiB  |  {:.1}s  |  Ctrl+C stop",
        bytes as f64 / (1024.0 * 1024.0),
        elapsed.as_secs_f64()
    );
    let compact = format!("PREPARING  {frames}  |  Ctrl+C stop");
    let message = if detailed.len() <= columns {
        detailed
    } else {
        compact
    };
    write!(
        output,
        "\x1b[H\x1b[2K{}",
        &message[..message.len().min(columns)]
    )?;
    output.flush()
}

fn present(
    output: &mut impl Write,
    frame: &RenderedFrame<impl AsRef<str>>,
    terminal: (u64, u64),
    verbose: bool,
    interactive: bool,
    stats: &mut PlaybackStats,
) -> io::Result<()> {
    output.write_all(b"\x1b[H")?;
    output.write_all(frame.ansi.as_ref().as_bytes())?;
    let displayed_at = Instant::now();
    stats.started.get_or_insert(displayed_at);
    stats.last_presented = Some(displayed_at);
    stats.shown += 1;
    if (verbose || interactive) && frame.height < terminal.1 {
        let status = status_line(stats, frame.delay, terminal.0, verbose);
        write!(output, "\x1b[{};1H\x1b[2K{status}", frame.height + 1)?;
    }
    output.flush()
}

pub fn render_animated(
    args: &cli::Args,
    media_type: MediaType,
    cancelled: Arc<AtomicBool>,
) -> io::Result<()> {
    let session = TerminalSession::enter()?;
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    let mut cache: Vec<RenderedFrame> = Vec::new();
    let mut cache_terminal = None;
    let mut stats = PlaybackStats::default();
    let mut completed_loops = 0;
    let cache_enabled = !args.no_cache && (args.loop_playback || args.precompute);

    'playback: while !cancelled.load(Ordering::Relaxed) {
        let terminal = console_wh();
        stats.loop_number = completed_loops + 1;
        let rows = available_rows(args, terminal.1);
        let mut deadline = None;
        if cache_terminal != Some(terminal) {
            cache.clear();
            cache_terminal = None;
        }
        if cache.is_empty() {
            let prepare_start = Instant::now();
            let mut last_progress = prepare_start;
            let mut cache_bytes = 0;
            if args.precompute && session.active {
                show_progress(&mut stdout, terminal.0, 0, 0, Duration::ZERO)?;
            }
            let stream = frames::open_stream_with_cancel(
                &args.image,
                &media_type,
                args.prefetch,
                Arc::clone(&cancelled),
            )
            .map_err(io::Error::other)?;
            let mut renderer = None;
            let mut geometry = None;
            let mut frame_count = 0;
            for frame in stream {
                if cancelled.load(Ordering::Relaxed) {
                    break 'playback;
                }
                if console_wh() != terminal {
                    execute!(stdout, terminal::Clear(terminal::ClearType::All))?;
                    cache.clear();
                    continue 'playback;
                }
                let frame = frame.map_err(io::Error::other)?;
                let (ow, oh) = compute_output_dims(
                    frame.width,
                    frame.height,
                    terminal.0,
                    rows,
                    args.cell_width,
                    args.cell_height,
                );
                let new_geometry = (frame.width, frame.height, ow, oh);
                if geometry != Some(new_geometry) {
                    renderer = Some(
                        Renderer::new(
                            frame.width,
                            frame.height,
                            ow,
                            oh,
                            args.background,
                            args.opaque,
                        )
                        .map_err(io::Error::other)?,
                    );
                    geometry = Some(new_geometry);
                    execute!(stdout, terminal::Clear(terminal::ClearType::All))?;
                }
                let render_start = Instant::now();
                let ansi = renderer
                    .as_mut()
                    .unwrap()
                    .render(&frame.rgba)
                    .map_err(io::Error::other)?;
                stats.render_ms = render_start.elapsed().as_secs_f64() * 1000.0;
                let min_delay = if args.fps_limit == 0 {
                    Duration::ZERO
                } else {
                    Duration::from_secs_f64(1.0 / args.fps_limit as f64)
                };
                let rendered = RenderedFrame {
                    ansi,
                    height: oh,
                    delay: frame.delay.max(min_delay),
                };
                frame_count += 1;
                stats.rendered += 1;
                if !args.precompute {
                    stats.from_cache = false;
                    present(
                        &mut stdout,
                        &rendered,
                        terminal,
                        args.verbose,
                        session.active,
                        &mut stats,
                    )?;
                    if wait_for_frame(&mut deadline, rendered.delay, &cancelled) {
                        stats.late += 1;
                    }
                }
                if cache_enabled {
                    cache_bytes += rendered.ansi.len();
                    cache.push(RenderedFrame {
                        ansi: rendered.ansi.to_owned(),
                        height: rendered.height,
                        delay: rendered.delay,
                    });
                }
                if args.precompute
                    && session.active
                    && (frame_count == 1 || last_progress.elapsed() >= Duration::from_millis(100))
                {
                    show_progress(
                        &mut stdout,
                        terminal.0,
                        frame_count,
                        cache_bytes,
                        prepare_start.elapsed(),
                    )?;
                    last_progress = Instant::now();
                }
            }
            if cancelled.load(Ordering::Relaxed) {
                break;
            }
            if frame_count == 0 {
                return Err(io::Error::other("No frames decoded from media"));
            }
            cache_terminal = Some(terminal);
            if !args.precompute {
                completed_loops += 1;
                if !args.loop_playback {
                    break;
                }
                continue;
            }
        }
        if args.precompute {
            execute!(stdout, terminal::Clear(terminal::ClearType::All))?;
        }
        let mut last_height = None;
        for frame in &cache {
            if cancelled.load(Ordering::Relaxed) {
                break 'playback;
            }
            if console_wh() != terminal {
                execute!(stdout, terminal::Clear(terminal::ClearType::All))?;
                cache.clear();
                cache_terminal = None;
                continue 'playback;
            }
            if last_height.is_some_and(|h| h != frame.height) {
                execute!(stdout, terminal::Clear(terminal::ClearType::All))?;
            }
            last_height = Some(frame.height);
            stats.cached += 1;
            stats.from_cache = true;
            stats.render_ms = 0.0;
            present(
                &mut stdout,
                frame,
                terminal,
                args.verbose,
                session.active,
                &mut stats,
            )?;
            if wait_for_frame(&mut deadline, frame.delay, &cancelled) {
                stats.late += 1;
            }
        }
        completed_loops += 1;
        if !args.loop_playback {
            break;
        }
    }
    stdout.flush()?;
    drop(stdout);
    drop(session);
    if args.verbose {
        eprintln!(
            "Playback finished: {completed_loops} complete loops, {} displayed frames | rendered: {} cached: {} late: {} | {:.1} avg FPS",
            stats.shown,
            stats.rendered,
            stats.cached,
            stats.late,
            stats.fps()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variable_delays_use_current_frame_duration() {
        let start = Instant::now();
        let (first, late) = frame_deadline(None, start, Duration::from_millis(20));
        assert!(!late);
        let (second, late) = frame_deadline(
            Some(first),
            start + Duration::from_millis(25),
            Duration::from_millis(100),
        );
        assert!(!late);
        assert_eq!(second.duration_since(start), Duration::from_millis(120));
        let now = start + Duration::from_secs(100);
        assert_eq!(
            frame_deadline(Some(second), now, Duration::from_nanos(1)),
            (now, true)
        );
    }
    #[test]
    fn status_adapts_to_small_terminals() {
        let stats = PlaybackStats {
            shown: 123,
            loop_number: 2,
            ..Default::default()
        };
        for width in [1, 2, 10, 25, 40, 80, 160] {
            let line = status_line(&stats, Duration::from_millis(40), width, true);
            assert!(line.len() < width as usize);
            if width >= 25 {
                assert!(line.contains("Ctrl+C"));
            }
        }
        assert!(status_line(&stats, Duration::from_millis(40), 80, true).contains("FPS"));
        assert!(!status_line(&stats, Duration::from_millis(40), 80, false).contains("FPS"));
    }
    #[test]
    fn summary_fps_uses_display_intervals_not_final_frame_hold() {
        let start = Instant::now();
        let stats = PlaybackStats {
            shown: 4,
            started: Some(start),
            last_presented: Some(start + Duration::from_secs(3)),
            ..Default::default()
        };
        assert_eq!(stats.fps(), 1.0);
        assert_eq!(PlaybackStats::default().fps(), 0.0);
    }
}
