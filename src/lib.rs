//! Image and animation rendering for true-color terminals.
//!
//! [`Renderer`] draws two vertically stacked pixels per terminal cell using `▀`
//! with foreground and background colors, reusing sampling geometry and buffers
//! across frames. Use [`compute_output_dims`] to fit media into whole terminal
//! cells while preserving its aspect ratio and [`open_stream`] to decode GIFs or
//! videos lazily. [`frames::open_stdin_stream`] decodes piped media as it arrives.
//! The library never changes terminal state or installs signal handlers.
mod color;
pub mod frames;
mod media_render;
mod render;

pub use color::{Color, ParseColorError, parse_color};
pub use frames::{DecodeError, Frame, FrameStream, MediaType, open_stream};
pub use media_render::compute_output_dims;
pub use render::{RenderError, Renderer};
