//! STUB (replaced by the weapon-model agent's real models).
use super::anchors::WeaponAnchors;
use macroquad::prelude::*;
use vesper3d::viewer::kit::Template;

pub struct WeaponModel {
    pub body: Template,
    pub anchors: WeaponAnchors,
    pub length: f32,
    pub mag: Option<(Template, Vec3)>,
    pub slide: Option<(Template, Vec3)>,
}

pub fn keys() -> &'static [&'static str] {
    &[]
}

pub fn build(_key: &str) -> Option<WeaponModel> {
    let mut body = Template::new();
    body.box_(vec3(0., 0.03, -0.3), vec3(0.03, 0.05, 0.35), [0.15, 0.15, 0.17], 0.);
    Some(WeaponModel {
        body,
        anchors: WeaponAnchors { grip: Vec3::ZERO, support: Some(vec3(0., -0.02, -0.4)), sight: vec3(0., 0.09, 0.05), muzzle: vec3(0., 0.03, -0.65), eject: vec3(0.03, 0.05, -0.1) },
        length: 0.7,
        mag: None,
        slide: None,
    })
}
