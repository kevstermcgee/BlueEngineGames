//! The two teams.

/// Ironclad wear army green and tan; Nightwatch wear navy and black.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Team {
    Ironclad,
    Nightwatch,
}

impl Team {
    pub const ALL: [Team; 2] = [Team::Ironclad, Team::Nightwatch];
    pub fn index(self) -> usize {
        self as usize
    }
    pub fn from_index(i: usize) -> Team {
        if i == 0 {
            Team::Ironclad
        } else {
            Team::Nightwatch
        }
    }
    pub fn other(self) -> Team {
        Team::from_index(1 - self.index())
    }
    pub fn name(self) -> &'static str {
        match self {
            Team::Ironclad => "Ironclad",
            Team::Nightwatch => "Nightwatch",
        }
    }
}
