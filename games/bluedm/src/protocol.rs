use serde::{Deserialize, Serialize};
use vesper3d::{
    math::V,
    viewer::{
        controller::Movement,
        fps::{OperativeModel, Team},
    },
};

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_PACKET_BYTES: usize = 4096;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClientInput {
    pub sequence: u64,
    pub movement: Movement,
    pub yaw: f32,
    pub pitch: f32,
    pub weapon_slot: u8,
    pub fire_counter: u32,
    pub fire_held: bool,
    pub reload_counter: u32,
    pub ads: bool,
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
            && self.weapon_slot < 10
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerSnapshot {
    pub id: u64,
    pub position: V,
    pub yaw: f32,
    pub pitch: f32,
    pub team: Team,
    pub model: OperativeModel,
    pub health: u16,
    pub kills: u32,
    pub deaths: u32,
    pub weapon_slot: u8,
    pub magazine: u16,
    pub reserve: u16,
    pub reloading: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MatchSnapshot {
    pub tick: u64,
    pub team_scores: [u32; 2],
    pub score_limit: u32,
    pub winner: Option<Team>,
    pub players: Vec<PlayerSnapshot>,
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
            return Err("BlueDM packet exceeds the 4096-byte transport budget".into());
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
    fn full_eight_player_match_fits_snapshot_budget() {
        let state = MatchSnapshot {
            tick: u64::MAX,
            team_scores: [99, 99],
            score_limit: 100,
            winner: None,
            players: (1..=8)
                .map(|id| PlayerSnapshot {
                    id,
                    position: V(-19.123456, 1.68, 3.612345),
                    yaw: 1.2345678,
                    pitch: -1.1234567,
                    team: Team::Crimson,
                    model: OperativeModel::FieldTech,
                    health: 100,
                    kills: 999,
                    deaths: 999,
                    weapon_slot: 4,
                    magazine: 30,
                    reserve: 120,
                    reloading: false,
                })
                .collect(),
        };
        let message = WireMessage::Snapshot {
            token: [255; 16],
            state,
        };
        let bytes = message.encode().unwrap();
        assert!(WireMessage::decode(&bytes).is_some());
    }
}
