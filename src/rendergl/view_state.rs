
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize, Copy)]
#[serde(rename_all = "lowercase")]
pub struct ViewState {
    pub zoom: Zoom,
    pub pos: Position,
    pub frame: Option<u32>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Position {
    Relative((f32, f32)),
    Aboslute((f32, f32)),
}

#[derive(Debug, Clone, Serialize, Deserialize, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Zoom {
    Fit(f32),
    Pixel(f32),
}

impl ViewState {
    pub fn new() -> Self {
        ViewState {
            zoom: Zoom::Fit(1.0),
            pos: Position::Relative((0.0, 0.0)),
            frame: None,
        }
    }

    pub fn for_pointer(position: Option<(f32, f32)>) -> Option<Self> {
        if let Some(position) = position {
            return Some(ViewState {
                zoom: Zoom::Pixel(1.0),
                pos: Position::Aboslute(position),
                frame: None,
            });
        }
        None
    }

    pub fn set_zoom_mode(&mut self, z: Zoom) {
        self.zoom = z;
    }

    pub fn update_magnification(&mut self, mag: f32) {
        match self.zoom {
            Zoom::Fit(ref mut current) => *current *= mag,
            Zoom::Pixel(ref mut current) => *current *= mag,
        }
    }

    pub fn set_position(&mut self, pos: (f32, f32)) {
        match self.pos {
            Position::Relative(ref mut p) => *p = pos,
            Position::Aboslute(ref mut p) => *p = pos,
        }
    }
}
