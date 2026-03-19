pub fn compute_output_dims(iw: u32, ih: u32, tw: u64, th: u64, cw_px: u32, ch_px: u32) -> (u64, u64) {
    let cw_px = cw_px.max(1) as f64;
    let ch_px = ch_px.max(1) as f64;
    let h_from_w = ((tw as f64 * ih as f64 * cw_px) / (iw as f64 * ch_px))
        .round()
        .max(1.0) as u64;
    if h_from_w <= th {
        (tw, h_from_w)
    } else {
        let w_from_h = ((th as f64 * iw as f64 * ch_px) / (ih as f64 * cw_px))
            .round()
            .max(1.0)
            .min(tw as f64) as u64;
        (w_from_h, th)
    }
}

pub fn render_frame(
    rgba: &[u8],
    iw: u32,
    ih: u32,
    out_w: u64,
    out_h: u64,
    bg: (u8, u8, u8),
    opaque: bool,
) -> String {
    if opaque {
        crate::render::render_opaque(rgba, iw, ih, out_w, out_h, bg.0, bg.1, bg.2)
    } else {
        crate::render::render_alpha(rgba, iw, ih, out_w, out_h, bg.0, bg.1, bg.2)
    }
}
