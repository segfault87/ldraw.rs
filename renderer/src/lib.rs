pub mod camera;
pub mod display_list;
pub mod part;
pub mod pipeline;
pub mod util;

#[derive(Copy, Clone, Debug, PartialEq, PartialOrd)]
pub struct AspectRatio(f32);

impl From<(u32, u32)> for AspectRatio {
    fn from((width, height): (u32, u32)) -> Self {
        Self(width as f32 / height as f32)
    }
}

impl From<AspectRatio> for f32 {
    fn from(value: AspectRatio) -> f32 {
        value.0
    }
}

impl From<f32> for AspectRatio {
    fn from(value: f32) -> Self {
        Self(value)
    }
}
