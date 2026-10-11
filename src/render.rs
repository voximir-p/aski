use std::fmt::{self, Write as _};

/// Invalid geometry, pixel data, or an output allocation that is too large.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderError(&'static str);

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for RenderError {}

// Pixel coverage is computed in integer coordinates to avoid rounding a boundary
// into the wrong source pixel. We normalize weights once, outside the hot loop.
struct Span {
    start: usize,
    end: usize,
    first: f64,
    last: f64,
    interior: f64,
}

impl Span {
    fn weight(&self, pixel: usize) -> f64 {
        if pixel == self.start {
            self.first
        } else if pixel + 1 == self.end {
            self.last
        } else {
            self.interior
        }
    }
}

fn spans(source: u32, output: u32) -> Result<Vec<Span>, RenderError> {
    let mut result = Vec::new();
    result
        .try_reserve_exact(output as usize)
        .map_err(|_| RenderError("Output dimensions are too large"))?;
    let source = u64::from(source);
    let output = u64::from(output);
    for i in 0..output {
        let left = i * source;
        let right = (i + 1) * source;
        let start = left / output;
        let end = right.div_ceil(output);
        result.push(Span {
            start: start as usize,
            end: end as usize,
            first: (right.min((start + 1) * output) - left) as f64 / source as f64,
            last: (right - (end - 1) * output) as f64 / source as f64,
            interior: output as f64 / source as f64,
        });
    }
    Ok(result)
}

#[inline]
fn channels<const OPAQUE: bool>(pixel: &[u8]) -> [u64; 4] {
    if OPAQUE {
        [pixel[0] as u64, pixel[1] as u64, pixel[2] as u64, 0]
    } else {
        let alpha = pixel[3] as u64;
        [
            pixel[0] as u64 * alpha,
            pixel[1] as u64 * alpha,
            pixel[2] as u64 * alpha,
            alpha,
        ]
    }
}

/// Reusable area-filtered RGBA-to-ANSI renderer.
///
/// Sampling geometry, row buffers, and output capacity are reused between frames.
/// Alpha is composited against `background` before averaging, avoiding halos from
/// the RGB channels of transparent pixels. Colors are averaged in sRGB space.
/// Rows are separated by CRLF, with no trailing newline; output ends in an ANSI reset.
///
/// ```
/// use aski::Renderer;
/// let mut renderer = Renderer::new(1, 1, 2, 1, (0, 0, 0), false)?;
/// let ansi = renderer.render(&[255, 0, 0, 255])?;
/// assert!(ansi.contains("██"));
/// # Ok::<(), aski::RenderError>(())
/// ```
pub struct Renderer {
    width: usize,
    input_len: usize,
    horizontal: Vec<Span>,
    vertical: Vec<Span>,
    row: Vec<[f64; 3]>,
    sums: Vec<[f64; 3]>,
    background: (u8, u8, u8),
    opaque: bool,
    output: String,
}

