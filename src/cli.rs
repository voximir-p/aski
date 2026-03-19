use clap::builder::Styles;
use clap::builder::styling::{AnsiColor, Color, Style};
use clap::{CommandFactory, FromArgMatches, Parser};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "aski",
    about = "Render an image as ANSI ASCII art",
    version,
    arg_required_else_help = true
)]
pub struct Args {
    /// Path to the input image
    pub image: PathBuf,

    /// Lines to reserve at the bottom of the terminal
    #[arg(short = 'r', long = "reserve", default_value_t = 2)]
    pub reserve: u64,

    /// Background color (e.g. 15161c, #abc, 0xff00ff, rgb(255,0,128), hsl(270,50%,50%), etc.)
    #[arg(short = 'b', long = "background", default_value = "#15161c")]
    pub background: String,

    /// Verbose output
    #[arg(short = 'v', long = "verbose")]
    pub verbose: bool,

    /// Skip alpha blending (may increase performance)
    #[arg(short = 'o', long = "opaque")]
    pub opaque: bool,

    /// Loop playback until Ctrl+C (for animations/videos)
    #[arg(short = 'l', long = "loop")]
    pub loop_playback: bool,

    /// Precompute all frames before displaying (trades startup time for smoother playback)
    #[arg(short = 'p', long = "precompute")]
    pub precompute: bool,
}

const DEFAULT: (u8, u8, u8) = (0x15, 0x16, 0x1c);

fn parse_hex(s: &str) -> Option<(u8, u8, u8)> {
    let s = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .or_else(|| s.strip_prefix('#'))
        .unwrap_or(s);
    match s.len() {
        3 => {
            let r = u8::from_str_radix(&s[0..1], 16).ok()?;
            let g = u8::from_str_radix(&s[1..2], 16).ok()?;
            let b = u8::from_str_radix(&s[2..3], 16).ok()?;
            Some((r << 4 | r, g << 4 | g, b << 4 | b))
        }
        6 => {
            let r = u8::from_str_radix(&s[0..2], 16).ok()?;
            let g = u8::from_str_radix(&s[2..4], 16).ok()?;
            let b = u8::from_str_radix(&s[4..6], 16).ok()?;
            Some((r, g, b))
        }
        _ => None,
    }
}

