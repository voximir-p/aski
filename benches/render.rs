use aski::Renderer;
use std::hint::black_box;
use std::time::{Duration, Instant};

fn main() {
    for (width, height) in [(640, 360), (1920, 1080), (3840, 2160)] {
        let pixels: Vec<u8> = (0..width * height * 4)
            .map(|i| ((i * 13 + i / 127) % 256) as u8)
            .collect();
        for opaque in [true, false] {
            let mut renderer = Renderer::new(width, height, 80, 22, (21, 22, 28), opaque).unwrap();
            black_box(renderer.render(&pixels).unwrap());
            let start = Instant::now();
            let mut frames = 0;
            let mut bytes = 0;
            while start.elapsed() < Duration::from_millis(500) {
                bytes = black_box(renderer.render(black_box(&pixels)).unwrap()).len();
                frames += 1;
            }
            println!(
                "{width}x{height} -> 80x22 | opaque={opaque} | {:.3} ms/frame | {bytes} bytes/frame",
                start.elapsed().as_secs_f64() * 1000.0 / frames as f64
            );
        }
    }
}
