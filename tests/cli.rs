use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture(PathBuf);

impl Fixture {
    fn new(extension: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self(std::env::temp_dir().join(format!(
            "aski-test-{}-{}.{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
            extension
        )))
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_aski"))
            .arg(&self.0)
            .args(args)
            .output()
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn gif(path: &Path) {
    let file = std::fs::File::create(path).unwrap();
    let mut encoder = image::codecs::gif::GifEncoder::new(file);
    for (color, ms) in [([255, 0, 0, 255], 10), ([0, 0, 255, 255], 30)] {
        encoder
            .encode_frame(image::Frame::from_parts(
                image::RgbaImage::from_pixel(2, 2, image::Rgba(color)),
                0,
                0,
                image::Delay::from_numer_denom_ms(ms, 1),
            ))
            .unwrap();
    }
}

#[test]
fn tiny_static_images_render_with_final_newline() {
    let file = Fixture::new("png");
    image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]))
        .save(&file.0)
        .unwrap();
    let result = file.run(&[]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = String::from_utf8(result.stdout).unwrap();
    assert!(output.contains("\x1b[38;2;255;0;0m"));
    assert!(output.ends_with("\x1b[0m\n"));
    assert!(output.matches('█').count() > 1);
}

#[test]
fn invalid_media_and_arguments_fail_without_panicking() {
    for ext in ["png", "gif", "mp4"] {
        let file = Fixture::new(ext);
        std::fs::write(&file.0, b"not media").unwrap();
        let result = file.run(&[]);
        assert!(!result.status.success());
        assert!(!String::from_utf8_lossy(&result.stderr).contains("panicked"));
    }
    let file = Fixture::new("png");
    for args in [
        vec!["--background", "#éa"],
        vec!["--cell-width", "0"],
        vec!["--precompute", "--no-cache"],
    ] {
        assert_eq!(file.run(&args).status.code(), Some(2));
    }
}

#[test]
fn gif_streaming_precompute_and_no_cache_show_each_frame_once() {
    let file = Fixture::new("gif");
    gif(&file.0);
    for args in [
        vec!["--verbose"],
        vec!["--verbose", "--precompute"],
        vec!["--verbose", "--no-cache"],
    ] {
        let result = file.run(&args);
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(result.status.success(), "{stderr}");
        assert!(stderr.contains("2 displayed frames"), "{stderr}");
        let output = String::from_utf8(result.stdout).unwrap();
        assert_eq!(output.matches("\x1b[38;2;255;0;0m").count(), 1);
        assert_eq!(output.matches("\x1b[38;2;0;0;255m").count(), 1);
    }
}

// Explicitly invoked by CI with ffmpeg installed. Kept separate so the normal
// test suite also works for library users without external video tools.
#[test]
#[ignore = "requires ffmpeg and ffprobe; run cargo test --test cli -- --include-ignored"]
fn video_decode_and_early_drop_with_full_and_unbuffered_queues() {
    let file = Fixture::new("mkv");
    let result = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=red:s=16x8:r=25:d=0.2",
            "-c:v",
            "ffv1",
            "-y",
        ])
        .arg(&file.0)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    for prefetch in [0, 1, 8] {
        let frames: Vec<_> = aski::open_stream(&file.0, &aski::MediaType::Video, prefetch)
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(frames.len(), 5);
        assert!(
            frames
                .iter()
                .all(|f| (f.width, f.height, f.rgba.len()) == (16, 8, 512))
        );
        let mut stream = aski::open_stream(&file.0, &aski::MediaType::Video, prefetch).unwrap();
        assert!(stream.next().unwrap().is_ok());
        std::thread::sleep(std::time::Duration::from_millis(50));
        let start = std::time::Instant::now();
        drop(stream);
        assert!(start.elapsed() < std::time::Duration::from_secs(2));
    }
    let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut stream = aski::frames::open_stream_with_cancel(
        &file.0,
        &aski::MediaType::Video,
        1,
        cancelled.clone(),
    )
    .unwrap();
    cancelled.store(true, Ordering::Relaxed);
    assert!(stream.next().is_none());
    drop(stream);
    let result = file.run(&["--verbose", "--precompute"]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(String::from_utf8_lossy(&result.stderr).contains("5 displayed frames"));
}

#[cfg(unix)]
#[test]
fn decoder_exit_errors_and_truncated_frames_are_not_silent() {
    use std::os::unix::fs::PermissionsExt;
    let directory = Fixture::new("tools");
    std::fs::create_dir(&directory.0).unwrap();
    let probe = directory.0.join("ffprobe");
    std::fs::write(
        &probe,
        "#!/bin/sh\nprintf 'width=1\\nheight=1\\navg_frame_rate=100/1\\n'\n",
    )
    .unwrap();
    std::fs::set_permissions(&probe, std::fs::Permissions::from_mode(0o755)).unwrap();
    let media = Fixture::new("mp4");
    std::fs::write(&media.0, b"fixture").unwrap();
    let decoder = directory.0.join("ffmpeg");
    for (script, expected) in [
        (
            "#!/bin/sh\nprintf '\\377\\000\\000\\377'\nexit 7\n",
            "ffmpeg failed",
        ),
        ("#!/bin/sh\nprintf '\\377\\000'\n", "truncated RGBA frame"),
        ("#!/bin/sh\nexit 0\n", "No frames decoded"),
    ] {
        std::fs::write(&decoder, script).unwrap();
        std::fs::set_permissions(&decoder, std::fs::Permissions::from_mode(0o755)).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_aski"))
            .arg(&media.0)
            .env("PATH", &directory.0)
            .output()
            .unwrap();
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains(expected), "{error}");
    }
    std::fs::remove_dir_all(&directory.0).unwrap();
}

#[test]
fn closed_stdout_exits_quietly() {
    let file = Fixture::new("png");
    image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]))
        .save(&file.0)
        .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_aski"))
        .arg(&file.0)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
}
