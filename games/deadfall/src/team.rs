//! The two teams of Deadfall.

/// Ironclad (army green, tan gear) and Nightwatch (navy, black gear).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Team {
    Ironclad,
    Nightwatch,
}

impl Team {
    pub const ALL: [Team; 2] = [Team::Ironclad, Team::Nightwatch];

    /// 0 for Ironclad, 1 for Nightwatch (wire and array index).
    pub fn index(self) -> usize {
        match self {
            Team::Ironclad => 0,
            Team::Nightwatch => 1,
        }
    }

    /// Inverse of [`Team::index`]; anything but 0 is Nightwatch.
    pub fn from_index(i: usize) -> Team {
        if i == 0 {
            Team::Ironclad
        } else {
            Team::Nightwatch
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Team::Ironclad => "Ironclad",
            Team::Nightwatch => "Nightwatch",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_roundtrip() {
        for t in Team::ALL {
            assert_eq!(Team::from_index(t.index()), t);
        }
        assert_eq!(Team::Ironclad.index(), 0);
        assert_eq!(Team::Nightwatch.index(), 1);
    }
}
