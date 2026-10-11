#[cfg(test)]
use clap::CommandFactory;
use clap::Parser;
use clap::builder::Styles;
use clap::builder::styling::{AnsiColor, Color, Style};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "aski",
    about = "Render images, GIFs, and videos in a true-color terminal",
    version,
    styles = make_styles(),
    max_term_width = 100,
    arg_required_else_help = true,
    after_help = "Examples:\n  aski photo.jpg\n  aski --url 'https://example.com/photo.jpg'\n  aski logo.png -b '#1e1e2e'\n  aski animation.gif --loop\n  aski video.mp4 --precompute --loop\n\nUse --help for details. Press Ctrl+C to stop playback.",
    after_long_help = "Examples:\n  aski photo.jpg                         Render an image\n  aski -u 'https://example.com/clip.gif'  Download and play a GIF\n  aski logo.png -b '#1e1e2e'             Blend transparency\n  aski animation.gif --loop              Replay until Ctrl+C\n  aski video.mp4 --precompute --loop     Prepare frames, then replay\n  aski video.mp4 --no-cache --prefetch 2  Limit buffered frames\n\nPlayback notes:\n  Videos require ffmpeg and ffprobe on PATH. Audio is not played.\n  --fps-limit slows playback; it does not skip frames.\n  --precompute requires caching and cannot be combined with --no-cache.\n  RGB percentages and hue units are accepted in background colors.\n  Use a terminal with 24-bit color support for the best results."
)]
pub struct Args {
    #[arg(
        value_name = "MEDIA",
        help = "Media file to display (HTTP/HTTPS address with --url)",
        long_help = "Path to the image, GIF, or video file to render in the terminal.\nWith --url, treat this argument as a direct HTTP or HTTPS media URL."
    )]
    pub image: PathBuf,

    #[arg(
        short = 'u',
        long = "url",
        help_heading = "Input",
        help = "Read MEDIA from an HTTP or HTTPS URL",
        long_help = "Download MEDIA from a direct HTTP or HTTPS URL before rendering.\nFollows redirects and supports links without file extensions.\nThe temporary download is reused when looping and removed on exit.\nUse a direct media link, not a web page or streaming playlist.\nSites requiring browser verification must be downloaded in a browser.\nQuote URLs containing shell characters such as &."
    )]
    pub url: bool,

    // ── Display ────────────────────────────────────────────────────────────
    #[arg(
        short = 'b',
        long = "background",
        default_value = "#1a1515",
        value_name = "COLOR",
        value_parser = aski::parse_color,
        help_heading = "Display",
        help = "Background color for transparent areas",
        long_help = "Color to show behind transparent areas of the image.\nAccepts CSS-like color formats: #15161c, #abc, rgb(21,22,28),\nhsl(235,14%,10%), hwb(), lab(), lch(), oklab(), oklch().\nRGB percentages and hue units (deg, rad, grad, turn) are supported.\nThe background must be opaque. Only transparent source pixels change."
    )]
    pub background: aski::Color,

    #[arg(
        short = 'o',
        long = "opaque",
        help_heading = "Display",
        help = "Treat pixels as fully opaque (should render faster)",
        long_help = "Render every pixel as fully opaque, skipping transparency math.\nUse this when your image has no meaningful alpha channel to skip\ncompositing against the background color."
    )]
    pub opaque: bool,

    #[arg(
        short = 'r',
        long = "reserve",
        default_value_t = 2,
        value_name = "ROWS",
        help_heading = "Display",
        help = "Reserve rows at terminal bottom",
        long_help = "Keep this many rows below the image. Animated playback uses one\nreserved row for its status line. With --verbose, at least one row\nis reserved when the terminal has room. Set to 0 for maximum image area."
    )]
    pub reserve: u64,

    // ── Scaling ────────────────────────────────────────────────────────────
    #[arg(
        long = "cell-width",
        default_value_t = 10,
        value_parser = clap::value_parser!(u32).range(1..),
        value_name = "PX",
        help_heading = "Scaling",
        help = "Terminal cell width hint in pixels",
        long_help = "Pixel width of one terminal cell, used to preserve the correct aspect\nratio when mapping image pixels to character cells. The default of 10\nis a starting point. Lower this value if the output looks\nhorizontally squashed; raise it if it looks stretched."
    )]
    pub cell_width: u32,

    #[arg(
        long = "cell-height",
        default_value_t = 22,
        value_parser = clap::value_parser!(u32).range(1..),
        value_name = "PX",
        help_heading = "Scaling",
        help = "Terminal cell height hint in pixels",
        long_help = "Pixel height of one terminal cell, used together with --cell-width\nto preserve aspect ratio. The default of 22 matches most monospace\nfonts. Raise this if the output looks vertically squashed; lower it\nif it looks stretched."
    )]
    pub cell_height: u32,

    // ── Playback ───────────────────────────────────────────────────────────
    #[arg(
        short = 'l',
        long = "loop",
        help_heading = "Playback",
        help = "Loop playback until interrupted",
        long_help = "Keep playing the animation or video in an infinite loop until\nCtrl+C is pressed. Unless --no-cache is set, later passes reuse\nrendered frames from memory."
    )]
    pub loop_playback: bool,

    #[arg(
        short = 'p',
        long = "precompute",
        conflicts_with = "no_cache",
        help_heading = "Playback",
        help = "Render all frames before playback",
        long_help = "Decode and render every frame into memory before playback begins.\nThis causes a startup pause proportional to the video length, and\nremoves decoding and rendering work from playback.\nWithout this flag, frames are rendered on demand. Frames are cached\nonly when looping is requested."
    )]
    pub precompute: bool,

    #[arg(
        long = "fps-limit",
        default_value_t = 0,
        value_name = "FPS",
        help_heading = "Playback",
        help = "Cap playback FPS (0 = source FPS)",
        long_help = "Cap playback speed to at most N frames per second. Set to 0 (default)\nto follow the source timing. Handy on slow machines to cut\nCPU usage per second, or to inspect high-FPS content. This slows\nplayback without skipping frames; it does not preserve video duration."
    )]
    pub fps_limit: u64,

    // ── Performance ────────────────────────────────────────────────────────
    #[arg(
        long = "prefetch",
        default_value_t = 8,
        value_parser = parse_prefetch,
        value_name = "FRAMES",
        help_heading = "Performance",
        help = "Decoder prefetch buffer size",
        long_help = "How many video frames the background ffmpeg decoder thread buffers\nahead of the display thread. A larger buffer absorbs decode slowdowns\nand makes playback smoother; a smaller buffer lowers peak RAM and\nreduces the startup delay before the first frame appears.\nAccepts 0 to 1024. Set to 0 for unbuffered delivery.\nHas no effect on GIFs or static images."
    )]
    pub prefetch: usize,

    #[arg(
        long = "no-cache",
        help_heading = "Performance",
        help = "Disable ANSI frame caching",
        long_help = "Turn off the in-memory ANSI frame cache. Every frame is decoded and\nrendered from scratch on each pass instead of being stored.\nRecommended for very long or high-resolution videos where the cached\nstrings would consume too much RAM. CPU usage on loops will be higher."
    )]
    pub no_cache: bool,

    // ── Diagnostics ────────────────────────────────────────────────────────
    #[arg(
        short = 'v',
        long = "verbose",
        help_heading = "Diagnostics",
        help = "Show runtime diagnostics",
        long_help = "Show image dimensions and a final playback summary on stderr.\nDuring animation, show FPS, render time, and cache/late-frame counts\nwhen the terminal is wide enough. Narrow windows show a compact status."
    )]
    pub verbose: bool,
}