fn parse_func_args(s: &str) -> Option<Vec<f64>> {
    let inner = s.trim();
    if inner.is_empty() {
        return None;
    }
    // Split on comma or whitespace, strip '%', parse as f64
    let parts: Vec<f64> = inner
        .split(|c: char| c == ',' || c == '/')
        .flat_map(|p| p.split_whitespace())
        .map(|p| p.trim_end_matches('%'))
        .filter(|p| !p.is_empty())
        .map(|p| p.parse::<f64>())
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    Some(parts)
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    let s = s / 100.0;
    let l = l / 100.0;
    let h = ((h % 360.0) + 360.0) % 360.0;

    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;

    let (r1, g1, b1) = match h as u32 {
        0..=59 => (c, x, 0.0),
        60..=119 => (x, c, 0.0),
        120..=179 => (0.0, c, x),
        180..=239 => (0.0, x, c),
        240..=299 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    (
        ((r1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((g1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((b1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

fn hwb_to_rgb(h: f64, w: f64, b: f64) -> (u8, u8, u8) {
    let w = w / 100.0;
    let b = b / 100.0;
    let (w, b) = if w + b > 1.0 {
        let sum = w + b;
        (w / sum, b / sum)
    } else {
        (w, b)
    };
    let (r, g, bl) = hsl_to_rgb(h, 100.0, 50.0);
    let f = |c: u8| {
        ((c as f64 / 255.0 * (1.0 - w - b) + w) * 255.0)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    (f(r), f(g), f(bl))
}

fn linear_to_srgb(c: f64) -> f64 {
    if c <= 0.0031308 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

fn linear_rgb_to_srgb(r: f64, g: f64, b: f64) -> (u8, u8, u8) {
    (
        (linear_to_srgb(r) * 255.0).round().clamp(0.0, 255.0) as u8,
        (linear_to_srgb(g) * 255.0).round().clamp(0.0, 255.0) as u8,
        (linear_to_srgb(b) * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

fn lab_to_rgb(l: f64, a: f64, b: f64) -> (u8, u8, u8) {
    // CIE Lab -> XYZ (D65)
    let fy = (l + 16.0) / 116.0;
    let fx = a / 500.0 + fy;
    let fz = fy - b / 200.0;

    let xr = if fx.powi(3) > 0.008856 {
        fx.powi(3)
    } else {
        (116.0 * fx - 16.0) / 903.3
    };
    let yr = if l > 7.9996 { fy.powi(3) } else { l / 903.3 };
    let zr = if fz.powi(3) > 0.008856 {
        fz.powi(3)
    } else {
        (116.0 * fz - 16.0) / 903.3
    };

    // D65 white point
    let x = xr * 0.95047;
    let y = yr * 1.00000;
    let z = zr * 1.08883;

    // XYZ -> linear sRGB
    let rl = 3.2404542 * x - 1.5371385 * y - 0.4985314 * z;
    let gl = -0.9692660 * x + 1.8760108 * y + 0.0415560 * z;
    let bl = 0.0556434 * x - 0.2040259 * y + 1.0572252 * z;

    linear_rgb_to_srgb(rl, gl, bl)
}

fn lch_to_rgb(l: f64, c: f64, h: f64) -> (u8, u8, u8) {
    let h_rad = h.to_radians();
    let a = c * h_rad.cos();
    let b = c * h_rad.sin();
    lab_to_rgb(l, a, b)
}

fn oklab_to_rgb(l: f64, a: f64, b: f64) -> (u8, u8, u8) {
    // Oklab -> LMS (approximate inverse)
    let l_ = l + 0.3963377774 * a + 0.2158037573 * b;
    let m_ = l - 0.1055613458 * a - 0.0638541728 * b;
    let s_ = l - 0.0894841775 * a - 1.2914855480 * b;

    let l3 = l_ * l_ * l_;
    let m3 = m_ * m_ * m_;
    let s3 = s_ * s_ * s_;

    // LMS -> linear sRGB
    let rl = 4.0767416621 * l3 - 3.3077115913 * m3 + 0.2309699292 * s3;
    let gl = -1.2684380046 * l3 + 2.6097574011 * m3 - 0.3413193965 * s3;
    let bl = -0.0041960863 * l3 - 0.7034186147 * m3 + 1.7076147010 * s3;

    linear_rgb_to_srgb(rl, gl, bl)
}

fn oklch_to_rgb(l: f64, c: f64, h: f64) -> (u8, u8, u8) {
    let h_rad = h.to_radians();
    let a = c * h_rad.cos();
    let b = c * h_rad.sin();
    oklab_to_rgb(l, a, b)
}

pub fn parse_color(input: &str) -> (u8, u8, u8) {
    let s = input.trim();

    // Try hex formats: #abc, #aabbcc, 0xabc, 0xaabbcc
    if s.starts_with('#') || s.starts_with("0x") || s.starts_with("0X") {
        if let Some(c) = parse_hex(s) {
            return c;
        }
    }

    // Try functional notations: rgb(), hsl(), hwb()
    if let Some(rest) = s.strip_prefix("rgb(").or_else(|| s.strip_prefix("rgba(")) {
        if let Some(inner) = rest.strip_suffix(')') {
            if let Some(args) = parse_func_args(inner) {
                if args.len() >= 3 {
                    return (
                        args[0].round().clamp(0.0, 255.0) as u8,
                        args[1].round().clamp(0.0, 255.0) as u8,
                        args[2].round().clamp(0.0, 255.0) as u8,
                    );
                }
            }
        }
    }

    if let Some(rest) = s.strip_prefix("hsl(").or_else(|| s.strip_prefix("hsla(")) {
        if let Some(inner) = rest.strip_suffix(')') {
            if let Some(args) = parse_func_args(inner) {
                if args.len() >= 3 {
                    return hsl_to_rgb(args[0], args[1], args[2]);
                }
            }
        }
    }

    if let Some(rest) = s.strip_prefix("hwb(") {
        if let Some(inner) = rest.strip_suffix(')') {
            if let Some(args) = parse_func_args(inner) {
                if args.len() >= 3 {
                    return hwb_to_rgb(args[0], args[1], args[2]);
                }
            }
        }
    }

    if let Some(rest) = s.strip_prefix("lab(") {
        if let Some(inner) = rest.strip_suffix(')') {
            if let Some(args) = parse_func_args(inner) {
                if args.len() >= 3 {
                    return lab_to_rgb(args[0], args[1], args[2]);
                }
            }
        }
    }

    if let Some(rest) = s.strip_prefix("lch(") {
        if let Some(inner) = rest.strip_suffix(')') {
            if let Some(args) = parse_func_args(inner) {
                if args.len() >= 3 {
                    return lch_to_rgb(args[0], args[1], args[2]);
                }
            }
        }
    }

    if let Some(rest) = s.strip_prefix("oklab(") {
        if let Some(inner) = rest.strip_suffix(')') {
            if let Some(args) = parse_func_args(inner) {
                if args.len() >= 3 {
                    return oklab_to_rgb(args[0], args[1], args[2]);
                }
            }
        }
    }

    if let Some(rest) = s.strip_prefix("oklch(") {
        if let Some(inner) = rest.strip_suffix(')') {
            if let Some(args) = parse_func_args(inner) {
                if args.len() >= 3 {
                    return oklch_to_rgb(args[0], args[1], args[2]);
                }
            }
        }
    }

    // Try as bare 6-digit or 3-digit hex without prefix
    if let Some(c) = parse_hex(s) {
        return c;
    }

    eprintln!("Invalid color '{}', using default #15161c", input);
    DEFAULT
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
    Args::from_arg_matches(&Args::command().styles(make_styles()).get_matches()).unwrap()
}
