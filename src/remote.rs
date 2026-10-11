//! CLI-only HTTP input preparation. Decoders always receive a local file.

use std::fs::File;
use std::io::{self, IsTerminal, Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use aski::MediaType;

pub struct DownloadedMedia {
    pub path: PathBuf,
    pub media_type: MediaType,
    _directory: tempfile::TempDir,
}

/// Return `None` on Ctrl+C. Dropping the transfer future closes the connection
/// and removes its temporary directory, including during a stalled response.
pub fn download(
    input: &str,
    cancelled: &AtomicBool,
    verbose: bool,
) -> io::Result<Option<DownloadedMedia>> {
    let url = reqwest::Url::parse(input)
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "--url requires an absolute HTTP or HTTPS URL",
            )
        })?;
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let transfer = fetch(url, verbose);
            tokio::pin!(transfer);
            loop {
                if cancelled.load(Ordering::Relaxed) {
                    return Ok(None);
                }
                tokio::select! {
                    result = &mut transfer => return result.map(Some),
                    _ = tokio::time::sleep(Duration::from_millis(50)) => {}
                }
            }
        })
}

async fn fetch(url: reqwest::Url, verbose: bool) -> io::Result<DownloadedMedia> {
    let report = verbose || io::stderr().is_terminal();
    if report {
        eprintln!("Downloading media... (Ctrl+C to cancel)");
    }
    let client = reqwest::Client::builder()
        .user_agent(concat!("aski/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(download_error)?;
    let mut response = client.get(url).send().await.map_err(download_error)?;
    // Cloudflare identifies challenge pages with this header, regardless of
    // the requested resource or HTTP status. Do not mistake one for media.
    if response
        .headers()
        .get("cf-mitigated")
        .is_some_and(|value| value == "challenge")
    {
        return Err(io::Error::other(format!(
            "Download blocked by Cloudflare browser verification (HTTP {}). \
             Even direct media URLs can require a browser challenge, which aski cannot complete. \
             Open the URL in your browser, complete verification, save the media, \
             then run aski on the saved file without --url.",
            response.status()
        )));
    }
    if !response.status().is_success() {
        let hint = if response.status() == reqwest::StatusCode::FORBIDDEN {
            "; the server refused access. Direct media URLs may still require authentication, \
             browser verification, or permission to download. Try saving the media in your \
             browser and opening the saved file without --url"
        } else {
            ""
        };
        return Err(io::Error::other(format!(
            "Download failed: HTTP {}{hint}",
            response.status()
        )));
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if matches!(
        content_type.as_str(),
        "text/html"
            | "application/xhtml+xml"
            | "application/vnd.apple.mpegurl"
            | "application/x-mpegurl"
            | "audio/x-mpegurl"
            | "application/dash+xml"
    ) {
        return Err(io::Error::other(
            "URL returned a web page or streaming playlist; use a direct image, GIF, or video URL",
        ));
    }

    let directory = tempfile::Builder::new()
        .prefix("aski-download-")
        .tempdir()?;
    let mut path = directory.path().join("media");
    let mut file = File::create(&path)?;
    let mut bytes = 0u64;
    let mut last_report = Instant::now();
    while let Some(chunk) = response.chunk().await.map_err(download_error)? {
        file.write_all(&chunk)?;
        bytes += chunk.len() as u64;
        if report && last_report.elapsed() >= Duration::from_secs(1) {
            eprintln!("Downloading: {:.1} MiB", bytes as f64 / 1_048_576.0);
            last_report = Instant::now();
        }
    }
    // Close before renaming or decoding, including on Windows.
    drop(file);
    if bytes == 0 {
        return Err(io::Error::other("URL returned an empty media file"));
    }
    let mut header = [0u8; 32];
    let count = File::open(&path)?.read(&mut header)?;
    let media_type = match image::guess_format(&header[..count]) {
        Ok(format) => {
            let named_path = path.with_extension(format.extensions_str()[0]);
            std::fs::rename(&path, &named_path)?;
            path = named_path;
            if format == image::ImageFormat::Gif {
                MediaType::Gif
            } else {
                MediaType::Static
            }
        }
        // ffprobe detects video containers from their contents. No extension or
        // server-provided filename is required, and query strings are irrelevant.
        Err(_) => MediaType::Video,
    };
    if report {
        eprintln!("Downloaded {:.1} MiB", bytes as f64 / 1_048_576.0);
    }
    Ok(DownloadedMedia {
        path,
        media_type,
        _directory: directory,
    })
}

fn download_error(error: reqwest::Error) -> io::Error {
    // Signed URLs and embedded credentials must not leak into diagnostics.
    io::Error::other(format!("Download failed: {}", error.without_url()))
}
