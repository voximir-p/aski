use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::thread::JoinHandle;
use std::time::Duration;

pub struct Frame {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub delay: Duration,
}

pub enum MediaType {
    Static,
    Gif,
    Video,
}

pub fn detect_media_type(path: &Path) -> MediaType {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .as_deref()
    {
        Some("gif") => MediaType::Gif,
        Some(
            "mp4" | "mkv" | "avi" | "mov" | "webm" | "flv" | "wmv" | "m4v" | "ts" | "ogv",
        ) => MediaType::Video,
        _ => MediaType::Static,
    }
}

/// Opens a lazy frame stream for the given media file.
/// Frames are decoded one at a time; no raw pixel data is held in memory
/// beyond the single frame currently being processed.
/// `prefetch` controls the number of video frames buffered ahead by the
/// background decode thread; it has no effect on GIFs or static images.
pub fn open_stream(
    path: &Path,
    media_type: &MediaType,
    prefetch: usize,
) -> Result<Box<dyn Iterator<Item = Result<Frame, String>>>, String> {
    match media_type {
        MediaType::Gif => open_gif_stream(path),
        MediaType::Video => open_video_stream(path, prefetch),
        MediaType::Static => unreachable!(),
    }
}

fn open_gif_stream(
    path: &Path,
) -> Result<Box<dyn Iterator<Item = Result<Frame, String>>>, String> {
    use image::codecs::gif::GifDecoder;
    use image::AnimationDecoder;
    use std::fs::File;
    use std::io::BufReader;

    let file = File::open(path)
        .map_err(|e| format!("Failed to open '{}': {}", path.display(), e))?;
    let reader = BufReader::new(file);
    let decoder =
        GifDecoder::new(reader).map_err(|e| format!("Failed to decode GIF: {}", e))?;

    let iter = decoder.into_frames().map(|frame_result| {
        let frame = frame_result.map_err(|e| format!("Failed to decode frame: {}", e))?;
        let (numer, denom) = frame.delay().numer_denom_ms();
        let delay_ms = (numer as f64) / (denom as f64).max(1.0);
        let buffer = frame.into_buffer();
        let (w, h) = buffer.dimensions();
        Ok(Frame {
            rgba: buffer.into_raw(),
            width: w,
            height: h,
            delay: Duration::from_secs_f64((delay_ms.max(10.0)) / 1000.0),
        })
    });

    Ok(Box::new(iter))
}

/// Streams raw RGBA frames from ffmpeg one at a time.
struct VideoFrameStream {
    rx: Receiver<Result<Frame, String>>,
    stop_tx: Option<SyncSender<()>>,
    worker: Option<JoinHandle<()>>,
}

impl Iterator for VideoFrameStream {
    type Item = Result<Frame, String>;

    fn next(&mut self) -> Option<Self::Item> {
        self.rx.recv().ok()
    }
}

impl Drop for VideoFrameStream {
    fn drop(&mut self) {
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(());
        }
        if let Some(worker) = self.worker.take() {
            std::thread::spawn(move || {
                let _ = worker.join();
            });
        }
    }
}

fn open_video_stream(
    path: &Path,
    prefetch: usize,
) -> Result<Box<dyn Iterator<Item = Result<Frame, String>>>, String> {
    // Probe dimensions and frame rate before opening the decode pipe.
    let probe = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,r_frame_rate",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .map_err(|_| "ffmpeg/ffprobe not found. Install ffmpeg to play videos.".to_string())?;

    if !probe.status.success() {
        return Err(format!(
            "ffprobe failed: {}",
            String::from_utf8_lossy(&probe.stderr)
        ));
    }

    let info = String::from_utf8_lossy(&probe.stdout);
    let parts: Vec<&str> = info.trim().split(',').collect();
    if parts.len() < 3 {
        return Err("Failed to read video info from ffprobe".to_string());
    }

    let width: u32 = parts[0]
        .parse()
        .map_err(|_| format!("Invalid video width: {}", parts[0]))?;
    let height: u32 = parts[1]
        .parse()
        .map_err(|_| format!("Invalid video height: {}", parts[1]))?;

    let frame_delay = parse_frame_delay(parts[2]);
    let frame_size = (width as usize) * (height as usize) * 4;

    // Spawn ffmpeg and stream raw RGBA frames through the pipe.
    let mut child = Command::new("ffmpeg")
        .arg("-nostdin")
        .arg("-i")
        .arg(path)
        .args(["-f", "rawvideo", "-pix_fmt", "rgba", "-v", "quiet", "pipe:1"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "Failed to start ffmpeg".to_string())?;

    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Failed to capture ffmpeg stdout".to_string())?;

    let (tx, rx) = sync_channel::<Result<Frame, String>>(prefetch.max(1));
    let (stop_tx, stop_rx) = sync_channel::<()>(1);

    let worker = std::thread::spawn(move || {
        let mut sent_any = false;
        loop {
            if stop_rx.try_recv().is_ok() {
                break;
            }

            let mut buf = vec![0u8; frame_size];
            let mut read = 0;
            while read < frame_size {
                match stdout.read(&mut buf[read..]) {
                    Ok(0) => break,
                    Ok(n) => read += n,
                    Err(e) => {
                        let _ = tx.send(Err(format!("Read error: {}", e)));
                        let _ = child.kill();
                        let _ = child.wait();
                        return;
                    }
                }
            }

            if read < frame_size {
                break;
            }

            if tx
                .send(Ok(Frame {
                    rgba: buf,
                    width,
                    height,
                    delay: frame_delay,
                }))
                .is_err()
            {
                break;
            }
            sent_any = true;
        }

        if !sent_any {
            let _ = tx.send(Err("No frames decoded from video".to_string()));
        }
        let _ = child.kill();
        let _ = child.wait();
    });

    Ok(Box::new(VideoFrameStream {
        rx,
        stop_tx: Some(stop_tx),
        worker: Some(worker),
    }))
}

fn parse_frame_delay(rate: &str) -> Duration {
    let default_fps = 30.0;

    if let Some((num_s, den_s)) = rate.split_once('/') {
        let num = num_s.parse::<u64>().ok();
        let den = den_s.parse::<u64>().ok();
        if let (Some(num), Some(den)) = (num, den) {
            if num > 0 {
                let nanos = ((den as u128) * 1_000_000_000u128) / (num as u128);
                let nanos = nanos.max(1).min(u64::MAX as u128) as u64;
                return Duration::from_nanos(nanos);
            }
        }
    }

    let fps = rate.parse::<f64>().ok().filter(|v| *v > 0.0).unwrap_or(default_fps);
    Duration::from_secs_f64(1.0 / fps)
}
