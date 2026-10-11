use terminal_size::{Height, Width, terminal_size};

pub fn console_wh() -> (u64, u64) {
    if let Some((Width(w), Height(h))) = terminal_size() {
        (w.max(1) as u64, h.max(1) as u64)
    } else {
        (80, 24u64)
    }
}
