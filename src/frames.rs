use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// Largest supported number of queued raw video frames. Zero uses a rendezvous queue.
pub const MAX_PREFETCH: usize = 1024;

/// An error opening media, probing a video, or decoding a frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeError(String);

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DecodeError {}

/// Lazy sequence of decoded animation frames.
pub type FrameStream = Box<dyn Iterator<Item = Result<Frame, DecodeError>>>;

/// One decoded, tightly packed RGBA8 frame and its display duration.
#[derive(Debug)]
pub struct Frame {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub delay: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
        Some("mp4" | "mkv" | "avi" | "mov" | "webm" | "flv" | "wmv" | "m4v" | "ts" | "ogv") => {
            MediaType::Video
        }
        _ => MediaType::Static,
    }
}

/// Opens a lazy frame stream for the given media file.
/// GIFs decode one frame at a time. Videos retain at most `prefetch` queued
/// frames, plus frames being decoded and consumed.
/// `prefetch` controls the number of video frames buffered ahead by the
/// background decode thread (0 through [`MAX_PREFETCH`]); it has no effect on GIFs.
/// Passing [`MediaType::Static`] returns an error.
pub fn open_stream(
    path: &Path,
    media_type: &MediaType,
    prefetch: usize,
) -> Result<FrameStream, DecodeError> {
    open_stream_with_cancel(path, media_type, prefetch, Arc::new(AtomicBool::new(false)))
}

/// Open a stream with cooperative cancellation. Set `cancelled` to true to stop
/// waiting for video frames. Dropping the stream kills and reaps ffmpeg and joins
/// the decoder worker, including when its bounded queue is full.
pub fn open_stream_with_cancel(
    path: &Path,
    media_type: &MediaType,
    prefetch: usize,
    cancelled: Arc<AtomicBool>,
) -> Result<FrameStream, DecodeError> {
    if cancelled.load(Ordering::Relaxed) {
        return Ok(Box::new(std::iter::empty()));
    }
    match media_type {
        MediaType::Gif => open_gif_stream(path),
        MediaType::Video => open_video_stream(path, prefetch, cancelled),
        MediaType::Static => Err("Static images must be opened with an image decoder".into()),
    }
    .map(|stream| Box::new(stream.map(|frame| frame.map_err(DecodeError))) as FrameStream)
    .map_err(DecodeError)
}

fn open_gif_stream(path: &Path) -> Result<Box<dyn Iterator<Item = Result<Frame, String>>>, String> {
    use image::AnimationDecoder;
    use image::codecs::gif::GifDecoder;
    use std::fs::File;
    use std::io::BufReader;

    let file =
        File::open(path).map_err(|e| format!("Failed to open '{}': {}", path.display(), e))?;
    let reader = BufReader::new(file);
    let decoder = GifDecoder::new(reader).map_err(|e| format!("Failed to decode GIF: {}", e))?;

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

/// Owns both the process and worker so cancellation does not leak either.
struct VideoFrameStream {
    rx: Option<Receiver<Result<Frame, String>>>,
    child: Arc<Mutex<Child>>,
    worker: Option<JoinHandle<()>>,
    cancelled: Arc<AtomicBool>,
}

impl Iterator for VideoFrameStream {
    type Item = Result<Frame, String>;

    fn next(&mut self) -> Option<Self::Item> {
        while !self.cancelled.load(Ordering::Relaxed) {
            match self.rx.as_ref()?.recv_timeout(Duration::from_millis(25)) {
                Ok(frame) => return Some(frame),
                Err(RecvTimeoutError::Disconnected) => return None,
                Err(RecvTimeoutError::Timeout) => {}
            }
        }
        None
    }
}

impl Drop for VideoFrameStream {
    fn drop(&mut self) {
        // Disconnect first: this releases a worker blocked sending to a full queue.
        self.rx.take();
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        if let Ok(mut child) = self.child.lock() {
            let _ = child.wait();
        }
    }
}

fn video_info(info: &str) -> Result<(u32, u32, Duration), String> {
    let field = |name: &str| {
        info.lines().find_map(|line| {
            let (key, value) = line.split_once('=')?;
            (key == name).then_some(value.trim())
        })
    };
    let dimension = |name| {
        field(name)
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|&v| v > 0)
            .ok_or_else(|| format!("Missing or invalid video {name}"))
    };
    let width = dimension("width")?;
    let height = dimension("height")?;
    let delay = field("avg_frame_rate")
        .and_then(frame_delay)
        .or_else(|| field("r_frame_rate").and_then(frame_delay))
        .unwrap_or(Duration::from_nanos(1_000_000_000 / 30));
    Ok((width, height, delay))
}

