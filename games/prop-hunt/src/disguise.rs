//! What a hider can pretend to be: a curated subset of BlueEngine's prop catalog
//! (`vesper3d::viewer::props::CATALOG`), picked for a spread of sizes that plausibly sit on a floor or
//! low surface. `half_extents` comes straight from the catalog, so a disguised hider's mesh
//! (`models.rs`) is sized exactly like the decoy props `layout.rs` scatters around the house; the
//! engine's own catalog is the one source of truth for how big each kind is.
use serde::{Deserialize, Serialize};
use vesper3d::math::V;
use vesper3d::viewer::props::{PropKind, CATALOG};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DisguiseKind {
    Chair,
    TableLamp,
    BookStack,
    CandleTrio,
    PottedCactus,
    FlowerVase,
    TallVase,
    MantelClock,
    WovenBasket,
    Bowl,
}

impl DisguiseKind {
    pub const ALL: [DisguiseKind; 10] = [
        DisguiseKind::Chair,
        DisguiseKind::TableLamp,
        DisguiseKind::BookStack,
        DisguiseKind::CandleTrio,
        DisguiseKind::PottedCactus,
        DisguiseKind::FlowerVase,
        DisguiseKind::TallVase,
        DisguiseKind::MantelClock,
        DisguiseKind::WovenBasket,
        DisguiseKind::Bowl,
    ];

    fn prop_kind(self) -> PropKind {
        match self {
            DisguiseKind::Chair => PropKind::Chair,
            DisguiseKind::TableLamp => PropKind::TableLamp,
            DisguiseKind::BookStack => PropKind::BookStack,
            DisguiseKind::CandleTrio => PropKind::CandleTrio,
            DisguiseKind::PottedCactus => PropKind::PottedCactus,
            DisguiseKind::FlowerVase => PropKind::FlowerVase,
            DisguiseKind::TallVase => PropKind::TallVase,
            DisguiseKind::MantelClock => PropKind::MantelClock,
            DisguiseKind::WovenBasket => PropKind::WovenBasket,
            DisguiseKind::Bowl => PropKind::Bowl,
        }
    }

    /// Half the size of the disguise's bounding box, in metres: `(x, y, z)` from the engine's own prop
    /// catalog, so rendering and any future size-based logic never drift from what `models.rs` draws.
    pub fn half_extents(self) -> V {
        CATALOG
            .iter()
            .find(|d| d.kind == self.prop_kind())
            .expect("every DisguiseKind has a catalog entry")
            .half_extents
    }

    pub fn label(self) -> &'static str {
        CATALOG.iter().find(|d| d.kind == self.prop_kind()).expect("every DisguiseKind has a catalog entry").label
    }

    /// Stable position in [`DisguiseKind::ALL`], used on the wire and for the hide-phase picker.
    pub fn index(self) -> u8 {
        Self::ALL.iter().position(|k| *k == self).expect("self is always in ALL") as u8
    }

    pub fn from_index(i: u8) -> Option<Self> {
        Self::ALL.get(i as usize).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_round_trips_through_its_index() {
        for kind in DisguiseKind::ALL {
            assert_eq!(DisguiseKind::from_index(kind.index()), Some(kind));
        }
        assert_eq!(DisguiseKind::from_index(DisguiseKind::ALL.len() as u8), None);
        assert_eq!(DisguiseKind::from_index(255), None);
    }

    #[test]
    fn every_kind_has_a_plausible_floor_sized_half_extent() {
        for kind in DisguiseKind::ALL {
            let h = kind.half_extents();
            assert!(h.0 > 0. && h.1 > 0. && h.2 > 0., "{kind:?} has a non-positive extent: {h:?}");
            assert!(h.0 < 1. && h.1 < 1. && h.2 < 1., "{kind:?} is implausibly large for a hiding spot: {h:?}");
            assert!(!kind.label().is_empty());
        }
    }

    #[test]
    fn indices_are_dense_and_unique() {
        let mut seen: Vec<u8> = DisguiseKind::ALL.iter().map(|k| k.index()).collect();
        seen.sort();
        assert_eq!(seen, (0..DisguiseKind::ALL.len() as u8).collect::<Vec<_>>());
    }
}
