//! What a weapon model tells everything that holds it. Weapon-local space: the origin is the centre of the
//! firing hand's grip, -Z points out of the muzzle, +Y is up, +X is to the weapon's right. Metres.
use macroquad::prelude::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct WeaponAnchors {
    /// Centre of the firing hand's palm.
    pub grip: Vec3,
    /// Where the other hand holds it (handguard, foregrip, pump, rocket tube). `None` for one-handed weapons
    /// (pistols, knives, grenades): the support hand then hangs ready or is empty.
    pub support: Option<Vec3>,
    /// Where the aiming eye sits relative to the weapon when looking through its sights (iron sights, dot or
    /// scope eyepiece): the first-person view is moved so this point lands on the camera.
    pub sight: Vec3,
    /// Muzzle (tracers and flash start here); for a thrown weapon, where it leaves the hand.
    pub muzzle: Vec3,
    /// Where spent cases come out (right side), for effects. May equal the grip for weapons that do not eject.
    pub eject: Vec3,
}