fn open_video_stream(
    path: &Path,
    prefetch: usize,
    cancelled: Arc<AtomicBool>,
) -> Result<Box<dyn Iterator<Item = Result<Frame, String>>>, String> {
    if prefetch > MAX_PREFETCH {
        return Err(format!("Prefetch must be between 0 and {MAX_PREFETCH}"));
    }
    // Absolute local paths cannot accidentally become ffmpeg options or URLs.
    let path = path
        .canonicalize()
        .map_err(|e| format!("Failed to open '{}': {e}", path.display()))?;
    let probe = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,avg_frame_rate,r_frame_rate",
            "-of",
            "default=noprint_wrappers=1",
        ])
        .arg(&path)
        .output()
        .map_err(|e| format!("Failed to start ffprobe (install ffmpeg to play videos): {e}"))?;
    if !probe.status.success() {
        return Err(format!(
            "ffprobe failed: {}",
            String::from_utf8_lossy(&probe.stderr).trim()
        ));
    }
    let (width, height, delay) = video_info(&String::from_utf8_lossy(&probe.stdout))?;
    let frame_size = (width as usize)
        .checked_mul(height as usize)
        .and_then(|v| v.checked_mul(4))
        .filter(|&n| n <= isize::MAX as usize)
        .ok_or("Video dimensions are too large")?;

    let mut child = Command::new("ffmpeg")
        // Keep raw frame geometry consistent with ffprobe, including rotated files.
        .args(["-nostdin", "-noautorotate", "-i"])
        .arg(&path)
        .args([
            "-map",
            "0:v:0",
            "-an",
            "-sn",
            "-dn",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-v",
            "error",
            "-fps_mode",
            "passthrough",
            "pipe:1",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to start ffmpeg: {e}"))?;
    let mut stdout = child.stdout.take().expect("stdout was configured as piped");
    let child = Arc::new(Mutex::new(child));
    let worker_child = Arc::clone(&child);
    let (tx, rx) = sync_channel(prefetch);
    let worker = std::thread::spawn(move || {
        let mut sent_any = false;
        loop {
            let mut buf = Vec::new();
            if let Err(e) = buf.try_reserve_exact(frame_size) {
                let _ = tx.send(Err(format!("Cannot allocate video frame: {e}")));
                break;
            }
            buf.resize(frame_size, 0);
            let mut read = 0;
            let mut read_error = None;
            while read < frame_size {
                match stdout.read(&mut buf[read..]) {
                    Ok(0) => break,
                    Ok(n) => read += n,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(e) => {
                        read_error = Some(format!("Video read error: {e}"));
                        break;
                    }
                }
            }
            if let Some(error) = read_error {
                let _ = tx.send(Err(error));
                break;
            }
            if read != frame_size {
                if read != 0 {
                    let _ = tx.send(Err("ffmpeg returned a truncated RGBA frame".into()));
                } else {
                    // Poll without holding the process lock while waiting; Drop
                    // must always be able to kill a decoder during cancellation.
                    let status = loop {
                        let status = worker_child.lock().unwrap().try_wait();
                        match status {
                            Ok(Some(status)) => break Ok(status),
                            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                            Err(e) => break Err(e),
                        }
                    };
                    match status {
                        Ok(status) if !status.success() => {
                            let _ = tx.send(Err(format!("ffmpeg failed with {status}")));
                        }
                        Err(e) => {
                            let _ = tx.send(Err(format!("Failed to wait for ffmpeg: {e}")));
                        }
                        _ if !sent_any => {
                            let _ = tx.send(Err("No frames decoded from video".into()));
                        }
                        _ => {}
                    }
                }
                break;
            }
            if tx
                .send(Ok(Frame {
                    rgba: buf,
                    width,
                    height,
                    delay,
                }))
                .is_err()
            {
                break;
            }
            sent_any = true;
        }
        let mut child = worker_child.lock().unwrap();
        let _ = child.kill();
        let _ = child.wait();
    });
    Ok(Box::new(VideoFrameStream {
        rx: Some(rx),
        child,
        worker: Some(worker),
        cancelled,
    }))
}

fn frame_delay(rate: &str) -> Option<Duration> {
    let fps = if let Some((num, den)) = rate.split_once('/') {
        let num = num.parse::<f64>().ok()?;
        let den = den.parse::<f64>().ok()?;
        if den <= 0.0 {
            return None;
        }
        num / den
    } else {
        rate.parse::<f64>().ok()?
    };
    if !fps.is_finite() || fps <= 0.0 {
        return None;
    }
    Duration::try_from_secs_f64(1.0 / fps)
        .ok()
        .filter(|d| !d.is_zero())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_fields_are_order_independent_and_rates_have_fallbacks() {
        let (w, h, delay) =
            video_info("r_frame_rate=25/1\nheight=1080\navg_frame_rate=0/0\nwidth=1920\n").unwrap();
        assert_eq!((w, h), (1920, 1080));
        assert_eq!(delay, Duration::from_millis(40));
        assert!(video_info("width=0\nheight=1080").is_err());
        assert!(video_info("").is_err());
        assert_eq!(frame_delay("30000/1001").unwrap().as_nanos(), 33_366_667);
        for rate in ["NaN", "inf", "0", "-1", "1/0", "0/0", "1e-300"] {
            assert!(frame_delay(rate).is_none(), "{rate}");
        }
    }

    #[test]
    fn media_extensions_are_case_insensitive() {
        assert_eq!(detect_media_type(Path::new("x.GIF")), MediaType::Gif);
        assert_eq!(detect_media_type(Path::new("x.MP4")), MediaType::Video);
        assert_eq!(detect_media_type(Path::new("x.png")), MediaType::Static);
        assert!(open_stream(Path::new("x.png"), &MediaType::Static, 0).is_err());
    }
}
