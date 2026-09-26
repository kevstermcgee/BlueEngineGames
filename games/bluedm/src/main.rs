#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
use bluedm::{
    content_path,
    protocol::{ClientInput, MatchSnapshot, WireMessage, PROTOCOL_VERSION},
    server::Server,
    DEFAULT_KEY, DEFAULT_SERVER,
};
use macroquad::prelude::*;
use vesper3d::viewer::game_text::{draw_text, measure_text};
use vesper3d::viewer::game_visuals::{SurfaceRenderer, WeaponPresentation, WeaponStyle};
mod native_input;
use native_input::{is_key_down, is_key_pressed, is_mouse_button_down, is_mouse_button_pressed};
use std::{
    net::SocketAddr,
    sync::{atomic::AtomicBool, Arc},
    time::{Duration, Instant},
};
use vesper3d::viewer::{
    game_client::{static_meshes, window_config, GameShell},
    presentation::PoseStream,
};
use vesper3d::{
    math::V,
    viewer::{
        authoring::MapDocument,
        controller::Movement,
        fps::{armory, AimState, OperativeModel, Team, WeaponClass},
        net::{transport::DatagramTransport, UdpTransport},
    },
};

fn window_conf() -> macroquad::conf::Conf {
    window_config("BlueDM")
}

struct Client {
    transport: UdpTransport,
    server: SocketAddr,
    key: String,
    token: Option<[u8; 16]>,
    player_id: Option<u64>,
    sequence: u64,
    fire_counter: u32,
    reload_counter: u32,
    weapon_slot: usize,
    yaw: f32,
    pitch: f32,
    aim: AimState,
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
            reload_counter: 0,
            weapon_slot: 4,
            yaw: 1.57,
            pitch: 0.0,
            aim: AimState::default(),
            snapshot: None,
            poses: PoseStream::default(),
            status: "Connecting to Cobalt Foundry...".into(),
            last_join: Instant::now() - Duration::from_secs(1),
        })
    }

    fn update_network(
        &mut self,
        movement: Movement,
        fire_pressed: bool,
        fire_held: bool,
        reload: bool,
        ads: bool,
    ) -> vesper3d::Result<()> {
        let datagrams = match self.transport.receive() {
            Ok(datagrams) => datagrams,
            Err(error) => {
                self.status = format!("Waiting for server: {error}");
                Vec::new()
            }
        };
        for datagram in datagrams {
            if datagram.peer != self.server {
                continue;
            }
            match WireMessage::decode(&datagram.data) {
                Some(WireMessage::Welcome { player_id, token }) => {
                    self.player_id = Some(player_id);
                    self.token = Some(token);
                    self.status = format!("Online as operative {player_id}");
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
                    self.snapshot = Some(state);
                }
                _ => {}
            }
        }

        if self.token.is_none() && self.last_join.elapsed() >= Duration::from_millis(500) {
            self.last_join = Instant::now();
            let join = WireMessage::Join {
                version: PROTOCOL_VERSION,
                key: self.key.clone(),
                name: "Blue operative".into(),
            };
            self.transport.send(self.server, &join.encode()?)?;
        }
        if let Some(token) = self.token.filter(|_| self.snapshot.is_some()) {
            self.sequence += 1;
            self.fire_counter = self.fire_counter.wrapping_add(u32::from(fire_pressed));
            self.reload_counter = self.reload_counter.wrapping_add(u32::from(reload));
            let input = ClientInput {
                sequence: self.sequence,
                movement,
                yaw: self.yaw,
                pitch: self.pitch,
                weapon_slot: self.weapon_slot as u8,
                fire_counter: self.fire_counter,
                fire_held,
                reload_counter: self.reload_counter,
                ads,
            };
            let message = WireMessage::Input { token, input };
            let _ = self.transport.send(self.server, &message.encode()?);
        }
        Ok(())
    }
}

