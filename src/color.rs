fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    let s = s.clamp(0.0, 100.0) / 100.0;
    let l = l.clamp(0.0, 100.0) / 100.0;
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
    let w = w.clamp(0.0, 100.0) / 100.0;
    let b = b.clamp(0.0, 100.0) / 100.0;
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
    // CSS Lab uses D50; adapt to D65 before converting to sRGB.
    // https://www.w3.org/TR/css-color-4/#color-conversion-code
    let l = l.clamp(0.0, 100.0);
    let fy = (l + 16.0) / 116.0;
    let inverse = |v: f64| {
        if v.powi(3) > 216.0 / 24389.0 {
            v.powi(3)
        } else {
            (116.0 * v - 16.0) / (24389.0 / 27.0)
        }
    };
    let x50 = inverse(fy + a / 500.0) * (0.3457 / 0.3585);
    let y50 = inverse(fy);
    let z50 = inverse(fy - b / 200.0) * ((1.0 - 0.3457 - 0.3585) / 0.3585);
    let x = 0.955473421488075 * x50 - 0.02309845494876471 * y50 + 0.06325924320057072 * z50;
    let y = -0.0283697093338637 * x50 + 1.0099953980813041 * y50 + 0.021041441191917323 * z50;
    let z = 0.012314014864481998 * x50 - 0.020507649298898964 * y50 + 1.330365926242124 * z50;

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

/// An opaque sRGB color with eight bits per channel.
pub type Color = (u8, u8, u8);

/// A malformed or unsupported background color.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseColorError;

impl std::fmt::Display for ParseColorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "expected an opaque hex, rgb(), hsl(), hwb(), lab(), lch(), oklab(), or oklch() color",
        )
    }
}

impl std::error::Error for ParseColorError {}

fn number(token: &str, percentage_scale: f64) -> Option<f64> {
    let value = if let Some(percent) = token.strip_suffix('%') {
        percent.parse::<f64>().ok()? * percentage_scale / 100.0
    } else {
        token.parse::<f64>().ok()?
    };
    value.is_finite().then_some(value)
}

fn hue(token: &str) -> Option<f64> {
    for (unit, scale) in [
        ("deg", 1.0),
        ("grad", 0.9),
        ("turn", 360.0),
        ("rad", 180.0 / std::f64::consts::PI),
    ] {
        if let Some(value) = token.strip_suffix(unit) {
            let value = value.parse::<f64>().ok()? * scale;
            return value.is_finite().then(|| value.rem_euclid(360.0));
        }
    }
    let value = token.parse::<f64>().ok()?;
    value.is_finite().then(|| value.rem_euclid(360.0))
}

fn hex(input: &str) -> Option<Color> {
    let s = input
        .strip_prefix('#')
        .or_else(|| input.strip_prefix("0x"))
        .unwrap_or(input);
    if !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    match s.len() {
        3 => Some((
            u8::from_str_radix(&s[0..1], 16).ok()? * 17,
            u8::from_str_radix(&s[1..2], 16).ok()? * 17,
            u8::from_str_radix(&s[2..3], 16).ok()? * 17,
        )),
        6 => Some((
            u8::from_str_radix(&s[0..2], 16).ok()?,
            u8::from_str_radix(&s[2..4], 16).ok()?,
            u8::from_str_radix(&s[4..6], 16).ok()?,
        )),
        _ => None,
    }
}

/// Parse CSS-like color notation, including percentages and hue angle units.
///
/// Supports three/six-digit hex and the functions named in [`ParseColorError`].
/// Named colors, `none`, and relative colors are not supported. Optional alpha
/// must be fully opaque, since a terminal background cannot itself be transparent.
/// Invalid input returns an error without printing, panicking, or using a fallback.
pub fn parse_color(input: &str) -> Result<Color, ParseColorError> {
    parse_color_inner(&input.trim().to_ascii_lowercase()).ok_or(ParseColorError)
}

