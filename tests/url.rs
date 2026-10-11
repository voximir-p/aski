use std::io::{Cursor, Read, Write};
use std::net::TcpListener;
use std::process::{Command, Output};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

fn png() -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        1,
        1,
        image::Rgba([255, 0, 0, 255]),
    ))
    .write_to(&mut bytes, image::ImageFormat::Png)
    .unwrap();
    bytes.into_inner()
}

fn response(status: &str, headers: &str, body: &[u8]) -> Vec<u8> {
    let mut result = format!(
        "HTTP/1.1 {status}\r\nConnection: close\r\nContent-Length: {}\r\n{headers}\r\n",
        body.len()
    )
    .into_bytes();
    result.extend_from_slice(body);
    result
}

// A deterministic loopback HTTP server; no external network is needed. Bound
// waits keep a regression in URL parsing from hanging the test suite.
fn server(replies: Vec<(&str, Vec<u8>)>) -> (String, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let replies: Vec<_> = replies
        .into_iter()
        .map(|(path, reply)| (path.to_owned(), reply))
        .collect();
    let worker = thread::spawn(move || {
        for (path, reply) in replies {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "HTTP request never arrived");
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("{error}"),
                }
            };
            // Windows inherits the listener's nonblocking mode on accept.
            // Request reads and response writes below require blocking I/O.
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
                assert!(request.len() < 16_384);
            }
            assert!(
                String::from_utf8(request)
                    .unwrap()
                    .starts_with(&format!("GET {path} HTTP/1.1\r\n"))
            );
            stream.write_all(&reply).unwrap();
        }
    });
    (address, worker)
}

fn command(directory: &tempfile::TempDir) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_aski"));
    command
        .env("NO_PROXY", "*")
        .env("no_proxy", "*")
        .env("TMPDIR", directory.path())
        .env("TMP", directory.path())
        .env("TEMP", directory.path());
    command
}

fn run(args: &[&str]) -> Output {
    let directory = tempfile::tempdir().unwrap();
    let output = command(&directory).args(args).output().unwrap();
    assert_eq!(
        std::fs::read_dir(directory.path()).unwrap().count(),
        0,
        "download directory was not cleaned up"
    );
    output
}

#[test]
fn url_flags_render_by_content_and_preserve_query_strings() {
    for (flag, path) in [
        ("--url", "/download?id=42&token=secret"),
        ("-u", "/wrong.gif"),
    ] {
        let (base, worker) = server(vec![(
            path,
            response(
                "200 OK",
                "Content-Type: application/octet-stream\r\n",
                &png(),
            ),
        )]);
        let url = format!("{base}{path}");
        // Exercise both positions of the boolean flag around the positional input.
        let args = if flag == "-u" {
            vec![url.as_str(), flag]
        } else {
            vec![flag, url.as_str()]
        };
        let output = run(&args);
        worker.join().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8(output.stdout)
                .unwrap()
                .contains("\x1b[38;2;255;0;0m")
        );
    }
}

#[test]
fn redirects_to_extensionless_gifs_keep_animation() {
    let mut bytes = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
        for color in [[255, 0, 0, 255], [0, 0, 255, 255]] {
            encoder
                .encode_frame(image::Frame::from_parts(
                    image::RgbaImage::from_pixel(2, 2, image::Rgba(color)),
                    0,
                    0,
                    image::Delay::from_numer_denom_ms(10, 1),
                ))
                .unwrap();
        }
    }
    let (base, worker) = server(vec![
        (
            "/redirect",
            response("302 Found", "Location: /asset?format=gif\r\n", b""),
        ),
        ("/asset?format=gif", response("200 OK", "", &bytes)),
    ]);
    let output = run(&[
        "-u",
        &format!("{base}/redirect"),
        "--precompute",
        "--verbose",
    ]);
    worker.join().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(stderr.contains("2 displayed frames"), "{stderr}");
}

