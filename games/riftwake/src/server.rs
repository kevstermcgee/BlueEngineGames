use std::{
    collections::BTreeMap,
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use vesper3d::{
    math::V,
    viewer::{
        arena::{
            ArenaBody, ArenaCombatant, ArenaMovementConfig, FragMatch, FragMatchConfig, Projectile,
            ARENA_TICK_RATE,
        },
        net::{session::random_token, transport::DatagramTransport, UdpTransport},
    },
};

use crate::{armory, colliders, launch_pads, pickups, protocol::*, spawns, WeaponKind};

struct ConnectedPlayer {
    peer: SocketAddr,
    token: [u8; 16],
    last_seen: Instant,
    input: ClientInput,
    last_fire_counter: u32,
    cooldown: u32,
    body: ArenaBody,
}

struct ActiveProjectile {
    projectile: Projectile,
    weapon: WeaponKind,
}

pub struct Server {
    transport: UdpTransport,
    key: String,
    players: BTreeMap<u64, ConnectedPlayer>,
    game: FragMatch,
    projectiles: Vec<ActiveProjectile>,
    pickups: Vec<vesper3d::viewer::arena::TimedPickup>,
    spawn_cursor: usize,
    next_player_id: u64,
}

impl Server {
    pub fn bind(address: &str, key: String) -> vesper3d::Result<Self> {
        Ok(Self {
            transport: UdpTransport::bind(address)?,
            key,
            players: BTreeMap::new(),
            game: FragMatch::new(FragMatchConfig::default())?,
            projectiles: Vec::new(),
            pickups: pickups(),
            spawn_cursor: 0,
            next_player_id: 1,
        })
    }
    pub fn local_addr(&self) -> vesper3d::Result<SocketAddr> {
        self.transport.local_addr()
    }
    pub fn run(&mut self, stop: Arc<AtomicBool>, max_ticks: Option<u64>) -> vesper3d::Result<()> {
        println!("Riftwake server listening on {}", self.local_addr()?);
        let tick = Duration::from_secs_f64(1.0 / ARENA_TICK_RATE as f64);
        let mut next = Instant::now();
        while !stop.load(Ordering::Relaxed) {
            next += tick;
            self.receive()?;
            self.step();
            if self.game.tick.is_multiple_of(3) {
                self.broadcast();
            }
            if max_ticks.is_some_and(|limit| self.game.tick >= limit) {
                break;
            }
            let now = Instant::now();
            if next > now {
                std::thread::sleep(next - now);
            } else if now.duration_since(next) > tick * 8 {
                next = now;
            }
        }
        Ok(())
    }

    fn receive(&mut self) -> vesper3d::Result<()> {
        let now = Instant::now();
        for datagram in self.transport.receive()? {
            let Some(message) = WireMessage::decode(&datagram.data) else {
                continue;
            };
            match message {
                WireMessage::Join {
                    version,
                    key,
                    name: _,
                } => {
                    if version != PROTOCOL_VERSION || key != self.key {
                        let reject = WireMessage::Reject {
                            reason: "Protocol mismatch or incorrect join key".into(),
                        };
                        let _ = self.transport.send(datagram.peer, &reject.encode()?);
                        continue;
                    }
                    if let Some((&id, player)) =
                        self.players.iter().find(|(_, p)| p.peer == datagram.peer)
                    {
                        let welcome = WireMessage::Welcome {
                            player_id: id,
                            token: player.token,
                        };
                        let _ = self.transport.send(datagram.peer, &welcome.encode()?);
                        continue;
                    }
                    if self.players.len() >= 12 {
                        let reject = WireMessage::Reject {
                            reason: "Arena is full".into(),
                        };
                        let _ = self.transport.send(datagram.peer, &reject.encode()?);
                        continue;
                    }
                    let id = self.next_player_id;
                    self.next_player_id += 1;
                    let pair = random_token()?;
                    let mut token = [0; 16];
                    token[..8].copy_from_slice(&pair[0].to_le_bytes());
                    token[8..].copy_from_slice(&pair[1].to_le_bytes());
                    let (feet, yaw) = self.next_spawn();
                    self.game.join(id);
                    self.players.insert(
                        id,
                        ConnectedPlayer {
                            peer: datagram.peer,
                            token,
                            last_seen: now,
                            input: ClientInput {
                                sequence: 0,
                                movement: Default::default(),
                                yaw,
                                pitch: 0.0,
                                weapon: WeaponKind::Rocket,
                                fire_counter: 0,
                                fire_held: false,
                            },
                            last_fire_counter: 0,
                            cooldown: 0,
                            body: ArenaBody::spawn(feet, yaw, ArenaMovementConfig::default())?,
                        },
                    );
                    self.transport.send(
                        datagram.peer,
                        &WireMessage::Welcome {
                            player_id: id,
                            token,
                        }
                        .encode()?,
                    )?;
                    println!("runner {id} entered from {}", datagram.peer);
                }
                WireMessage::Input { token, input } if input.valid() => {
                    if let Some(player) = self
                        .players
                        .values_mut()
                        .find(|p| p.token == token && p.peer == datagram.peer)
                    {
                        if input.sequence > player.input.sequence {
                            player.last_seen = now;
                            player.input = input;
                        }
                    }
                }
                _ => {}
            }
        }
        let expired: Vec<_> = self
            .players
            .iter()
            .filter_map(|(&id, p)| {
                (now.duration_since(p.last_seen) > Duration::from_secs(10)).then_some(id)
            })
            .collect();
        for id in expired {
            self.players.remove(&id);
            self.game.players.remove(&id);
        }
        Ok(())
    }

    fn step(&mut self) {
        let walls = colliders();
        let mut shots = Vec::new();
        for (&id, player) in &mut self.players {
            let alive = self.game.players.get(&id).is_some_and(|p| p.health > 0);
            if !alive {
                continue;
            }
            player.body.yaw = player.input.yaw;
            player.body.pitch = player.input.pitch;
            player
                .body
                .step(player.input.movement, 1.0 / ARENA_TICK_RATE as f32, &walls);
            for pad in launch_pads() {
                pad.activate(&mut player.body);
            }
            player.cooldown = player.cooldown.saturating_sub(1);
            let def = armory()
                .into_iter()
                .find(|w| w.kind == player.input.weapon)
                .unwrap();
            let pressed = player.input.fire_counter != player.last_fire_counter;
            player.last_fire_counter = player.input.fire_counter;
            let automatic = matches!(def.kind, WeaponKind::Nailstorm | WeaponKind::Arc);
            if player.cooldown == 0 && (pressed || automatic && player.input.fire_held) {
                player.cooldown = def.cooldown;
                shots.push((
                    id,
                    player.body.position,
                    direction(player.body.yaw, player.body.pitch),
                    def,
                ));
            }
        }
        for (owner, origin, direction, def) in shots {
            if let Some(spec) = def.projectile {
                if let Ok(projectile) =
                    Projectile::spawn(owner, origin + direction * 0.6, direction, spec)
                {
                    self.projectiles.push(ActiveProjectile {
                        projectile,
                        weapon: def.kind,
                    });
                }
            } else {
                let pellets = if def.kind == WeaponKind::Scattergun {
                    10
                } else {
                    1
                };
                for pellet in 0..pellets {
                    let spread = if pellets > 1 {
                        (pellet as f32 - 4.5) * 0.008
                    } else {
                        0.0
                    };
                    let ray = V(
                        direction.0 + spread,
                        direction.1,
                        direction.2 - spread * 0.5,
                    )
                    .norm();
                    if let Some(victim) = self.raycast_player(owner, origin, ray, &walls) {
                        self.game.damage(owner, victim, def.damage);
                    }
                }
            }
        }
        self.step_projectiles(&walls);
        for pickup in &mut self.pickups {
            pickup.step();
            if !pickup.available() {
                continue;
            }
            for (&id, player) in &self.players {
                if let Some(amount) = pickup.collect(player.body.position) {
                    if let Some(combatant) = self.game.players.get_mut(&id) {
                        match pickup.kind {
                            vesper3d::viewer::arena::PickupKind::Health => {
                                combatant.health = combatant
                                    .health
                                    .saturating_add(amount)
                                    .min(if pickup.id == "mega" { 200 } else { 100 })
                            }
                            vesper3d::viewer::arena::PickupKind::Armor => {
                                combatant.armor = combatant.armor.saturating_add(amount).min(200)
                            }
                            _ => {}
                        }
                    }
                    break;
                }
            }
        }
        for id in self.game.step() {
            let (feet, yaw) = self.next_spawn();
            if let Some(player) = self.players.get_mut(&id) {
                player.body = ArenaBody::spawn(feet, yaw, ArenaMovementConfig::default()).unwrap();
            }
        }
        let fallen: Vec<_> = self
            .players
            .iter()
            .filter_map(|(&id, p)| (p.body.feet().1 < -8.0).then_some(id))
            .collect();
        for id in fallen {
            self.game.damage(id, id, 999.0);
        }
    }

    fn step_projectiles(&mut self, walls: &[vesper3d::viewer::controller::Collider]) {
        let mut explosions = Vec::new();
        for active in &mut self.projectiles {
            let alive = active.projectile.step(1.0 / ARENA_TICK_RATE as f32);
            let wall = walls.iter().any(|c| c.contains(active.projectile.position));
            let direct = self.players.iter().find_map(|(&id, p)| {
                (id != active.projectile.owner
                    && (p.body.position - active.projectile.position).length() < 0.7
                    && self.game.players.get(&id).is_some_and(|x| x.health > 0))
                .then_some(id)
            });
            if !alive || wall || direct.is_some() {
                explosions.push((
                    active.projectile.owner,
                    active.projectile.position,
                    active.projectile.spec,
                    direct,
                ));
                active.projectile.remaining_ticks = 0;
            }
        }
        self.projectiles
            .retain(|p| p.projectile.remaining_ticks > 0);
        for (owner, position, spec, direct) in explosions {
            if let Some(victim) = direct {
                self.game.damage(owner, victim, spec.direct_damage);
            }
            let victims: Vec<_> = self
                .players
                .iter()
                .filter_map(|(&id, p)| {
                    let damage = spec.splash_at((p.body.position - position).length());
                    (damage > 0.0 && Some(id) != direct).then_some((id, damage))
                })
                .collect();
            for (id, damage) in victims {
                self.game.damage(owner, id, damage);
            }
        }
    }

    fn raycast_player(
        &self,
        owner: u64,
        origin: V,
        ray: V,
        walls: &[vesper3d::viewer::controller::Collider],
    ) -> Option<u64> {
        let wall_distance = walls
            .iter()
            .filter_map(|c| ray_box(origin, ray, c.min, c.max))
            .fold(120.0_f32, f32::min);
        self.players
            .iter()
            .filter_map(|(&id, p)| {
                if id == owner || !self.game.players.get(&id).is_some_and(|x| x.health > 0) {
                    return None;
                }
                let along = (p.body.position - origin).dot(ray);
                let offset = p.body.position - (origin + ray * along);
                (along > 0.0 && along < wall_distance && offset.length() < 0.55)
                    .then_some((id, along))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|x| x.0)
    }

    fn next_spawn(&mut self) -> (V, f32) {
        let points = spawns();
        let point = points[self.spawn_cursor % points.len()];
        self.spawn_cursor += 1;
        point
    }

    fn broadcast(&self) {
        let state = MatchSnapshot {
            tick: self.game.tick,
            frag_limit: self.game.config.frag_limit,
            winner: self.game.winner(),
            players: self
                .players
                .iter()
                .filter_map(|(&id, p)| {
                    let combat = self.game.players.get(&id)?;
                    Some(snapshot_player(id, p, combat))
                })
                .collect(),
            projectiles: self
                .projectiles
                .iter()
                .map(|p| ProjectileSnapshot {
                    position: p.projectile.position,
                    weapon: p.weapon,
                })
                .collect(),
            pickups: self
                .pickups
                .iter()
                .map(|p| PickupSnapshot {
                    id: p.id.clone(),
                    available: p.available(),
                })
                .collect(),
        };
        for player in self.players.values() {
            if let Ok(bytes) = (WireMessage::Snapshot {
                token: player.token,
                state: state.clone(),
            })
            .encode()
            {
                let _ = self.transport.send(player.peer, &bytes);
            }
        }
    }
}

fn snapshot_player(id: u64, player: &ConnectedPlayer, combat: &ArenaCombatant) -> PlayerSnapshot {
    PlayerSnapshot {
        id,
        position: player.body.position,
        velocity: player.body.velocity,
        yaw: player.body.yaw,
        pitch: player.body.pitch,
        health: combat.health,
        armor: combat.armor,
        frags: combat.frags,
        deaths: combat.deaths,
        weapon: player.input.weapon,
    }
}
fn direction(yaw: f32, pitch: f32) -> V {
    V(
        yaw.sin() * pitch.cos(),
        pitch.sin(),
        -yaw.cos() * pitch.cos(),
    )
    .norm()
}
fn ray_box(origin: V, direction: V, min: V, max: V) -> Option<f32> {
    let mut near = 0.0_f32;
    let mut far = 120.0_f32;
    for (o, d, lo, hi) in [
        (origin.0, direction.0, min.0, max.0),
        (origin.1, direction.1, min.1, max.1),
        (origin.2, direction.2, min.2, max.2),
    ] {
        if d.abs() < 0.00001 {
            if o < lo || o > hi {
                return None;
            }
        } else {
            let a = (lo - o) / d;
            let b = (hi - o) / d;
            near = near.max(a.min(b));
            far = far.min(a.max(b));
            if near > far {
                return None;
            }
        }
    }
    (far >= 0.0).then_some(near.max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn server_constructs_and_arena_has_vertical_routes() {
        let server = Server::bind("127.0.0.1:0", "test".into()).unwrap();
        assert_eq!(server.game.players.len(), 0);
        assert!(colliders().len() >= 10);
        assert_eq!(launch_pads().len(), 2);
    }
    #[test]
    fn ray_box_obstructs_hitscan() {
        assert_eq!(
            ray_box(
                V(0., 1., 4.),
                V(0., 0., -1.),
                V(-1., 0., -1.),
                V(1., 2., 1.)
            ),
            Some(3.0)
        );
    }
}
