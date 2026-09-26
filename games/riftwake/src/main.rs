#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
use std::{
    net::SocketAddr,
    sync::{atomic::AtomicBool, Arc},
    time::{Duration, Instant},
};

use macroquad::prelude::*;
use vesper3d::viewer::game_text::{draw_text, measure_text};
use vesper3d::viewer::game_visuals::{SurfaceRenderer, WeaponPresentation, WeaponStyle};
mod native_input;
use native_input::{is_key_down, is_key_pressed, is_mouse_button_down, is_mouse_button_pressed};
use riftwake::{
    arena_boxes, armory, pickups,
    protocol::{ClientInput, MatchSnapshot, WireMessage, PROTOCOL_VERSION},
    server::Server,
    WeaponDef, WeaponKind, DEFAULT_KEY, DEFAULT_SERVER,
};
use vesper3d::viewer::{
    game_client::{static_meshes, window_config, GameShell},
    presentation::PoseStream,
};
use vesper3d::{
    math::V,
    viewer::{
        arena::{ArenaInput, DEFAULT_ARENA_FOV},
        net::{transport::DatagramTransport, UdpTransport},
    },
};

fn window_conf() -> macroquad::conf::Conf {
    window_config("Riftwake")
}

struct Client {
    transport: UdpTransport,
    server: SocketAddr,
    key: String,
    token: Option<[u8; 16]>,
    player_id: Option<u64>,
    sequence: u64,
    fire_counter: u32,
    weapon_slot: usize,
    yaw: f32,
    pitch: f32,
    snapshot: Option<MatchSnapshot>,
    poses: PoseStream,
    status: String,
    last_join: Instant,
}
impl Client {
    fn new(server: SocketAddr, key: String) -> vesper3d::Result<Self> {
        Ok(Self {
            transport: UdpTransport::bind("0.0.0.0:0")?,
            server,
            key,
            token: None,
            player_id: None,
            sequence: 0,
            fire_counter: 0,
            weapon_slot: 3,
            yaw: 0.0,
            pitch: 0.0,
            snapshot: None,
            poses: PoseStream::default(),
            status: "TUNING INTO THE RIFT...".into(),
            last_join: Instant::now() - Duration::from_secs(1),
        })
    }
    fn update(
        &mut self,
        movement: ArenaInput,
        fire_pressed: bool,
        fire_held: bool,
    ) -> vesper3d::Result<()> {
        for datagram in self.transport.receive().unwrap_or_default() {
            if datagram.peer != self.server {
                continue;
            }
            match WireMessage::decode(&datagram.data) {
                Some(WireMessage::Welcome { player_id, token }) => {
                    self.player_id = Some(player_id);
                    self.token = Some(token);
                    self.status = format!("RUNNER {player_id} // LINK STABLE");
                }
                Some(WireMessage::Reject { reason }) => self.status = reason,
                Some(WireMessage::Snapshot { token, state }) if Some(token) == self.token => {
                    if self
                        .snapshot
                        .as_ref()
                        .is_some_and(|old| state.tick <= old.tick)
                    {
                        continue;
                    }
                    if self.snapshot.is_none() {
                        if let Some(local) =
                            state.players.iter().find(|p| Some(p.id) == self.player_id)
                        {
                            client_initial_look(
                                &mut self.yaw,
                                &mut self.pitch,
                                local.yaw,
                                local.pitch,
                            );
                        }
                    }
                    self.poses.push(
                        state.tick,
                        get_time(),
                        &state
                            .players
                            .iter()
                            .map(|p| (p.id, p.position))
                            .collect::<Vec<_>>(),
                    );
                    self.snapshot = Some(state)
                }
                _ => {}
            }
        }
        if self.token.is_none() && self.last_join.elapsed() >= Duration::from_millis(500) {
            self.last_join = Instant::now();
            self.transport.send(
                self.server,
                &WireMessage::Join {
                    version: PROTOCOL_VERSION,
                    key: self.key.clone(),
                    name: "Rift runner".into(),
                }
                .encode()?,
            )?;
        }
        if let Some(token) = self.token.filter(|_| self.snapshot.is_some()) {
            self.sequence += 1;
            self.fire_counter = self.fire_counter.wrapping_add(u32::from(fire_pressed));
            let input = ClientInput {
                sequence: self.sequence,
                movement,
                yaw: self.yaw,
                pitch: self.pitch,
                weapon: armory()[self.weapon_slot].kind,
                fire_counter: self.fire_counter,
                fire_held,
            };
            let _ = self
                .transport
                .send(self.server, &WireMessage::Input { token, input }.encode()?);
        }
        Ok(())
    }
    fn local(&self) -> Option<&riftwake::protocol::PlayerSnapshot> {
        let id = self.player_id?;
        self.snapshot.as_ref()?.players.iter().find(|p| p.id == id)
    }
}

