/// Fit media into terminal cells, correcting for the cell's physical aspect ratio.
/// Returns `(0, 0)` if any input dimension is zero. Small images may be enlarged.
pub fn compute_output_dims(
    iw: u32,
    ih: u32,
    tw: u64,
    th: u64,
    cw_px: u32,
    ch_px: u32,
) -> (u64, u64) {
    if iw == 0 || ih == 0 || tw == 0 || th == 0 || cw_px == 0 || ch_px == 0 {
        return (0, 0);
    }
    let ratio = (iw as f64 * ch_px as f64) / (ih as f64 * cw_px as f64);
    if tw as f64 / th as f64 <= ratio {
        (
            tw,
            (tw as f64 / ratio).round().max(1.0).min(th as f64) as u64,
        )
    } else {
        (
            (th as f64 * ratio).round().max(1.0).min(tw as f64) as u64,
            th,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fits_wide_tall_and_tiny_images() {
        assert_eq!(compute_output_dims(1920, 1080, 80, 22, 10, 22), (80, 20));
        assert_eq!(compute_output_dims(100, 1000, 80, 22, 10, 22), (5, 22));
        assert_eq!(compute_output_dims(1, 1, 80, 22, 10, 22), (48, 22));
        assert_eq!(compute_output_dims(1, 1, 0, 22, 10, 22), (0, 0));
        assert_eq!(compute_output_dims(0, 1, 80, 22, 10, 22), (0, 0));
    }
}