#[test]
fn download_failures_are_clear_and_clean_up_temporary_files() {
    for (reply, expected) in [
        (response("404 Not Found", "", b"missing"), "HTTP 404"),
        (response("200 OK", "", b""), "empty media file"),
        (
            response(
                "200 OK",
                "Content-Type: text/html; charset=UTF-8\r\n",
                b"<html>page</html>",
            ),
            "direct image, GIF, or video URL",
        ),
        (
            response(
                "200 OK",
                "Content-Type: application/vnd.apple.mpegurl\r\n",
                b"#EXTM3U",
            ),
            "streaming playlist",
        ),
        (
            b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\nConnection: close\r\n\r\npartial".to_vec(),
            "Download failed",
        ),
        (response("200 OK", "", b"GIF89a"), "Failed to decode GIF"),
    ] {
        let (base, worker) = server(vec![("/media?token=private", reply)]);
        let output = run(&["--url", &format!("{base}/media?token=private")]);
        worker.join().unwrap();
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(expected), "{stderr}");
        assert!(!stderr.contains("private"), "{stderr}");
        assert!(!stderr.contains("panicked"), "{stderr}");
    }
}

#[test]
fn invalid_url_schemes_fail_and_remote_input_is_opt_in() {
    for url in ["file:///etc/passwd", "ftp://example.com/a.gif", "not-a-url"] {
        let output = run(&["--url", url]);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("absolute HTTP or HTTPS URL"));
    }
    // Without the flag this is a local filename, so it must not make a request.
    let output = run(&["http://127.0.0.1:1/photo.png"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Failed to open"));
}

#[test]
#[ignore = "requires ffmpeg and ffprobe; run cargo test --test url -- --include-ignored"]
fn extensionless_url_video_uses_existing_decoder() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("sample.mkv");
    let output = Command::new("ffmpeg")
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
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let (base, worker) = server(vec![(
        "/video",
        response("200 OK", "", &std::fs::read(path).unwrap()),
    )]);
    let output = run(&["-u", &format!("{base}/video"), "--precompute", "--verbose"]);
    worker.join().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    assert!(stderr.contains("5 displayed frames"), "{stderr}");
}

#[cfg(unix)]
#[test]
fn ctrl_c_cancels_a_stalled_download_and_removes_partial_file() {
    use std::process::Stdio;
    let directory = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/stall", listener.local_addr().unwrap());
    let mut child = command(&directory)
        .args(["-u", &url])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut stream = loop {
        if let Ok((stream, _)) = listener.accept() {
            break stream;
        }
        if Instant::now() > deadline {
            child.kill().unwrap();
            panic!("download did not connect");
        }
        thread::sleep(Duration::from_millis(10));
    };
    stream
        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1000000\r\n\r\npartial")
        .unwrap();
    while std::fs::read_dir(directory.path()).unwrap().count() == 0 {
        if Instant::now() > deadline {
            child.kill().unwrap();
            panic!("download did not create a temporary file");
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        Command::new("kill")
            .args(["-INT", &child.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() > deadline {
            child.kill().unwrap();
            panic!("Ctrl+C did not cancel the stalled download");
        }
        thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn cloudflare_challenges_explain_browser_workaround_without_leaking_url() {
    // A challenge can replace any resource, even with a successful HTTP status.
    for status in ["403 Forbidden", "200 OK"] {
        let (base, worker) = server(vec![(
            "/video.mp4?token=private",
            response(
                status,
                "Content-Type: text/html\r\nCF-Mitigated: challenge\r\n",
                b"<html>Checking your browser</html>",
            ),
        )]);
        let output = run(&["-lovur1", &format!("{base}/video.mp4?token=private")]);
        worker.join().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success());
        assert!(
            stderr.contains("Cloudflare browser verification"),
            "{stderr}"
        );
        assert!(stderr.contains(status), "{stderr}");
        assert!(stderr.contains("saved file without --url"), "{stderr}");
        assert!(!stderr.contains("private"), "{stderr}");
        assert!(
            !stderr.contains("web page or streaming playlist"),
            "{stderr}"
        );
    }
}

#[test]
fn ordinary_forbidden_responses_do_not_assume_a_cloudflare_challenge() {
    let (base, worker) = server(vec![(
        "/video.mp4",
        response("403 Forbidden", "Server: cloudflare\r\n", b"Denied"),
    )]);
    let output = run(&["-u", &format!("{base}/video.mp4")]);
    worker.join().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(stderr.contains("HTTP 403 Forbidden"), "{stderr}");
    assert!(stderr.contains("server refused access"), "{stderr}");
    assert!(stderr.contains("without --url"), "{stderr}");
    assert!(
        !stderr.contains("Cloudflare browser verification"),
        "{stderr}"
    );
}
