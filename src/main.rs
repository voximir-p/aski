mod cli;
mod frames;
mod terminal_utils;
mod media_render;
mod static_playback;
mod animated_playback;
mod render;

fn main() {
    let args = cli::parse_args();
    let path = &args.image;
    let bg = cli::parse_color(&args.background);

    match frames::detect_media_type(path) {
        frames::MediaType::Static => static_playback::render_static(&args, bg),
        media_type => animated_playback::render_animated(&args, bg, media_type),
    }
}
