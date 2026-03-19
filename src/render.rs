use std::fmt::Write as _;

const RESET: &str = "\x1b[0m\n";
const RESET_LAST: &str = "\x1b[0m";

#[inline(always)]
fn ansi_rgb(buf: &mut String, r: u8, g: u8, b: u8) {
    let _ = write!(buf, "\x1b[38;2;{};{};{}m", r, g, b);
}

pub fn render_opaque(
    px: &[u8],
    iw: u32,
    ih: u32,
    out_w: u64,
    out_h: u64,
    bg_r: u8,
    bg_g: u8,
    bg_b: u8,
) -> String {
    let cells = (out_w * out_h) as usize;
    let iw_u = iw as u64;
    let ih_u = ih as u64;

    let mut sum_r = vec![0u32; cells];
    let mut sum_g = vec![0u32; cells];
    let mut sum_b = vec![0u32; cells];
    let mut count = vec![0u32; cells];

    for y in 0..ih {
        let cy = (y as u64 * out_h) / ih_u;
        let row = cy * out_w;
        let base = (y as u64 * iw_u * 4) as usize;

        for x in 0..iw {
            let p = base + (x as usize * 4);
            let cx = (x as u64 * out_w) / iw_u;
            let idx = (row + cx) as usize;

            sum_r[idx] += px[p] as u32;
            sum_g[idx] += px[p + 1] as u32;
            sum_b[idx] += px[p + 2] as u32;
            count[idx] += 1;
        }
    }

    let mut out = String::with_capacity((out_w * out_h * 24) as usize);

    for y in 0..out_h {
        let row = (y * out_w) as usize;
        for x in 0..out_w {
            let i = row + x as usize;
            let c = count[i];

            if c == 0 {
                ansi_rgb(&mut out, bg_r, bg_g, bg_b);
            } else {
                let r = (sum_r[i] / c) as u8;
                let g = (sum_g[i] / c) as u8;
                let b = (sum_b[i] / c) as u8;
                ansi_rgb(&mut out, r, g, b);
            }
            out.push('█');
        }
        if y + 1 < out_h {
            out.push_str(RESET);
        } else {
            out.push_str(RESET_LAST);
        }
    }

    out
}

pub fn render_alpha(
    px: &[u8],
    iw: u32,
    ih: u32,
    out_w: u64,
    out_h: u64,
    bg_r: u8,
    bg_g: u8,
    bg_b: u8,
) -> String {
    let cells = (out_w * out_h) as usize;
    let iw_u = iw as u64;
    let ih_u = ih as u64;
    let bg_r64 = bg_r as u64;
    let bg_g64 = bg_g as u64;
    let bg_b64 = bg_b as u64;

    let mut sum_r = vec![0u64; cells];
    let mut sum_g = vec![0u64; cells];
    let mut sum_b = vec![0u64; cells];
    let mut sum_a = vec![0u64; cells];
    let mut count = vec![0u64; cells];

    for y in 0..ih {
        let cy = (y as u64 * out_h) / ih_u;
        let row = cy * out_w;
        let base = (y as u64 * iw_u * 4) as usize;

        for x in 0..iw {
            let p = base + (x as usize * 4);
            let a = px[p + 3] as u64;

            let cx = (x as u64 * out_w) / iw_u;
            let idx = (row + cx) as usize;

            sum_r[idx] += px[p] as u64 * a;
            sum_g[idx] += px[p + 1] as u64 * a;
            sum_b[idx] += px[p + 2] as u64 * a;
            sum_a[idx] += a;
            count[idx] += 1;
        }
    }

    let mut out = String::with_capacity((out_w * out_h * 24) as usize);

    for y in 0..out_h {
        let row = (y * out_w) as usize;
        for x in 0..out_w {
            let i = row + x as usize;
            let c = count[i];

            if c == 0 {
                ansi_rgb(&mut out, bg_r, bg_g, bg_b);
                out.push('█');
            } else {
                let total_a = sum_a[i];
                let max_a = c * 255;

                let fg_r = sum_r[i] / total_a.max(1);
                let fg_g = sum_g[i] / total_a.max(1);
                let fg_b = sum_b[i] / total_a.max(1);

                let r = ((fg_r * total_a + bg_r64 * (max_a - total_a)) / max_a) as u8;
                let g = ((fg_g * total_a + bg_g64 * (max_a - total_a)) / max_a) as u8;
                let b = ((fg_b * total_a + bg_b64 * (max_a - total_a)) / max_a) as u8;

                ansi_rgb(&mut out, r, g, b);
                out.push('█');
            }
        }
        if y + 1 < out_h {
            out.push_str(RESET);
        } else {
            out.push_str(RESET_LAST);
        }
    }

    out
}