#[macroquad::main(window_conf)]
async fn main() -> vesper3d::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let host = args.iter().any(|arg| arg == "--host");
    let address = value_after(&args, "--connect").unwrap_or_else(|| DEFAULT_SERVER.into());
    let key = value_after(&args, "--key").unwrap_or_else(|| DEFAULT_KEY.into());
    let capture_path = value_after(&args, "--capture");
    if host {
        let server_key = key.clone();
        std::thread::spawn(move || {
            let mut server =
                Server::bind(DEFAULT_SERVER, server_key).expect("start local BlueDM server");
            server
                .run(Arc::new(AtomicBool::new(false)), None)
                .expect("run local BlueDM server");
        });
        std::thread::sleep(Duration::from_millis(750));
    }
    let map = MapDocument::load(&content_path())?;
    let room = map.build()?;
    let meshes = static_meshes(&room.world);
    let mut surfaces = SurfaceRenderer::new()?;
    surfaces.add_sign(
        "FOUNDRY / LOADING 02",
        vec3(-15., 3.10, -11.84),
        Vec3::X,
        0.50,
    );
    surfaces.add_sign("DISPATCH / 01", vec3(9., 3.10, 11.84), -Vec3::X, 0.50);
    let mut effects = WeaponPresentation::new().await;
    let mut client = Client::new(address.parse()?, key)?;
    let catalog = armory();

    let mut shell = GameShell::new();
    let mut frame_times = Vec::<f32>::new();
    let mut frame = 0u32;
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
            client.pitch = (client.pitch + delta.y * 2.35).clamp(-1.48, 1.48);
        }
        if playing {
            select_weapon(&mut client.weapon_slot);
            let (_, wheel) = mouse_wheel();
            if wheel != 0.0 {
                client.weapon_slot = if wheel > 0.0 {
                    (client.weapon_slot + catalog.weapons.len() - 1) % catalog.weapons.len()
                } else {
                    (client.weapon_slot + 1) % catalog.weapons.len()
                };
            }
        }
        let (forward, right) = native_input::axes();
        let mut movement = Movement {
            forward,
            right,
            jump: is_key_pressed(KeyCode::Space),
            crouch: is_key_down(KeyCode::LeftControl),
            sprint: is_key_down(KeyCode::LeftShift),
        };
        if !playing {
            movement = Movement::default();
        }
        if capture_path.is_some() && frame < 75 {
            movement.forward = 1.0;
            movement.sprint = true;
        }
        let ads = playing && is_mouse_button_down(MouseButton::Right);
        client
            .aim
            .update(ads, get_frame_time(), &catalog.weapons[client.weapon_slot]);
        client.update_network(
            movement,
            playing && is_mouse_button_pressed(MouseButton::Left),
            playing && is_mouse_button_down(MouseButton::Left),
            playing && is_key_pressed(KeyCode::R),
            ads,
        )?;

        let local = client
            .snapshot
            .as_ref()
            .and_then(|s| s.players.iter().find(|p| Some(p.id) == client.player_id));
        let origin = local.map_or(V(-20., 1.68, 0.), |p| p.position);
        let weapon = &catalog.weapons[client.weapon_slot];
        let can_fire = playing
            && match weapon.fire_mode {
                vesper3d::viewer::fps::FireMode::SemiAutomatic => {
                    is_mouse_button_pressed(MouseButton::Left)
                }
                _ => {
                    is_mouse_button_down(MouseButton::Left)
                        || is_mouse_button_pressed(MouseButton::Left)
                }
            }
            && local.is_some_and(|p| p.health > 0 && p.magazine > 0 && !p.reloading);
        effects.trigger(
            can_fire,
            weapon.ticks_between_shots() as f64 / 60.,
            origin,
            vesper3d::viewer::fps::direction(client.yaw, client.pitch),
            &room.world,
        );
        draw_world(
            &meshes,
            &client,
            &room.colliders,
            &surfaces,
            &effects,
            client.aim.fov(76.0, &catalog.weapons[client.weapon_slot]),
            capture_path.is_some(),
        );
        effects.draw(
            match weapon.class {
                WeaponClass::Pistol => WeaponStyle::Pistol,
                WeaponClass::Shotgun => WeaponStyle::Shotgun,
                WeaponClass::SniperRifle | WeaponClass::MarksmanRifle => WeaponStyle::Scoped,
                WeaponClass::MachineGun => WeaponStyle::Heavy,
                _ => WeaponStyle::Rifle,
            },
            client.aim.amount(),
            movement.forward != 0. || movement.right != 0.,
            local.is_some_and(|p| p.reloading),
        );
        draw_hud(&client, &catalog.weapons[client.weapon_slot]);
        if args.iter().any(|a| a == "--capture-menu") {
            shell.paused = true;
        }
        if shell.menu(
            "BLUEDM",
            &[
                "WASD / arrows   Move",
                "Mouse   Look / fire",
                "Space   Jump    Shift   Sprint",
                "1-0 / wheel   Weapons",
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
            if let Some(path) = &capture_path {
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
    meshes: &[Mesh],
    client: &Client,
    colliders: &[vesper3d::viewer::controller::Collider],
    surfaces: &SurfaceRenderer,
    effects: &WeaponPresentation,
    fov: f32,
    showcase: bool,
) {
    clear_background(Color::new(0.65, 0.77, 0.84, 1.0));
    let local = client.snapshot.as_ref().and_then(|snapshot| {
        let id = client.player_id?;
        snapshot.players.iter().find(|player| player.id == id)
    });
    let eye = if showcase {
        V(-12.0, 1.68, 8.0)
    } else {
        local.map_or(V(-20.0, 1.68, 0.0), |player| {
            client
                .poses
                .collision_safe_position(player.id, get_time(), colliders, 1.68, 1.8, 0.23)
                .unwrap_or(player.position)
        })
    };
    let direction = if showcase {
        vesper3d::viewer::fps::direction(1.0, -0.02)
    } else {
        vesper3d::viewer::fps::direction(client.yaw, client.pitch)
    };
    set_camera(&Camera3D {
        position: vec3(eye.0, eye.1, eye.2),
        target: vec3(
            eye.0 + direction.0,
            eye.1 + direction.1,
            eye.2 + direction.2,
        ),
        up: Vec3::Y,
        fovy: fov.to_radians(),
        ..Default::default()
    });
    surfaces.draw(meshes);
    effects.world_effects();
    if let Some(snapshot) = &client.snapshot {
        for player in &snapshot.players {
            if Some(player.id) != client.player_id && player.health > 0 {
                draw_operative(
                    client
                        .poses
                        .position(player.id, get_time(), false)
                        .unwrap_or(player.position),
                    player.team,
                    player.model,
                );
            }
        }
    }
    set_default_camera();
}

fn draw_operative(position: V, team: Team, model: OperativeModel) {
    let color = match team {
        Team::Azure => Color::new(0.12, 0.48, 0.95, 1.0),
        Team::Crimson => Color::new(0.92, 0.16, 0.23, 1.0),
    };
    let accent = match model {
        OperativeModel::Vanguard => GOLD,
        OperativeModel::Recon => LIME,
        OperativeModel::Breacher => ORANGE,
        OperativeModel::FieldTech => SKYBLUE,
    };
    let feet = position.1 - 1.68;
    let width = if model == OperativeModel::Breacher {
        0.62
    } else {
        0.50
    };
    draw_cube(
        vec3(position.0, feet + 1.02, position.2),
        vec3(width, 0.82, 0.36),
        None,
        color,
    );
    draw_sphere(
        vec3(position.0, feet + 1.61, position.2),
        0.22,
        None,
        accent,
    );
    for side in [-1.0, 1.0] {
        draw_cube(
            vec3(position.0 + side * 0.15, feet + 0.42, position.2),
            vec3(0.18, 0.78, 0.22),
            None,
            color,
        );
        if model == OperativeModel::Vanguard || model == OperativeModel::Breacher {
            draw_cube(
                vec3(position.0 + side * 0.31, feet + 1.14, position.2),
                vec3(0.18, 0.22, 0.42),
                None,
                accent,
            );
        }
    }
    if model == OperativeModel::Recon {
        draw_cube(
            vec3(position.0, feet + 1.70, position.2),
            vec3(0.54, 0.06, 0.36),
            None,
            accent,
        );
    }
    if model == OperativeModel::FieldTech {
        draw_cube(
            vec3(position.0, feet + 1.03, position.2 + 0.24),
            vec3(0.36, 0.52, 0.18),
            None,
            accent,
        );
    }
}

fn draw_hud(client: &Client, weapon: &vesper3d::viewer::fps::WeaponDefinition) {
    let w = screen_width();
    let h = screen_height();
    let local = client
        .snapshot
        .as_ref()
        .and_then(|s| s.players.iter().find(|p| Some(p.id) == client.player_id));
    let health = local.map_or(0, |p| p.health);
    let ammo = local.map_or(weapon.magazine_size, |p| p.magazine);
    let reserve = local.map_or(weapon.reserve_ammo, |p| p.reserve);

    draw_circle(w * 0.5, h * 0.5, 3.5, BLACK);
    draw_circle(w * 0.5, h * 0.5, 1.8, WHITE);
    draw_rectangle(20., h - 74., 150., 54., Color::from_rgba(12, 24, 29, 155));
    draw_text(
        &format!("{health}"),
        34.,
        h - 37.,
        32.,
        if health > 30 { WHITE } else { ORANGE },
    );
    draw_text("HEALTH", 94., h - 39., 15., WHITE);
    let x = (w - 220.).max(180.);
    draw_rectangle(x, h - 84., 200., 64., Color::from_rgba(12, 24, 29, 155));
    draw_text(&weapon.name, x + 14., h - 61., 16., WHITE);
    let text = if local.is_some_and(|p| p.reloading) {
        "RELOADING".into()
    } else {
        format!("{ammo}  /  {reserve}")
    };
    draw_text(&text, x + 14., h - 34., 27., WHITE);
    if let Some(s) = &client.snapshot {
        let score = format!(
            "AZURE  {} : {}  CRIMSON",
            s.team_scores[0], s.team_scores[1]
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
        if let Some(team) = s.winner {
            draw_text(
                &format!("{team:?} wins"),
                w * 0.5 - 100.,
                h * 0.4,
                32.,
                GOLD,
            );
        }
    } else {
        draw_text(&client.status, 20., 32., 20., WHITE);
    }
    if is_key_down(KeyCode::Tab) {
        draw_rectangle(24., 60., 260., 80., Color::from_rgba(12, 24, 29, 220));
        if let Some(p) = local {
            draw_text(
                &format!("Kills {}   Deaths {}", p.kills, p.deaths),
                40.,
                95.,
                22.,
                WHITE,
            );
        }
        draw_text("Esc menu    F fullscreen", 40., 123., 17., WHITE);
    }
}

fn select_weapon(slot: &mut usize) {
    for (index, key) in [
        KeyCode::Key1,
        KeyCode::Key2,
        KeyCode::Key3,
        KeyCode::Key4,
        KeyCode::Key5,
        KeyCode::Key6,
        KeyCode::Key7,
        KeyCode::Key8,
        KeyCode::Key9,
        KeyCode::Key0,
    ]
    .iter()
    .enumerate()
    {
        if is_key_pressed(*key) {
            *slot = index;
        }
    }
}

fn value_after(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1))
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
