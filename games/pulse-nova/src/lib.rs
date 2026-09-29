//! Pulse Nova: high-velocity kinetic arena shooter.
//!
//! Blast swarms of cyber drones, chain multi-kill explosions, leap off jump pads,
//! and survive 5 intense waves in a neon colosseum floating above the void.
//!
//! A pure, deterministic 60 Hz simulation with no window, no sound device, and no wall clock.
use serde::{Deserialize, Serialize};
use vesper3d::math::V;
use vesper3d::viewer::controller::{Collider, Controller, ControllerState, Movement};
use vesper3d::viewer::devkit::{Rng, Simulation, Snapshot, StateHasher, TICK};

pub const ARENA_HALF: f32 = 14.5;
pub const VOID_Y: f32 = -8.0;
pub const MAX_HEALTH: f32 = 100.0;
pub const TOTAL_WAVES: u32 = 5;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Input {
    pub forward: f32,
    pub right: f32,
    pub look: [f32; 2],
    pub jump: bool,
    pub fire: bool,
    pub dash: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Fired { at: V, dir: V },
    Dashed { at: V },
    EnemyHit { at: V, fatal: bool },
    EnemyExploded { at: V, radius: f32 },
    PlayerHit { health: f32 },
    ShardCollected { at: V, score: u32 },
    JumpPadUsed { at: V },
    Jumped,
    Landed,
    WaveCleared { wave: u32 },
    WaveStarted { wave: u32 },
    Won,
    Lost,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnemyKind {
    Scout,
    Sentinel,
    Goliath,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Enemy {
    pub id: u32,
    pub kind: EnemyKind,
    pub pos: V,
    pub vel: V,
    pub health: f32,
    pub max_health: f32,
    pub cooldown: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Projectile {
    pub pos: V,
    pub vel: V,
    pub from_player: bool,
    pub lifetime: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Shard {
    pub pos: V,
    pub lifetime: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct JumpPad {
    pub pos: V,
}

pub struct Sim {
    pub tick: u64,
    pub player: Controller,
    pub health: f32,
    pub score: u32,
    pub combo: u32,
    pub combo_timer: u32,
    pub wave: u32,
    pub wave_delay: u32,
    pub over: bool,
    pub won: bool,
    pub dash_cooldown: u32,
    pub fire_cooldown: u32,
    pub pad_cooldown: u32,
    pub enemies: Vec<Enemy>,
    pub projectiles: Vec<Projectile>,
    pub shards: Vec<Shard>,
    pub jump_pads: Vec<JumpPad>,
    colliders: Vec<Collider>,
    rng: Rng,
    events: Vec<Event>,
    was_grounded: bool,
    next_enemy_id: u32,
}

impl Sim {
    pub fn new(seed: u64) -> Self {
        let mut player = Controller::for_profile(Default::default(), V(0., 0., 8.), 0.)
            .expect("the default profile is valid");
        player.set_floor(None);

        let colliders = vec![
            // Main arena floor
            Collider {
                min: V(-ARENA_HALF, -1.0, -ARENA_HALF),
                max: V(ARENA_HALF, 0.0, ARENA_HALF),
            },
            // Central dais
            Collider {
                min: V(-3.5, 0.0, -3.5),
                max: V(3.5, 1.0, 3.5),
            },
            // 4 tactical corner pillars
            Collider { min: V(-7.8, 0.0, -7.8), max: V(-6.2, 3.5, -6.2) },
            Collider { min: V(6.2, 0.0, -7.8), max: V(7.8, 3.5, -6.2) },
            Collider { min: V(-7.8, 0.0, 6.2), max: V(-6.2, 3.5, 7.8) },
            Collider { min: V(6.2, 0.0, 6.2), max: V(7.8, 3.5, 7.8) },
            // Perimeter curbs
            Collider { min: V(-ARENA_HALF, 0.0, ARENA_HALF - 0.5), max: V(ARENA_HALF, 0.7, ARENA_HALF) },
            Collider { min: V(-ARENA_HALF, 0.0, -ARENA_HALF), max: V(ARENA_HALF, 0.7, -ARENA_HALF + 0.5) },
            Collider { min: V(ARENA_HALF - 0.5, 0.0, -ARENA_HALF), max: V(ARENA_HALF, 0.7, ARENA_HALF) },
            Collider { min: V(-ARENA_HALF, 0.0, -ARENA_HALF), max: V(-ARENA_HALF + 0.5, 0.7, ARENA_HALF) },
        ];

        let jump_pads = vec![
            JumpPad { pos: V(0., 0.05, -5.5) },
            JumpPad { pos: V(0., 0.05, 5.5) },
            JumpPad { pos: V(-5.5, 0.05, 0.) },
            JumpPad { pos: V(5.5, 0.05, 0.) },
        ];

        Self {
            tick: 0,
            player,
            health: MAX_HEALTH,
            score: 0,
            combo: 0,
            combo_timer: 0,
            wave: 1,
            wave_delay: 30, // brief delay before wave 1 spawns
            over: false,
            won: false,
            dash_cooldown: 0,
            fire_cooldown: 0,
            pad_cooldown: 0,
            enemies: Vec::new(),
            projectiles: Vec::new(),
            shards: Vec::new(),
            jump_pads,
            colliders,
            rng: Rng::new(seed),
            events: Vec::new(),
            was_grounded: true,
            next_enemy_id: 1,
        }
    }

    pub fn spawn_wave(&mut self, wave: u32) {
        self.events.push(Event::WaveStarted { wave });
        let (scouts, sentinels, goliaths) = match wave {
            1 => (4, 0, 0),
            2 => (4, 2, 0),
            3 => (4, 3, 1),
            4 => (6, 4, 2),
            _ => (8, 6, 3),
        };

        for _ in 0..scouts {
            self.spawn_enemy(EnemyKind::Scout);
        }
        for _ in 0..sentinels {
            self.spawn_enemy(EnemyKind::Sentinel);
        }
        for _ in 0..goliaths {
            self.spawn_enemy(EnemyKind::Goliath);
        }
    }

    fn spawn_enemy(&mut self, kind: EnemyKind) {
        let angle = self.rng.range(0.0, std::f32::consts::TAU);
        let dist = self.rng.range(9.0, 13.5);
        let x = angle.cos() * dist;
        let z = angle.sin() * dist;
        let (health, y) = match kind {
            EnemyKind::Scout => (25.0, 1.2),
            EnemyKind::Sentinel => (50.0, 2.2),
            EnemyKind::Goliath => (120.0, 1.6),
        };

        let id = self.next_enemy_id;
        self.next_enemy_id += 1;
        self.enemies.push(Enemy {
            id,
            kind,
            pos: V(x, y, z),
            vel: V(0., 0., 0.),
            health,
            max_health: health,
            cooldown: match kind {
                EnemyKind::Scout => 0,
                EnemyKind::Sentinel => self.rng.range(40.0, 90.0) as u32,
                EnemyKind::Goliath => self.rng.range(60.0, 120.0) as u32,
            },
        });
    }

    pub fn step(&mut self, input: &Input) {
        if self.over {
            return;
        }
        self.tick += 1;

        // Player look & movement
        self.player.look(input.look[0], input.look[1], 1., false);
        self.player.update(
            Movement {
                forward: input.forward,
                right: input.right,
                jump: input.jump,
                ..Default::default()
            },
            TICK,
            &self.colliders,
        );

        let grounded = self.player.is_grounded();
        if input.jump && self.was_grounded && !grounded {
            self.events.push(Event::Jumped);
        }
        if grounded && !self.was_grounded {
            self.events.push(Event::Landed);
        }
        self.was_grounded = grounded;

        // Cooldowns
        self.dash_cooldown = self.dash_cooldown.saturating_sub(1);
        self.fire_cooldown = self.fire_cooldown.saturating_sub(1);
        self.pad_cooldown = self.pad_cooldown.saturating_sub(1);

        // Combo timer
        if self.combo_timer > 0 {
            self.combo_timer -= 1;
            if self.combo_timer == 0 {
                self.combo = 0;
            }
        }

        let p_pos = self.player.position;
        let feet = V(p_pos.0, self.player.feet_height(), p_pos.2);
        let center = V(p_pos.0, feet.1 + 0.8, p_pos.2);

        // Jump pads
        if self.pad_cooldown == 0 {
            for pad in &self.jump_pads {
                let dx = feet.0 - pad.pos.0;
                let dz = feet.2 - pad.pos.2;
                let dist_sq = dx * dx + dz * dz;
                if dist_sq < 1.4 && (feet.1 - pad.pos.1).abs() < 0.6 {
                    self.player.apply_impulse(V(0., 15.5, 0.));
                    self.pad_cooldown = 30;
                    self.events.push(Event::JumpPadUsed { at: pad.pos });
                    break;
                }
            }
        }

        // Player Dash
        if input.dash && self.dash_cooldown == 0 {
            let yaw = self.player.yaw;
            let fwd = V(-yaw.sin(), 0., -yaw.cos());
            let rgt = V(yaw.cos(), 0., -yaw.sin());
            let wish = fwd * input.forward + rgt * input.right;
            let dash_dir = if wish.length() > 0.1 { wish.norm() } else { fwd };
            self.player.apply_impulse(dash_dir * 18.0 + V(0., 2.0, 0.));
            self.dash_cooldown = 32;
            self.events.push(Event::Dashed { at: center });
        }

        // Player Fire
        if input.fire && self.fire_cooldown == 0 {
            let yaw = self.player.yaw;
            let pitch = self.player.pitch;
            let cp = pitch.cos();
            let dir = V(-yaw.sin() * cp, pitch.sin(), -yaw.cos() * cp).norm();
            let spawn_pos = center + dir * 0.7;
            self.projectiles.push(Projectile {
                pos: spawn_pos,
                vel: dir * 44.0,
                from_player: true,
                lifetime: 100,
            });
            self.fire_cooldown = 11;
            self.events.push(Event::Fired { at: spawn_pos, dir });
        }

        // Update Projectiles
        let mut expired_or_hit = Vec::new();
        for (pi, proj) in self.projectiles.iter_mut().enumerate() {
            proj.pos = proj.pos + proj.vel * TICK;
            proj.lifetime = proj.lifetime.saturating_sub(1);
            if proj.lifetime == 0
                || proj.pos.0.abs() > ARENA_HALF + 2.0
                || proj.pos.2.abs() > ARENA_HALF + 2.0
                || proj.pos.1 < -2.0
                || proj.pos.1 > 15.0
            {
                expired_or_hit.push(pi);
            }
        }

        // Projectile vs Enemy collision
        let mut dead_enemies = Vec::new();
        for (pi, proj) in self.projectiles.iter_mut().enumerate() {
            if !proj.from_player || expired_or_hit.contains(&pi) {
                continue;
            }
            for enemy in &mut self.enemies {
                let hit_radius = match enemy.kind {
                    EnemyKind::Scout => 0.85,
                    EnemyKind::Sentinel => 1.05,
                    EnemyKind::Goliath => 1.6,
                };
                if (enemy.pos - proj.pos).length() < hit_radius {
                    enemy.health -= 25.0;
                    expired_or_hit.push(pi);
                    let fatal = enemy.health <= 0.0;
                    self.events.push(Event::EnemyHit { at: proj.pos, fatal });
                    if fatal && !dead_enemies.contains(&enemy.id) {
                        dead_enemies.push(enemy.id);
                    }
                    break;
                }
            }
        }

        // Projectile vs Player collision
        for (pi, proj) in self.projectiles.iter_mut().enumerate() {
            if proj.from_player || expired_or_hit.contains(&pi) {
                continue;
            }
            if (center - proj.pos).length() < 0.9 {
                expired_or_hit.push(pi);
                self.health = (self.health - 15.0).max(0.0);
                self.combo = 0;
                self.events.push(Event::PlayerHit { health: self.health });
                if self.health <= 0.0 {
                    self.over = true;
                    self.events.push(Event::Lost);
                }
            }
        }

        // Remove expired projectiles in reverse order
        expired_or_hit.sort_unstable();
        expired_or_hit.dedup();
        for &pi in expired_or_hit.iter().rev() {
            if pi < self.projectiles.len() {
                self.projectiles.swap_remove(pi);
            }
        }

        // Process enemy deaths, explosions, and chain reactions
        while let Some(dead_id) = dead_enemies.pop() {
            if let Some(pos) = self.enemies.iter().position(|e| e.id == dead_id) {
                let enemy = self.enemies.remove(pos);
                let (blast_radius, blast_dmg, base_pts) = match enemy.kind {
                    EnemyKind::Scout => (3.0, 25.0, 100),
                    EnemyKind::Sentinel => (4.0, 40.0, 250),
                    EnemyKind::Goliath => (6.0, 80.0, 600),
                };

                self.combo += 1;
                self.combo_timer = 150;
                let multiplier = 1 + self.combo / 2;
                self.score += base_pts * multiplier;
                self.events.push(Event::EnemyExploded { at: enemy.pos, radius: blast_radius });

                // Spawn energy shard
                self.shards.push(Shard { pos: enemy.pos, lifetime: 720 });

                // Chain blast damage to nearby enemies
                for other in &mut self.enemies {
                    let d = (other.pos - enemy.pos).length();
                    if d < blast_radius {
                        other.health -= blast_dmg * (1.0 - d / blast_radius);
                        if other.health <= 0.0 && !dead_enemies.contains(&other.id) {
                            dead_enemies.push(other.id);
                        }
                    }
                }
            }
        }

        // Update Enemies AI
        let mut ram_suicides = Vec::new();
        for enemy in &mut self.enemies {
            let delta = center - enemy.pos;
            let dist = delta.length().max(0.1);
            let dir = delta.norm();

            match enemy.kind {
                EnemyKind::Scout => {
                    // Fast dive toward player
                    let desired = dir * 7.5;
                    enemy.vel = enemy.vel + (desired - enemy.vel) * (6.0 * TICK);
                    enemy.pos = enemy.pos + enemy.vel * TICK;
                    enemy.pos.1 = enemy.pos.1.clamp(0.8, 3.5);

                    // Check ram player
                    if dist < 1.15 {
                        self.health = (self.health - 12.0).max(0.0);
                        self.combo = 0;
                        self.player.apply_impulse(dir * 10.0 + V(0., 3.0, 0.));
                        self.events.push(Event::PlayerHit { health: self.health });
                        ram_suicides.push(enemy.id);
                    }
                }
                EnemyKind::Sentinel => {
                    // Hover at medium range and strafe
                    let target_dist = 9.5;
                    let dist_err = dist - target_dist;
                    let radial = dir * (dist_err * 2.0).clamp(-4.0, 4.0);
                    let tangent = V(-dir.2, 0., dir.0) * 3.5;
                    let desired = radial + tangent;
                    enemy.vel = enemy.vel + (desired - enemy.vel) * (4.0 * TICK);
                    enemy.pos = enemy.pos + enemy.vel * TICK;
                    enemy.pos.1 = enemy.pos.1.clamp(1.8, 4.0);

                    // Shooting
                    enemy.cooldown = enemy.cooldown.saturating_sub(1);
                    if enemy.cooldown == 0 {
                        self.projectiles.push(Projectile {
                            pos: enemy.pos,
                            vel: dir * 18.0,
                            from_player: false,
                            lifetime: 140,
                        });
                        enemy.cooldown = 100;
                    }
                }
                EnemyKind::Goliath => {
                    // Heavy slow advance
                    let desired = dir * 2.6;
                    enemy.vel = enemy.vel + (desired - enemy.vel) * (3.0 * TICK);
                    enemy.pos = enemy.pos + enemy.vel * TICK;
                    enemy.pos.1 = enemy.pos.1.clamp(1.2, 2.5);

                    // Radial burst attack
                    enemy.cooldown = enemy.cooldown.saturating_sub(1);
                    if enemy.cooldown == 0 {
                        for angle_deg in [0, 90, 180, 270] {
                            let rad = (angle_deg as f32).to_radians();
                            let b_dir = V(rad.cos(), 0., rad.sin());
                            self.projectiles.push(Projectile {
                                pos: enemy.pos,
                                vel: b_dir * 14.0,
                                from_player: false,
                                lifetime: 120,
                            });
                        }
                        enemy.cooldown = 130;
                    }
                }
            }

            // Arena boundary clamp
            enemy.pos.0 = enemy.pos.0.clamp(-ARENA_HALF + 0.8, ARENA_HALF - 0.8);
            enemy.pos.2 = enemy.pos.2.clamp(-ARENA_HALF + 0.8, ARENA_HALF - 0.8);
        }

        // Remove scout ram suicides
        for dead_id in ram_suicides {
            if let Some(pos) = self.enemies.iter().position(|e| e.id == dead_id) {
                let enemy = self.enemies.remove(pos);
                self.events.push(Event::EnemyExploded { at: enemy.pos, radius: 2.5 });
            }
        }

        // Shard pickup collection
        let mut collected_shards = Vec::new();
        for (si, shard) in self.shards.iter_mut().enumerate() {
            shard.lifetime = shard.lifetime.saturating_sub(1);
            if shard.lifetime == 0 {
                collected_shards.push(si);
                continue;
            }
            if (center - shard.pos).length() < 1.4 {
                collected_shards.push(si);
                let bonus = 50 * (1 + self.combo / 2);
                self.score += bonus;
                self.health = (self.health + 10.0).min(MAX_HEALTH);
                self.events.push(Event::ShardCollected { at: shard.pos, score: self.score });
            }
        }
        for &si in collected_shards.iter().rev() {
            if si < self.shards.len() {
                self.shards.swap_remove(si);
            }
        }

        // Wave progression
        if self.enemies.is_empty() && !self.over {
            if self.wave_delay > 0 {
                self.wave_delay -= 1;
                if self.wave_delay == 0 {
                    self.spawn_wave(self.wave);
                }
            } else if self.wave >= TOTAL_WAVES {
                self.over = true;
                self.won = true;
                self.events.push(Event::Won);
            } else {
                self.events.push(Event::WaveCleared { wave: self.wave });
                self.wave += 1;
                self.wave_delay = 90; // 1.5 seconds breather
            }
        }

        // Void fall check
        if self.player.position.1 < VOID_Y {
            self.health = 0.0;
            self.over = true;
            self.events.push(Event::Lost);
        }
    }

    pub fn drain_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }
}

impl Simulation for Sim {
    type Input = Input;

    fn step(&mut self, input: &Input) {
        Sim::step(self, input);
    }

    fn state_hash(&self) -> u64 {
        let mut h = StateHasher::new();
        h.u64(self.tick)
            .f32(self.health)
            .u32(self.score)
            .u32(self.combo)
            .u32(self.wave)
            .bool(self.over)
            .bool(self.won);
        let p = self.player.position;
        h.f32(p.0).f32(p.1).f32(p.2).f32(self.player.yaw).f32(self.player.pitch);
        h.u32(self.enemies.len() as u32);
        for e in &self.enemies {
            h.u32(e.id).f32(e.pos.0).f32(e.pos.1).f32(e.pos.2).f32(e.health);
        }
        h.u32(self.projectiles.len() as u32);
        for pr in &self.projectiles {
            h.f32(pr.pos.0).f32(pr.pos.1).f32(pr.pos.2).bool(pr.from_player);
        }
        h.u32(self.shards.len() as u32);
        for s in &self.shards {
            h.f32(s.pos.0).f32(s.pos.1).f32(s.pos.2);
        }
        h.finish()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SimState {
    pub tick: u64,
    pub player: ControllerState,
    pub health: f32,
    pub score: u32,
    pub combo: u32,
    pub combo_timer: u32,
    pub wave: u32,
    pub wave_delay: u32,
    pub over: bool,
    pub won: bool,
    pub dash_cooldown: u32,
    pub fire_cooldown: u32,
    pub pad_cooldown: u32,
    pub enemies: Vec<Enemy>,
    pub projectiles: Vec<Projectile>,
    pub shards: Vec<Shard>,
    pub rng: Rng,
    pub was_grounded: bool,
    pub next_enemy_id: u32,
}

impl Snapshot for Sim {
    const KIND: &'static str = "pulse-nova";
    type State = SimState;

    fn capture(&self) -> SimState {
        SimState {
            tick: self.tick,
            player: self.player.network_state(),
            health: self.health,
            score: self.score,
            combo: self.combo,
            combo_timer: self.combo_timer,
            wave: self.wave,
            wave_delay: self.wave_delay,
            over: self.over,
            won: self.won,
            dash_cooldown: self.dash_cooldown,
            fire_cooldown: self.fire_cooldown,
            pad_cooldown: self.pad_cooldown,
            enemies: self.enemies.clone(),
            projectiles: self.projectiles.clone(),
            shards: self.shards.clone(),
            rng: self.rng.clone(),
            was_grounded: self.was_grounded,
            next_enemy_id: self.next_enemy_id,
        }
    }

    fn restore(&mut self, state: SimState) -> Result<(), String> {
        if !state.health.is_finite() || state.health < 0.0 || state.health > MAX_HEALTH * 2.0 {
            return Err("health out of bounds".into());
        }
        if state.enemies.len() > 100 || state.projectiles.len() > 500 {
            return Err("entity count exceeds safe limit".into());
        }
        self.tick = state.tick;
        self.player.restore_network_state(&state.player);
        self.health = state.health;
        self.score = state.score;
        self.combo = state.combo;
        self.combo_timer = state.combo_timer;
        self.wave = state.wave;
        self.wave_delay = state.wave_delay;
        self.over = state.over;
        self.won = state.won;
        self.dash_cooldown = state.dash_cooldown;
        self.fire_cooldown = state.fire_cooldown;
        self.pad_cooldown = state.pad_cooldown;
        self.enemies = state.enemies;
        self.projectiles = state.projectiles;
        self.shards = state.shards;
        self.rng = state.rng;
        self.was_grounded = state.was_grounded;
        self.next_enemy_id = state.next_enemy_id;
        self.events.clear();
        Ok(())
    }

    fn save_tick(&self) -> u64 {
        self.tick
    }
}
