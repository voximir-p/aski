mod animated_playback;
mod cli;
mod static_playback;
mod terminal_utils;

use std::io;
use std::process::ExitCode;

fn run() -> io::Result<()> {
    let args = cli::parse_args();
    match aski::frames::detect_media_type(&args.image) {
        aski::MediaType::Static => static_playback::render_static(&args),
        media_type => animated_playback::render_animated(&args, media_type),
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
