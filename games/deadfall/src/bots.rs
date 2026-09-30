//! Stand-in bot: stands still. (Replaced below.)
use crate::input::Input;
use crate::sim::Match;
use vesper3d::viewer::devkit::Rng;

#[derive(Clone, Debug)]
pub struct BotState {}

impl BotState {
    pub fn new(_skill: u8, _rng: &mut Rng) -> Self {
        BotState {}
    }
}

pub fn name(n: usize) -> String {
    format!("Bot {}", n + 1)
}

pub fn think(_m: &mut Match, _slot: usize, _b: &mut BotState) -> Input {
    Input::default()
}
