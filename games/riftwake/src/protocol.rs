use crate::WeaponKind;
use serde::{Deserialize, Serialize};
use vesper3d::{math::V, viewer::arena::ArenaInput};

pub const PROTOCOL_VERSION: u32 = 1;
// This trusted-session prototype sends full state snapshots. The budget covers the
// advertised twelve-player arena plus active projectiles without silent broadcast loss.
pub const MAX_PACKET_BYTES: usize = 4096;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClientInput {
    pub sequence: u64,
    pub movement: ArenaInput,
    pub yaw: f32,
    pub pitch: f32,
    pub weapon: WeaponKind,
    pub fire_counter: u32,
    pub fire_held: bool,
}
impl ClientInput {
    pub fn valid(&self) -> bool {
        self.yaw.is_finite()
            && self.pitch.is_finite()
            && self.pitch.abs() <= 1.5
            && self.movement.forward.is_finite()
            && self.movement.right.is_finite()
            && self.movement.forward.abs() <= 1.0
            && self.movement.right.abs() <= 1.0
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerSnapshot {
    pub id: u64,
    pub position: V,
    pub velocity: V,
    pub yaw: f32,
    pub pitch: f32,
    pub health: u16,
    pub armor: u16,
    pub frags: i32,
    pub deaths: u32,
    pub weapon: WeaponKind,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectileSnapshot {
    pub position: V,
    pub weapon: WeaponKind,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PickupSnapshot {
    pub id: String,
    pub available: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MatchSnapshot {
    pub tick: u64,
    pub frag_limit: u32,
    pub winner: Option<u64>,
    pub players: Vec<PlayerSnapshot>,
    pub projectiles: Vec<ProjectileSnapshot>,
    pub pickups: Vec<PickupSnapshot>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum WireMessage {
    Join {
        version: u32,
        key: String,
        name: String,
    },
    Welcome {
        player_id: u64,
        token: [u8; 16],
    },
    Reject {
        reason: String,
    },
    Input {
        token: [u8; 16],
        input: ClientInput,
    },
    Snapshot {
        token: [u8; 16],
        state: MatchSnapshot,
    },
}
impl WireMessage {
    pub fn encode(&self) -> vesper3d::Result<Vec<u8>> {
        let bytes = serde_json::to_vec(self)?;
        if bytes.len() > MAX_PACKET_BYTES {
            return Err("Riftwake packet exceeds transport budget".into());
        }
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        (bytes.len() <= MAX_PACKET_BYTES)
            .then(|| serde_json::from_slice(bytes).ok())
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advertised_twelve_player_snapshot_fits_transport_budget() {
        let players = (1..=12)
            .map(|id| PlayerSnapshot {
                id,
                position: V(id as f32, 1.56, -10.0),
                velocity: V(10.5, 0.0, 2.0),
                yaw: 1.2,
                pitch: -0.1,
                health: 100,
                armor: 100,
                frags: 19,
                deaths: 9,
                weapon: WeaponKind::Rocket,
            })
            .collect();
        let state = MatchSnapshot {
            tick: 99_999,
            frag_limit: 20,
            winner: None,
            players,
            projectiles: (0..8)
                .map(|index| ProjectileSnapshot {
                    position: V(index as f32, 2.0, 3.0),
                    weapon: WeaponKind::Rocket,
                })
                .collect(),
            pickups: (0..5)
                .map(|index| PickupSnapshot {
                    id: format!("pickup-{index}"),
                    available: true,
                })
                .collect(),
        };
        WireMessage::Snapshot {
            token: [42; 16],
            state,
        }
        .encode()
        .unwrap();
    }
}