fn parse_color_inner(input: &str) -> Option<Color> {
    if let Some(color) = hex(input) {
        return Some(color);
    }
    let (name, rest) = input.split_once('(')?;
    let inner = rest.strip_suffix(')')?.trim();
    let legacy = inner.contains(',');
    let mut args: Vec<&str>;
    if legacy {
        if !matches!(name, "rgb" | "rgba" | "hsl" | "hsla") || inner.contains('/') {
            return None;
        }
        args = inner.split(',').map(str::trim).collect();
        if args.len() == 4 && number(args.pop()?, 1.0)? != 1.0 {
            return None;
        }
    } else {
        let components = if let Some((components, alpha)) = inner.split_once('/') {
            if number(alpha.trim(), 1.0)? != 1.0 {
                return None;
            }
            components
        } else {
            inner
        };
        args = components.split_whitespace().collect();
    }
    let [a, b, c]: [&str; 3] = args.try_into().ok()?;
    match name {
        "rgb" | "rgba" => {
            if legacy
                && (a.ends_with('%') != b.ends_with('%') || b.ends_with('%') != c.ends_with('%'))
            {
                return None;
            }
            Some((
                number(a, 255.0)?.round().clamp(0.0, 255.0) as u8,
                number(b, 255.0)?.round().clamp(0.0, 255.0) as u8,
                number(c, 255.0)?.round().clamp(0.0, 255.0) as u8,
            ))
        }
        "hsl" | "hsla" => Some(hsl_to_rgb(hue(a)?, number(b, 100.0)?, number(c, 100.0)?)),
        "hwb" => Some(hwb_to_rgb(hue(a)?, number(b, 100.0)?, number(c, 100.0)?)),
        "lab" => Some(lab_to_rgb(
            number(a, 100.0)?,
            number(b, 125.0)?,
            number(c, 125.0)?,
        )),
        "lch" => Some(lch_to_rgb(
            number(a, 100.0)?,
            number(b, 150.0)?.max(0.0),
            hue(c)?,
        )),
        "oklab" => Some(oklab_to_rgb(
            number(a, 1.0)?.clamp(0.0, 1.0),
            number(b, 0.4)?,
            number(c, 0.4)?,
        )),
        "oklch" => Some(oklch_to_rgb(
            number(a, 1.0)?.clamp(0.0, 1.0),
            number(b, 0.4)?.max(0.0),
            hue(c)?,
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_and_unicode_validation() {
        for s in ["#aBc", "0Xabc", " ABC ", "#aabbcc"] {
            assert_eq!(parse_color(s).unwrap(), (170, 187, 204));
        }
        for s in ["éa", "#éa", "€", "#ééé", "💙", "", "#abcd", "#gggggg"] {
            assert!(parse_color(s).is_err(), "{s}");
        }
    }

    #[test]
    fn percentages_angles_and_clamping() {
        assert_eq!(parse_color("RGB(100% 0% 50%)").unwrap(), (255, 0, 128));
        assert_eq!(parse_color("hsl(-240deg 100% 50%)").unwrap(), (0, 255, 0));
        assert_eq!(parse_color("hsl(0.5turn 100% 50%)").unwrap(), (0, 255, 255));
        assert_eq!(parse_color("hwb(0 100% 100%)").unwrap(), (128, 128, 128));
        assert_eq!(parse_color("rgb(-1, 999, 0)").unwrap(), (0, 255, 0));
        assert_eq!(parse_color("oklab(100% 0 0)").unwrap(), (255, 255, 255));
        assert_eq!(parse_color("rgba(1,2,3,1)").unwrap(), (1, 2, 3));
    }

    #[test]
    fn css_lab_reference_red_and_white() {
        assert_eq!(
            parse_color("lab(54.2917 80.8125 69.8851)").unwrap(),
            (255, 0, 0)
        );
        assert_eq!(parse_color("lab(100% 0 0)").unwrap(), (255, 255, 255));
    }

    #[test]
    fn rejects_invalid_components() {
        for s in [
            "rgb(NaN 0 0)",
            "rgb(inf 0 0)",
            "rgb(1 2)",
            "rgb(1 2 3 4 5)",
            "rgb(1,,2,3)",
            "rgb(1 2 3 / 0.5)",
            "hsl(1% 50% 50%)",
            "rgb(1%,2,3)",
            "rgb(1 2 3 / 1 / 1)",
            "lab(1e999 0 0)",
        ] {
            assert!(parse_color(s).is_err(), "{s}");
        }
    }
}