#[macroquad::main(window_conf)]
async fn main() -> vesper3d::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let host = args.iter().any(|x| x == "--host");
    let address = value_after(&args, "--connect").unwrap_or_else(|| DEFAULT_SERVER.into());
    let key = value_after(&args, "--key").unwrap_or_else(|| DEFAULT_KEY.into());
    let capture = value_after(&args, "--capture");
    if host {
        let server_key = key.clone();
        std::thread::spawn(move || {
            Server::bind(DEFAULT_SERVER, server_key)
                .expect("start Riftwake server")
                .run(Arc::new(AtomicBool::new(false)), None)
                .expect("run Riftwake server")
        });
        std::thread::sleep(Duration::from_millis(500));
    }
    let mut builder =
        vesper3d::viewer::builder::SceneBuilder::new("Riftwake arena").spawn(V(-18., 0., -12.), 0.);
    for (i, b) in arena_boxes()
        .iter()
        .chain(riftwake::arena_details().iter())
        .enumerate()
    {
        builder = builder.structural_box(
            format!("arena-{i}"),
            b.center,
            b.size * 0.5,
            V(b.color[0], b.color[1], b.color[2]),
        );
    }
    let room = builder.build()?.build()?;
    let meshes = static_meshes(&room.world);
    let walls = riftwake::colliders();
    let mut surfaces = SurfaceRenderer::new()?;
    surfaces.add_sign("TURBINE HALL / 04", vec3(-17., 3.48, -17.32), Vec3::X, 0.50);
    surfaces.add_sign("SERVICE YARD", vec3(18., 3.48, 17.32), -Vec3::X, 0.50);
    let mut effects = WeaponPresentation::new().await;
    let mut client = Client::new(address.parse()?, key)?;
    let mut shell = GameShell::new();
    let mut frame_times = Vec::<f32>::new();
    let mut frame = 0_u32;
    loop {
        if !foreground() && !args.iter().any(|a| a == "--capture") {
            shell.paused = true;
        }
        native_input::poll(foreground());
        shell.begin_frame_with_input(client.token.is_some(), is_key_pressed);
        let playing = shell.playing();
        if playing {
            let delta = mouse_delta_position();
            client.yaw = (client.yaw - delta.x * 2.35).rem_euclid(std::f32::consts::TAU);
            client.pitch = pitch_after_mouse(client.pitch, delta.y);
        }
        if playing {
            select_weapon(&mut client.weapon_slot);
            let (_, wheel) = mouse_wheel();
            if wheel != 0.0 {
                client.weapon_slot = if wheel > 0.0 {
                    (client.weapon_slot + 5) % 6
                } else {
                    (client.weapon_slot + 1) % 6
                };
            }
        }
        let (forward, right) = native_input::axes();
        let mut movement = ArenaInput {
            forward,
            right,
            jump: is_key_down(KeyCode::Space),
        };
        if !playing || capture.is_some() {
            movement = ArenaInput::default();
        }
        client.update(
            movement,
            playing && is_mouse_button_pressed(MouseButton::Left),
            playing && is_mouse_button_down(MouseButton::Left),
        )?;
        let weapon = &armory()[client.weapon_slot];
        let origin = client.local().map_or(V(-18., 1.56, -12.), |p| p.position);
        effects.trigger(
            playing
                && is_mouse_button_down(MouseButton::Left)
                && client.local().is_some_and(|p| p.health > 0),
            weapon.cooldown as f64 / 60.,
            origin,
            direction(client.yaw, client.pitch),
            &room.world,
        );
        draw_world(
            &client,
            &meshes,
            &walls,
            &surfaces,
            &effects,
            capture.is_some(),
            get_time() as f32,
        );
        effects.draw(
            match weapon.kind {
                WeaponKind::Scattergun => WeaponStyle::Shotgun,
                WeaponKind::Rocket | WeaponKind::Grenade => WeaponStyle::Launcher,
                WeaponKind::Rail => WeaponStyle::Scoped,
                WeaponKind::Nailstorm => WeaponStyle::Heavy,
                _ => WeaponStyle::Rifle,
            },
            0.,
            movement.forward != 0. || movement.right != 0.,
            false,
        );
        draw_hud(&client, &armory()[client.weapon_slot]);
        if args.iter().any(|a| a == "--capture-menu") {
            shell.paused = true;
        }
        if shell.menu(
            "RIFTWAKE",
            &[
                "WASD / arrows   Move",
                "Mouse   Look / fire",
                "Space   Jump / keep jumping",
                "1-6 / wheel   Weapons",
                "F / F11   Fullscreen",
                "Esc   Menu    F3   Diagnostics",
            ],
        ) {
            return Ok(());
        }
        if shell.diagnostics {
            draw_text(&format!("{} FPS", get_fps()), 16., 25., 18., WHITE);
        }
        if frame > 15 {
            frame_times.push(get_frame_time() * 1000.);
        }
        frame += 1;
        if frame == 90 {
            if let Some(path) = &capture {
                get_screen_data().export_png(path);
                frame_times.sort_by(f32::total_cmp);
                let n = frame_times.len();
                let report = serde_json::json!({"sample_frames":n,"median_ms":frame_times[n/2],"p95_ms":frame_times[n*95/100],"p99_ms":frame_times[n*99/100],"snapshot_tick":client.snapshot.as_ref().map(|s|s.tick)});
                std::fs::write(format!("{path}.json"), serde_json::to_vec_pretty(&report)?)?;
                return Ok(());
            }
        }
        next_frame().await;
    }
}

