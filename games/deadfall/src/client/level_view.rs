//! STUB (replaced by the level agent's real scene builder).
use crate::level::Level;
use macroquad::prelude::*;
use vesper3d::viewer::kit::{Look, PointLight, Template};

pub struct LevelScene {
    pub solid: Vec<Template>,
    pub glass: Template,
    pub decor: Vec<Template>,
    pub sky: Template,
    pub look: Look,
    pub lights: Vec<PointLight>,
}

pub fn build(level: &Level) -> LevelScene {
    let mut t = Template::new();
    for b in &level.blocks {
        let c = (b.min + b.max) * 0.5;
        let h = (b.max - b.min) * 0.5;
        t.box_(vec3(c.0, c.1, c.2), vec3(h.0, h.1, h.2), [0.45, 0.45, 0.48], 0.);
    }
    let mut sky = Template::new();
    sky.sky_dome(300., |e| [0.6 - 0.2 * e, 0.66 - 0.15 * e, 0.74], 24, 12);
    LevelScene { solid: t.split(), glass: Template::new(), decor: Vec::new(), sky, look: Look::daylight(), lights: Vec::new() }
}
