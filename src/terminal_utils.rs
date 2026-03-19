use terminal_size::{Height, Width, terminal_size};

pub fn console_wh() -> (u64, u64) {
    if let Some((Width(w), Height(h))) = terminal_size() {
        (w.max(1) as u64, h.max(1) as u64)
    } else {
        (80, 24u64)
    }
}

pub fn status_row_under_video_from_ansi(ansi: &str, terminal_h: u64) -> u64 {
    let video_rows = ansi.as_bytes().iter().filter(|&&b| b == b'\n').count() as u64 + 1;
    (video_rows + 1).min(terminal_h.max(1))
}