fn draw_world(
    client: &Client,
    meshes: &[Mesh],
    colliders: &[vesper3d::viewer::controller::Collider],
    surfaces: &SurfaceRenderer,
    effects: &WeaponPresentation,
    showcase: bool,
    elapsed: f32,
) {
    clear_background(Color::new(0.65, 0.77, 0.84, 1.0));
    let eye = if showcase {
        V(-13.0, 7.5, 14.0)
    } else {
        client.local().map_or(V(-18., 1.56, -12.), |p| {
            client
                .poses
                .collision_safe_position(p.id, get_time(), colliders, 1.56, 1.72, 0.34)
                .unwrap_or(p.position)
        })
    };
    let dir = if showcase {
        (V(0.0, 1.8, 0.0) - eye).norm()
    } else {
        direction(client.yaw, client.pitch)
    };
    set_camera(&Camera3D {
        position: vec3(eye.0, eye.1, eye.2),
        target: vec3(eye.0 + dir.0, eye.1 + dir.1, eye.2 + dir.2),
        up: Vec3::Y,
        fovy: DEFAULT_ARENA_FOV.to_radians(),
        ..Default::default()
    });
    surfaces.draw(meshes);
    effects.world_effects();
    for pickup in pickups() {
        let available = client
            .snapshot
            .as_ref()
            .and_then(|s| s.pickups.iter().find(|p| p.id == pickup.id))
            .is_none_or(|p| p.available);
        if available {
            let bob = pickup.position.1 + 0.25 + (elapsed * 2.4 + pickup.position.0).sin() * 0.12;
            let color = match pickup.kind {
                vesper3d::viewer::arena::PickupKind::Health => Color::new(0.15, 1.0, 0.55, 1.0),
                _ => Color::new(0.2, 0.72, 1.0, 1.0),
            };
            let pos = vec3(pickup.position.0, bob, pickup.position.2);
            if matches!(pickup.kind, vesper3d::viewer::arena::PickupKind::Health) {
                draw_cube(pos, vec3(0.65, 0.65, 0.3), None, color);
                draw_cube(
                    pos + vec3(0., 0., 0.16),
                    vec3(0.42, 0.14, 0.04),
                    None,
                    WHITE,
                );
                draw_cube(
                    pos + vec3(0., 0., 0.16),
                    vec3(0.14, 0.42, 0.04),
                    None,
                    WHITE,
                );
            } else {
                draw_sphere(pos, 0.38, None, color);
                draw_cube(pos, vec3(0.12, 0.82, 0.12), None, WHITE);
            }
        }
    }
    if let Some(snapshot) = &client.snapshot {
        for player in &snapshot.players {
            if Some(player.id) != client.player_id && player.health > 0 {
                draw_runner(
                    client
                        .poses
                        .position(player.id, get_time(), false)
                        .unwrap_or(player.position),
                    player.weapon,
                );
            }
        }
        for projectile in &snapshot.projectiles {
            let color = weapon_color(projectile.weapon);
            draw_sphere(
                vec3(
                    projectile.position.0,
                    projectile.position.1,
                    projectile.position.2,
                ),
                0.24,
                None,
                color,
            );
            draw_sphere_wires(
                vec3(
                    projectile.position.0,
                    projectile.position.1,
                    projectile.position.2,
                ),
                0.38,
                None,
                WHITE,
            );
        }
    }
    set_default_camera();
}

fn draw_runner(position: V, weapon: WeaponKind) {
    let accent = weapon_color(weapon);
    let feet = position.1 - 1.56;
    draw_cube(
        vec3(position.0, feet + 0.95, position.2),
        vec3(0.58, 0.82, 0.40),
        None,
        Color::new(0.14, 0.16, 0.24, 1.0),
    );
    draw_sphere(
        vec3(position.0, feet + 1.53, position.2),
        0.22,
        None,
        accent,
    );
    for side in [-1.0, 1.0] {
        draw_cube(
            vec3(position.0 + side * 0.17, feet + 0.38, position.2),
            vec3(0.18, 0.70, 0.22),
            None,
            accent,
        );
        draw_cube(
            vec3(position.0 + side * 0.36, feet + 1.05, position.2),
            vec3(0.20, 0.24, 0.42),
            None,
            accent,
        );
    }
}

