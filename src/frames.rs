use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};

pub struct Frame {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub delay_ms: u64,
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

pub fn decode_gif(path: &Path) -> Result<Vec<Frame>, String> {
    use image::codecs::gif::GifDecoder;
    use image::AnimationDecoder;
    use std::fs::File;
    use std::io::BufReader;

    let file = File::open(path).map_err(|e| format!("Failed to open '{}': {}", path.display(), e))?;
    let reader = BufReader::new(file);
    let decoder =
        GifDecoder::new(reader).map_err(|e| format!("Failed to decode GIF: {}", e))?;
    let frames_iter = decoder.into_frames();

    let mut frames = Vec::new();
    for frame_result in frames_iter {
        let frame = frame_result.map_err(|e| format!("Failed to decode frame: {}", e))?;
        let (numer, denom) = frame.delay().numer_denom_ms();
        let delay_ms = (numer as u64) / (denom as u64).max(1);
        let buffer = frame.into_buffer();
        let (w, h) = buffer.dimensions();
        frames.push(Frame {
            rgba: buffer.into_raw(),
            width: w,
            height: h,
            delay_ms: delay_ms.max(10),
        });
    }

    if frames.is_empty() {
        return Err("No frames found in GIF".to_string());
    }

    Ok(frames)
}

pub fn decode_video(path: &Path) -> Result<Vec<Frame>, String> {
    // Get video dimensions and frame rate via ffprobe
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
        .map_err(|_| {
            "ffmpeg/ffprobe not found. Install ffmpeg to play videos.".to_string()
        })?;

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

    // Parse frame rate (e.g. "30/1" or "30000/1001")
    let fps_parts: Vec<&str> = parts[2].split('/').collect();
    let fps = if fps_parts.len() == 2 {
        let num: f64 = fps_parts[0].parse().unwrap_or(30.0);
        let den: f64 = fps_parts[1].parse().unwrap_or(1.0);
        if den > 0.0 { num / den } else { 30.0 }
    } else {
        parts[2].parse::<f64>().unwrap_or(30.0)
    };

    let delay_ms = (1000.0 / fps).round().max(1.0) as u64;
    let frame_size = (width as usize) * (height as usize) * 4;

    // Decode frames via ffmpeg, piping raw RGBA to stdout
    let mut child = Command::new("ffmpeg")
        .arg("-i")
        .arg(path)
        .args(["-f", "rawvideo", "-pix_fmt", "rgba", "-v", "quiet", "pipe:1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "Failed to start ffmpeg".to_string())?;

    let mut stdout = child.stdout.take().unwrap();
    let mut frames = Vec::new();

    loop {
        let mut buf = vec![0u8; frame_size];
        let mut read = 0;
        while read < frame_size {
            match stdout.read(&mut buf[read..]) {
                Ok(0) => break,
                Ok(n) => read += n,
                Err(e) => return Err(format!("Read error: {}", e)),
            }
        }
        if read < frame_size {
            break;
        }
        frames.push(Frame {
            rgba: buf,
            width,
            height,
            delay_ms,
        });
    }

    child.wait().ok();

    if frames.is_empty() {
        return Err("No frames decoded from video".to_string());
    }

    Ok(frames)
}
