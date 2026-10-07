//! Authoritative objectives shared by clients, bots and simulation tests.
use crate::sim::{Match, Phase};
use crate::team::Team;
use vesper3d::math::V;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GameMode {
    #[default]
    TeamDeathmatch = 0,
    CaptureFlag = 1,
    SearchDestroy = 2,
    FreeForAll = 3,
}
impl GameMode {
    pub const ALL: [Self; 4] = [Self::TeamDeathmatch, Self::CaptureFlag, Self::SearchDestroy, Self::FreeForAll];
    pub fn from_id(id: u8) -> Self {
        Self::ALL[id.min(3) as usize]
    }
    pub fn name(self) -> &'static str {
        ["Team Deathmatch", "Capture the Flag", "Search and Destroy", "Free for All"][self as usize]
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flag {
    pub pos: V,
    pub carrier: u8,
    pub dropped_at: u32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Objectives {
    pub flags: [Flag; 2],
    pub round: u16,
    /// 0 preparation, 1 active, 2 planted, 3 round result.
    pub phase: u8,
    pub deadline: u32,
    pub bomb: V,
    pub carrier: u8,
    pub site: u8,
    pub actor: u8,
    pub progress: u16,
    pub winner: u8,
}
impl Objectives {
    pub fn new(bases: [V; 2]) -> Self {
        Self {
            flags: bases.map(|pos| Flag { pos, carrier: 255, dropped_at: 0 }),
            round: 1,
            phase: 0,
            deadline: 180,
            bomb: bases[0],
            carrier: 255,
            site: 255,
            actor: 255,
            progress: 0,
            winner: 255,
        }
    }
}
fn near(a: V, b: V, r: f32) -> bool {
    (a - b).length() < r
}
impl Match {
    pub fn enemies(&self, a: usize, b: usize) -> bool {
        a != b && (self.settings.mode == GameMode::FreeForAll || self.players[a].team != self.players[b].team)
    }
    pub fn objective_goal(&self, slot: usize) -> Option<V> {
        let p = &self.players[slot];
        let t = p.team.index();
        let o = self.objective;
        match self.settings.mode {
            GameMode::CaptureFlag => Some(if o.flags[1 - t].carrier == slot as u8 {
                self.settings.map.bases()[t]
            } else if o.flags[t].dropped_at != 0 {
                o.flags[t].pos
            } else {
                o.flags[1 - t].pos
            }),
            GameMode::SearchDestroy if o.phase < 3 => Some(if o.phase == 2 {
                o.bomb
            } else if t == (o.round as usize - 1) % 2 {
                if o.carrier == slot as u8 {
                    self.settings.map.sites()[slot % 2]
                } else {
                    o.bomb
                }
            } else {
                self.settings.map.sites()[slot % 2]
            }),
            _ => None,
        }
    }
    pub(crate) fn step_objectives(&mut self) {
        if self.phase != Phase::Live {
            return;
        }
        match self.settings.mode {
            GameMode::CaptureFlag => self.step_flags(),
            GameMode::SearchDestroy => self.step_bomb(),
            _ => {}
        }
    }
    fn step_flags(&mut self) {
        let bases = self.settings.map.bases();
        for (team, home) in bases.into_iter().enumerate() {
            let flag = &mut self.objective.flags[team];
            if flag.carrier != 255 {
                let p = &self.players[flag.carrier as usize];
                flag.pos = p.feet();
                if !p.alive {
                    flag.carrier = 255;
                    flag.dropped_at = self.tick;
                }
            }
            if flag.dropped_at != 0 && self.tick.saturating_sub(flag.dropped_at) >= 1200 {
                *flag = Flag { pos: home, carrier: 255, dropped_at: 0 };
            }
            if flag.carrier == 255 {
                // Return has priority when two opponents touch a dropped flag together.
                if flag.dropped_at != 0
                    && self.players.iter().any(|p| p.alive && p.team.index() == team && near(p.feet(), flag.pos, 1.5))
                {
                    *flag = Flag { pos: home, carrier: 255, dropped_at: 0 };
                } else if let Some(p) =
                    self.players.iter().find(|p| p.alive && p.team.index() != team && near(p.feet(), flag.pos, 1.5))
                {
                    flag.carrier = p.slot as u8;
                    flag.dropped_at = 0;
                }
            }
        }
        for (t, home) in bases.into_iter().enumerate() {
            let enemy = self.objective.flags[1 - t];
            let own = self.objective.flags[t];
            if enemy.carrier != 255 && own.carrier == 255 && own.dropped_at == 0 && near(enemy.pos, home, 1.8) {
                self.scores[t] += 1;
                self.objective.flags[1 - t] = Flag { pos: bases[1 - t], carrier: 255, dropped_at: 0 };
            }
        }
    }
    fn start_round(&mut self) {
        let attack = (self.objective.round as usize - 1) % 2;
        for slot in 0..self.players.len() {
            self.spawn(slot);
        }
        self.projectiles.clear();
        self.zones.clear();
        self.dropped.clear();
        self.history.clear();
        for l in &mut self.loot {
            l.available = true;
            l.back_at = 0;
        }
        self.objective.phase = 1;
        self.objective.deadline = self.tick + 90 * 60;
        self.objective.carrier = self.players.iter().find(|p| p.team.index() == attack).map_or(255, |p| p.slot as u8);
        self.objective.bomb = self.settings.map.bases()[attack];
        self.objective.site = 255;
        self.objective.actor = 255;
        self.objective.progress = 0;
    }
    fn round_over(&mut self, winner: usize) {
        self.scores[winner] += 1;
        self.objective.phase = 3;
        self.objective.winner = winner as u8;
        self.objective.deadline = self.tick + 4 * 60;
        self.objective.progress = 0;
        self.objective.actor = 255;
        if self.scores[winner] >= self.settings.objective_target {
            self.finish(Some(Team::from_index(winner)), winner as u8);
        }
    }
    fn step_bomb(&mut self) {
        let o = self.objective;
        if o.phase == 0 || o.phase == 3 {
            if self.tick >= o.deadline {
                if o.phase == 3 {
                    self.objective.round += 1;
                }
                self.start_round();
            }
            return;
        }
        let attack = (o.round as usize - 1) % 2;
        let defend = 1 - attack;
        if o.phase == 1 {
            if o.carrier != 255 {
                let p = &self.players[o.carrier as usize];
                self.objective.bomb = p.feet();
                if !p.alive {
                    self.objective.carrier = 255;
                }
            } else if let Some(p) =
                self.players.iter().find(|p| p.alive && p.team.index() == attack && near(p.feet(), o.bomb, 1.5))
            {
                self.objective.carrier = p.slot as u8;
            }
            let candidate = self.players.iter().find(|p| {
                p.alive
                    && p.slot as u8 == self.objective.carrier
                    && p.last_input.held(crate::input::USE_HELD)
                    && self.settings.map.sites().iter().any(|s| near(p.feet(), *s, 2.))
            });
            let actor = candidate.map_or(255, |p| p.slot as u8);
            self.interact(actor, 180);
            if actor != 255 && self.objective.progress >= 180 {
                let feet = self.players[actor as usize].feet();
                let sites = self.settings.map.sites();
                let site = if near(feet, sites[0], 2.) { 0 } else { 1 };
                self.objective.phase = 2;
                self.objective.site = site;
                self.objective.bomb = sites[site as usize];
                self.objective.carrier = 255;
                self.objective.deadline = self.tick + 35 * 60;
                self.objective.progress = 0;
                self.objective.actor = 255;
            } else if !self.players.iter().any(|p| p.alive && p.team.index() == attack) || self.tick >= o.deadline {
                self.round_over(defend);
            } else if !self.players.iter().any(|p| p.alive && p.team.index() == defend) {
                self.round_over(attack);
            }
        } else {
            let actor = self
                .players
                .iter()
                .find(|p| {
                    p.alive
                        && p.team.index() == defend
                        && p.last_input.held(crate::input::USE_HELD)
                        && near(p.feet(), o.bomb, 2.)
                })
                .map_or(255, |p| p.slot as u8);
            self.interact(actor, 300);
            // The deadline wins ties: a last-tick defuse is too late.
            if self.tick >= o.deadline {
                self.events.push(crate::sim::Event::Blast { pos: o.bomb, kind: 0, radius: 10. });
                self.round_over(attack);
            } else if actor != 255 && self.objective.progress >= 300 {
                self.round_over(defend);
            } else if !self.players.iter().any(|p| p.alive && p.team.index() == defend) {
                self.round_over(attack);
            }
        }
    }
    fn interact(&mut self, actor: u8, max: u16) {
        if actor == 255 || actor != self.objective.actor {
            self.objective.progress = 0;
        }
        self.objective.actor = actor;
        if actor != 255 {
            self.objective.progress = (self.objective.progress + 1).min(max);
        }
    }
}