fn draw_hud(client: &Client, weapon: &WeaponDef) {
    let w = screen_width();
    let h = screen_height();
    let local = client.local();
    let (health, armor, frags) = local.map_or((0, 0, 0), |p| (p.health, p.armor, p.frags));

    draw_circle(w * 0.5, h * 0.5, 3.5, BLACK);
    draw_circle(w * 0.5, h * 0.5, 1.8, WHITE);
    draw_rectangle(20., h - 82., 190., 62., Color::from_rgba(12, 24, 29, 155));
    draw_text(
        &format!("{health}"),
        34.,
        h - 48.,
        31.,
        if health > 30 { WHITE } else { ORANGE },
    );
    draw_text("HEALTH", 95., h - 50., 15., WHITE);
    draw_text(&format!("{armor}  ARMOR"), 34., h - 30., 16., SKYBLUE);
    draw_rectangle(
        w - 224.,
        h - 66.,
        204.,
        46.,
        Color::from_rgba(12, 24, 29, 155),
    );
    draw_text(weapon.name, w - 208., h - 37., 22., WHITE);
    let score = format!(
        "{frags} / {} FRAGS",
        client.snapshot.as_ref().map_or(20, |s| s.frag_limit)
    );
    let size = measure_text(&score, None, 20, 1.);
    draw_rectangle(
        w * 0.5 - size.width * 0.5 - 12.,
        12.,
        size.width + 24.,
        30.,
        Color::from_rgba(12, 24, 29, 155),
    );
    draw_text(&score, w * 0.5 - size.width * 0.5, 33., 20., WHITE);
    if client.snapshot.is_none() {
        draw_text(&client.status, 20., 32., 18., WHITE);
    }
    if is_key_down(KeyCode::Tab) {
        draw_rectangle(24., 60., 290., 90., Color::from_rgba(12, 24, 29, 220));
        if let Some(p) = local {
            draw_text(
                &format!(
                    "Deaths {}   Speed {:.1} m/s",
                    p.deaths,
                    V(p.velocity.0, 0., p.velocity.2).length()
                ),
                40.,
                95.,
                20.,
                WHITE,
            );
        }
        draw_text("Esc menu    F fullscreen", 40., 128., 18., WHITE);
    }
    if let Some(winner) = client.snapshot.as_ref().and_then(|s| s.winner) {
        draw_text(
            &format!("Runner {winner} wins"),
            w * 0.5 - 110.,
            h * 0.4,
            32.,
            GOLD,
        );
    }
}

fn weapon_color(kind: WeaponKind) -> Color {
    let c = armory().into_iter().find(|w| w.kind == kind).unwrap().color;
    Color::new(c[0], c[1], c[2], 1.)
}
fn direction(yaw: f32, pitch: f32) -> V {
    V(
        yaw.sin() * pitch.cos(),
        pitch.sin(),
        -yaw.cos() * pitch.cos(),
    )
    .norm()
}
fn pitch_after_mouse(current: f32, vertical_delta: f32) -> f32 {
    (current + vertical_delta * 2.35).clamp(-1.48, 1.48)
}
fn select_weapon(slot: &mut usize) {
    for (i, key) in [
        KeyCode::Key1,
        KeyCode::Key2,
        KeyCode::Key3,
        KeyCode::Key4,
        KeyCode::Key5,
        KeyCode::Key6,
    ]
    .iter()
    .enumerate()
    {
        if is_key_pressed(*key) {
            *slot = i;
        }
    }
}
fn value_after(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|x| x == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

// Own-window focus query belongs to the executable, not the engine library.
fn foreground() -> bool {
    #[cfg(windows)]
    {
        // These Win32 calls read the foreground window PID into a valid stack pointer.
        unsafe {
            let mut pid = 0;
            windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(
                windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow(),
                &mut pid,
            );
            pid == windows_sys::Win32::System::Threading::GetCurrentProcessId()
        }
    }
    #[cfg(not(windows))]
    {
        true
    }
}

fn client_initial_look(yaw: &mut f32, pitch: &mut f32, server_yaw: f32, server_pitch: f32) {
    *yaw = server_yaw;
    *pitch = server_pitch;
}

#[cfg(test)]
mod tests {
    use super::{direction, pitch_after_mouse};

    #[test]
    fn upward_mouse_motion_raises_the_view() {
        let pitch = pitch_after_mouse(0.0, 0.1);
        assert!(pitch > 0.0);
        assert!(direction(0.0, pitch).1 > 0.0);
    }
}