fn parse_prefetch(value: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .ok()
        .filter(|&n| n <= aski::frames::MAX_PREFETCH)
        .ok_or_else(|| {
            format!(
                "expected a frame count from 0 to {}",
                aski::frames::MAX_PREFETCH
            )
        })
}

fn make_styles() -> Styles {
    Styles::styled()
        .header(
            Style::new()
                .bold()
                .fg_color(Some(Color::Ansi(AnsiColor::Green))),
        )
        .usage(
            Style::new()
                .bold()
                .fg_color(Some(Color::Ansi(AnsiColor::Green))),
        )
        .literal(
            Style::new()
                .bold()
                .fg_color(Some(Color::Ansi(AnsiColor::Cyan))),
        )
        .placeholder(Style::new().fg_color(Some(Color::Ansi(AnsiColor::Cyan))))
        .error(
            Style::new()
                .bold()
                .fg_color(Some(Color::Ansi(AnsiColor::Red))),
        )
}

pub fn parse_args() -> Args {
    Args::parse()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_arguments_before_opening_media() {
        for args in [
            vec!["aski", "x", "--cell-width", "0"],
            vec!["aski", "x", "--prefetch", "18446744073709551615"],
            vec!["aski", "x", "--cell-height", "0"],
            vec!["aski", "x", "--background", "#éa"],
            vec!["aski", "x", "--precompute", "--no-cache"],
        ] {
            assert!(Args::try_parse_from(args).is_err());
        }
        assert!(Args::try_parse_from(["aski", "x", "--prefetch", "0"]).is_ok());
        Args::command().debug_assert();
    }
}
