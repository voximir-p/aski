mod animated_playback;
mod cli;
mod remote;
mod static_playback;
mod terminal_utils;

use std::io;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

fn run() -> io::Result<()> {
    let mut args = cli::parse_args();
    let mut media_type = aski::frames::detect_media_type(&args.image);
    let cancelled = Arc::new(AtomicBool::new(false));
    if args.url || media_type != aski::MediaType::Static {
        let signal = Arc::clone(&cancelled);
        ctrlc::set_handler(move || signal.store(true, Ordering::Relaxed))
            .map_err(io::Error::other)?;
    }
    // Keep the temporary file alive until playback and its decoder have finished.
    let _download = if args.url {
        let url = args.image.to_str().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "URL must be valid UTF-8")
        })?;
        let Some(download) = remote::download(url, &cancelled, args.verbose)? else {
            return Ok(());
        };
        args.image = download.path.clone();
        media_type = download.media_type;
        Some(download)
    } else {
        None
    };
    if cancelled.load(Ordering::Relaxed) {
        return Ok(());
    }
    match media_type {
        aski::MediaType::Static => static_playback::render_static(&args),
        media_type => animated_playback::render_animated(&args, media_type, cancelled),
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("aski: {error}");
            ExitCode::FAILURE
        }
    }
}