impl Renderer {
    /// Construct a renderer. All dimensions must be nonzero, and output
    /// dimensions must fit in `u32`. Invalid or overflowing sizes return an error.
    pub fn new(
        width: u32,
        height: u32,
        output_width: u64,
        output_height: u64,
        background: (u8, u8, u8),
        opaque: bool,
    ) -> Result<Self, RenderError> {
        let ow =
            u32::try_from(output_width).map_err(|_| RenderError("Output width is too large"))?;
        let oh =
            u32::try_from(output_height).map_err(|_| RenderError("Output height is too large"))?;
        if width == 0 || height == 0 || ow == 0 || oh == 0 {
            return Err(RenderError("Image and output dimensions must be nonzero"));
        }
        let input_len = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4))
            .filter(|&n| n <= isize::MAX as usize)
            .ok_or(RenderError("Image dimensions are too large"))?;
        let capacity = (ow as usize)
            .checked_mul(oh as usize)
            .and_then(|n| n.checked_mul(24))
            .and_then(|n| {
                (oh as usize)
                    .checked_mul(6)
                    .and_then(|rows| n.checked_add(rows))
            })
            .ok_or(RenderError("Output dimensions are too large"))?;
        let mut output = String::new();
        output
            .try_reserve_exact(capacity)
            .map_err(|_| RenderError("Output dimensions are too large"))?;
        Ok(Self {
            width: width as usize,
            input_len,
            horizontal: spans(width, ow)?,
            vertical: spans(height, oh)?,
            row: vec![[0.0; 3]; ow as usize],
            sums: vec![[0.0; 3]; ow as usize],
            background,
            opaque,
            output,
        })
    }

    /// Render tightly packed RGBA8 pixels. The returned text is valid until the
    /// next mutable borrow of this renderer; use `.to_owned()` to cache a frame.
    pub fn render(&mut self, rgba: &[u8]) -> Result<&str, RenderError> {
        if rgba.len() != self.input_len {
            return Err(RenderError(
                "RGBA buffer length does not match image dimensions",
            ));
        }
        if self.opaque {
            self.render_inner::<true>(rgba);
        } else {
            self.render_inner::<false>(rgba);
        }
        Ok(&self.output)
    }

    fn render_inner<const OPAQUE: bool>(&mut self, rgba: &[u8]) {
        self.output.clear();
        let bg = [
            self.background.0 as f64,
            self.background.1 as f64,
            self.background.2 as f64,
        ];
        let mut cached_row = None;
        let mut previous_color = None;
        for (y, vertical) in self.vertical.iter().enumerate() {
            self.sums.fill([0.0; 3]);
            for sy in vertical.start..vertical.end {
                if cached_row != Some(sy) {
                    let pixels = &rgba[sy * self.width * 4..(sy + 1) * self.width * 4];
                    for (sum, span) in self.row.iter_mut().zip(&self.horizontal) {
                        // Interior pixels all have the same coverage. Accumulate
                        // those as integers, then weight once per cell. Only the
                        // two boundary pixels need fractional coverage math.
                        let first = channels::<OPAQUE>(&pixels[span.start * 4..span.start * 4 + 4]);
                        let mut weighted = first.map(|v| v as f64 * span.first);
                        if span.end > span.start + 1 {
                            let last =
                                channels::<OPAQUE>(&pixels[(span.end - 1) * 4..span.end * 4]);
                            let mut interior = [0_u64; 4];
                            for pixel in pixels[(span.start + 1) * 4..(span.end - 1) * 4]
                                .as_chunks::<4>()
                                .0
                            {
                                let value = channels::<OPAQUE>(pixel);
                                for channel in 0..4 {
                                    interior[channel] += value[channel];
                                }
                            }
                            for channel in 0..4 {
                                weighted[channel] += last[channel] as f64 * span.last
                                    + interior[channel] as f64 * span.interior;
                            }
                        }
                        for channel in 0..3 {
                            sum[channel] = if OPAQUE {
                                weighted[channel]
                            } else {
                                (weighted[channel] + bg[channel] * (255.0 - weighted[3])) / 255.0
                            };
                        }
                    }
                    cached_row = Some(sy);
                }
                let weight = vertical.weight(sy);
                for (sum, row) in self.sums.iter_mut().zip(&self.row) {
                    for channel in 0..3 {
                        sum[channel] += row[channel] * weight;
                    }
                }
            }
            if y > 0 {
                self.output.push_str("\r\n");
            }
            for pixel in &self.sums {
                let color = pixel.map(|v| v.round().clamp(0.0, 255.0) as u8);
                if previous_color != Some(color) {
                    let _ = write!(
                        self.output,
                        "\x1b[38;2;{};{};{}m",
                        color[0], color[1], color[2]
                    );
                    previous_color = Some(color);
                }
                self.output.push('█');
            }
        }
        self.output.push_str("\x1b[0m");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fractional_area_coverage() {
        let mut renderer = Renderer::new(3, 1, 2, 1, (0, 0, 0), true).unwrap();
        assert_eq!(
            renderer
                .render(&[0, 0, 0, 255, 255, 255, 255, 255, 0, 0, 0, 255])
                .unwrap(),
            "\x1b[38;2;85;85;85m██\x1b[0m"
        );
    }

    #[test]
    fn transparency_and_opaque_override() {
        let pixels = [255, 0, 0, 255, 0, 255, 0, 0];
        let mut alpha = Renderer::new(2, 1, 1, 1, (0, 0, 255), false).unwrap();
        assert_eq!(
            alpha.render(&pixels).unwrap(),
            "\x1b[38;2;128;0;128m█\x1b[0m"
        );
        let mut opaque = Renderer::new(2, 1, 1, 1, (0, 0, 255), true).unwrap();
        assert_eq!(
            opaque.render(&pixels).unwrap(),
            "\x1b[38;2;128;128;0m█\x1b[0m"
        );
    }

    #[test]
    fn upscaling_fills_every_cell_and_reuses_renderer() {
        let mut renderer = Renderer::new(1, 1, 3, 2, (0, 0, 0), false).unwrap();
        assert_eq!(
            renderer.render(&[255, 0, 0, 255]).unwrap(),
            "\x1b[38;2;255;0;0m███\r\n███\x1b[0m"
        );
        assert_eq!(
            renderer.render(&[0, 255, 0, 255]).unwrap(),
            "\x1b[38;2;0;255;0m███\r\n███\x1b[0m"
        );
    }

    #[test]
    fn invalid_dimensions_and_buffers_return_errors() {
        assert!(Renderer::new(0, 1, 1, 1, (0, 0, 0), false).is_err());
        assert!(Renderer::new(1, 1, u64::MAX, 1, (0, 0, 0), false).is_err());
        assert!(Renderer::new(u32::MAX, u32::MAX, 1, 1, (0, 0, 0), false).is_err());
        assert!(
            Renderer::new(1, 1, 1, 1, (0, 0, 0), false)
                .unwrap()
                .render(&[0; 3])
                .is_err()
        );
    }
    fn ansi_pixels(mut ansi: &str) -> Vec<[u8; 3]> {
        let mut color = [0; 3];
        let mut pixels = Vec::new();
        while !ansi.is_empty() {
            if let Some(rest) = ansi.strip_prefix("\x1b[38;2;") {
                let (rgb, rest) = rest.split_once('m').unwrap();
                color = rgb
                    .split(';')
                    .map(|s| s.parse().unwrap())
                    .collect::<Vec<_>>()
                    .try_into()
                    .unwrap();
                ansi = rest;
            } else if let Some(rest) = ansi.strip_prefix('█') {
                pixels.push(color);
                ansi = rest;
            } else if let Some(rest) = ansi.strip_prefix("\x1b[0m") {
                ansi = rest;
            } else {
                ansi = &ansi[1..];
            }
        }
        pixels
    }

    #[test]
    fn separable_filter_matches_direct_area_reference() {
        // Independent, deliberately slow 2D overlap integration covers both
        // axes, fractional edges, upscaling, alpha, and single-pixel spans.
        for iw in [1, 2, 5, 8] {
            for ih in [1, 3, 7] {
                let pixels: Vec<u8> = (0..iw * ih * 4)
                    .map(|i| ((i * 71 + 17) % 256) as u8)
                    .collect();
                for ow in [1, 2, 3, 9] {
                    for oh in [1, 2, 5] {
                        for opaque in [false, true] {
                            let mut renderer =
                                Renderer::new(iw, ih, ow, oh, (21, 22, 28), opaque).unwrap();
                            let actual = ansi_pixels(renderer.render(&pixels).unwrap());
                            assert_eq!(actual.len(), (ow * oh) as usize);
                            for y in 0..oh {
                                for x in 0..ow {
                                    let left = x as f64 * iw as f64 / ow as f64;
                                    let right = (x + 1) as f64 * iw as f64 / ow as f64;
                                    let top = y as f64 * ih as f64 / oh as f64;
                                    let bottom = (y + 1) as f64 * ih as f64 / oh as f64;
                                    let mut expected = [0.0; 3];
                                    for sy in 0..ih {
                                        for sx in 0..iw {
                                            let coverage = (right.min(sx as f64 + 1.0)
                                                - left.max(sx as f64))
                                            .max(0.0)
                                                * (bottom.min(sy as f64 + 1.0)
                                                    - top.max(sy as f64))
                                                .max(0.0)
                                                / ((right - left) * (bottom - top));
                                            let p = ((sy * iw + sx) * 4) as usize;
                                            let alpha = if opaque {
                                                1.0
                                            } else {
                                                pixels[p + 3] as f64 / 255.0
                                            };
                                            for c in 0..3 {
                                                expected[c] += coverage
                                                    * (pixels[p + c] as f64 * alpha
                                                        + [21.0, 22.0, 28.0][c] * (1.0 - alpha));
                                            }
                                        }
                                    }
                                    for c in 0..3 {
                                        assert!(
                                            (actual[(y * ow + x) as usize][c] as f64 - expected[c])
                                                .abs()
                                                <= 0.500001
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
