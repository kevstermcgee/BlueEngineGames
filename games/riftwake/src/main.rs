use std::{
    net::SocketAddr,
    sync::{atomic::AtomicBool, Arc},
    time::{Duration, Instant},
};

use macroquad::prelude::*;
use riftwake::{
    arena_boxes, armory, pickups,
    protocol::{ClientInput, MatchSnapshot, WireMessage, PROTOCOL_VERSION},
    server::Server,
    WeaponDef, WeaponKind, DEFAULT_KEY, DEFAULT_SERVER,
};
use vesper3d::{
    math::V,
    viewer::{
        arena::{ArenaInput, DEFAULT_ARENA_FOV},
        net::{transport::DatagramTransport, UdpTransport},
    },
};

fn window_conf() -> Conf {
    Conf {
        window_title: "Riftwake".into(),
        window_width: 1280,
        window_height: 720,
        high_dpi: true,
        sample_count: 4,
        ..Default::default()
    }
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
                    set_cursor_grab(true);
                    show_mouse(false);
                }
                Some(WireMessage::Reject { reason }) => self.status = reason,
                Some(WireMessage::Snapshot { token, state }) if Some(token) == self.token => {
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
        if let Some(token) = self.token {
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
    let mut client = Client::new(address.parse()?, key)?;
    let mut frame = 0_u32;
    loop {
        if is_key_pressed(KeyCode::Escape) {
            set_cursor_grab(false);
            show_mouse(true);
        }
        if is_mouse_button_pressed(MouseButton::Left) && client.token.is_some() {
            set_cursor_grab(true);
            show_mouse(false);
        }
        if is_key_down(KeyCode::LeftAlt) {
            set_cursor_grab(false);
            show_mouse(true);
        } else if client.token.is_some() {
            let delta = mouse_delta_position();
            client.yaw = (client.yaw - delta.x * 2.35).rem_euclid(std::f32::consts::TAU);
            client.pitch = (client.pitch - delta.y * 2.35).clamp(-1.48, 1.48);
        }
        select_weapon(&mut client.weapon_slot);
        let (_, wheel) = mouse_wheel();
        if wheel != 0.0 {
            client.weapon_slot = if wheel > 0.0 {
                (client.weapon_slot + 5) % 6
            } else {
                (client.weapon_slot + 1) % 6
            };
        }
        let mut movement = ArenaInput {
            forward: axis(KeyCode::W, KeyCode::S),
            right: axis(KeyCode::D, KeyCode::A),
            jump: is_key_down(KeyCode::Space),
        };
        if capture.is_some() {
            movement = ArenaInput::default();
        }
        client.update(
            movement,
            is_mouse_button_pressed(MouseButton::Left),
            is_mouse_button_down(MouseButton::Left),
        )?;
        draw_world(&client, capture.is_some(), frame);
        draw_hud(&client, &armory()[client.weapon_slot]);
        frame += 1;
        if frame == 90 {
            if let Some(path) = &capture {
                get_screen_data().export_png(path);
                return Ok(());
            }
        }
        next_frame().await;
    }
}

fn draw_world(client: &Client, showcase: bool, frame: u32) {
    clear_background(Color::new(0.012, 0.014, 0.035, 1.0));
    let eye = if showcase {
        V(-13.0, 6.8, 15.5)
    } else {
        client.local().map_or(V(-18., 1.56, -12.), |p| p.position)
    };
    let dir = if showcase {
        (V(0.0, 2.8, 0.0) - eye).norm()
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
    draw_grid(
        44,
        1.0,
        Color::new(0.05, 0.12, 0.18, 0.35),
        Color::new(0.03, 0.05, 0.09, 1.0),
    );
    for block in arena_boxes() {
        let color = Color::new(block.color[0], block.color[1], block.color[2], 1.0);
        draw_cube(
            vec3(block.center.0, block.center.1, block.center.2),
            vec3(block.size.0, block.size.1, block.size.2),
            None,
            color,
        );
        if block.emissive {
            draw_cube_wires(
                vec3(block.center.0, block.center.1, block.center.2),
                vec3(
                    block.size.0 + 0.05,
                    block.size.1 + 0.05,
                    block.size.2 + 0.05,
                ),
                Color::new(0.9, 0.9, 1.0, 0.8),
            );
        }
    }
    // Rift pylons give the central platform a strong silhouette and landmark.
    for side in [-1.0_f32, 1.0] {
        let pulse = 0.76 + (frame as f32 * 0.035 + side).sin() * 0.18;
        draw_cylinder(
            vec3(side * 4.2, 6.3, 0.0),
            0.45,
            0.7,
            4.0,
            None,
            Color::new(0.45 * pulse, 0.08, 0.9 * pulse, 1.0),
        );
        draw_sphere(
            vec3(side * 4.2, 8.55, 0.0),
            0.75,
            None,
            Color::new(0.15, 0.75 * pulse, 1.0, 1.0),
        );
    }
    for pickup in pickups() {
        let available = client
            .snapshot
            .as_ref()
            .and_then(|s| s.pickups.iter().find(|p| p.id == pickup.id))
            .is_none_or(|p| p.available);
        if available {
            let bob =
                pickup.position.1 + 0.25 + (frame as f32 * 0.04 + pickup.position.0).sin() * 0.12;
            let color = match pickup.kind {
                vesper3d::viewer::arena::PickupKind::Health => Color::new(0.15, 1.0, 0.55, 1.0),
                _ => Color::new(0.2, 0.72, 1.0, 1.0),
            };
            draw_cube(
                vec3(pickup.position.0, bob, pickup.position.2),
                vec3(0.55, 0.55, 0.55),
                None,
                color,
            );
            draw_cube_wires(
                vec3(pickup.position.0, bob, pickup.position.2),
                vec3(0.8, 0.8, 0.8),
                WHITE,
            );
        }
    }
    if let Some(snapshot) = &client.snapshot {
        for player in &snapshot.players {
            if Some(player.id) != client.player_id && player.health > 0 {
                draw_runner(player.position, player.weapon);
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
    let cx = w * 0.5;
    let cy = h * 0.5;
    let cross = weapon_color(weapon.kind);
    draw_circle(cx, cy, 2.2, cross);
    for (x, y) in [(-14., 0.), (14., 0.), (0., -14.), (0., 14.)] {
        draw_line(cx + x * 0.45, cy + y * 0.45, cx + x, cy + y, 2.0, cross);
    }
    let (health, armor, frags, deaths, speed) = client.local().map_or((0, 0, 0, 0, 0.0), |p| {
        (
            p.health,
            p.armor,
            p.frags,
            p.deaths,
            V(p.velocity.0, 0., p.velocity.2).length(),
        )
    });
    panel(18., h - 102., 300., 78.);
    draw_text(
        &format!("{:03} HEALTH", health),
        34.,
        h - 65.,
        29.,
        if health > 30 { WHITE } else { RED },
    );
    draw_text(&format!("{:03} ARMOR", armor), 34., h - 36., 22., SKYBLUE);
    panel(w - 342., h - 102., 324., 78.);
    draw_text(weapon.name, w - 324., h - 66., 26., cross);
    draw_text(
        &format!("{:04.1} m/s  //  90° FOV", speed),
        w - 324.,
        h - 36.,
        20.,
        LIGHTGRAY,
    );
    panel(w * 0.5 - 180., 14., 360., 58.);
    let leader = client
        .snapshot
        .as_ref()
        .and_then(|s| s.players.iter().max_by_key(|p| p.frags));
    draw_text(
        &format!(
            "FRAGS {frags:02} / {:02}   DEATHS {deaths:02}",
            client.snapshot.as_ref().map_or(20, |s| s.frag_limit)
        ),
        w * 0.5 - 151.,
        50.,
        25.,
        WHITE,
    );
    if let Some(leader) = leader {
        draw_text(
            &format!("LEADER  RUNNER {}  [{}]", leader.id, leader.frags),
            20.,
            56.,
            19.,
            LIGHTGRAY,
        );
    }
    draw_text(&client.status, 20., 29., 18., LIGHTGRAY);
    draw_text(
        "WASD accelerate   HOLD SPACE bunny hop   MOUSE fire/look   1-6 weapons   ALT release",
        20.,
        h - 120.,
        17.,
        LIGHTGRAY,
    );
    draw_viewmodel(weapon);
    if let Some(winner) = client.snapshot.as_ref().and_then(|s| s.winner) {
        draw_rectangle(0., h * 0.38, w, 110., Color::from_rgba(5, 3, 15, 230));
        draw_text(
            &format!("RUNNER {winner} OWNS THE RIFT"),
            w * 0.5 - 255.,
            h * 0.38 + 68.,
            42.,
            Color::new(0.65, 0.3, 1.0, 1.0),
        );
    }
}

fn draw_viewmodel(weapon: &WeaponDef) {
    let w = screen_width();
    let h = screen_height();
    let accent = weapon_color(weapon.kind);
    let (length, height) = match weapon.kind {
        WeaponKind::Scattergun => (350., 54.),
        WeaponKind::Nailstorm => (280., 62.),
        WeaponKind::Grenade => (300., 82.),
        WeaponKind::Rocket => (390., 76.),
        WeaponKind::Arc => (330., 48.),
        WeaponKind::Rail => (420., 52.),
    };
    let x = w * 0.72;
    let y = h * 0.86;
    draw_rectangle(
        x - length * 0.5,
        y - height,
        length,
        height,
        Color::new(0.035, 0.045, 0.075, 1.0),
    );
    draw_rectangle(
        x - length * 0.32,
        y - height - 9.,
        length * 0.62,
        10.,
        accent,
    );
    draw_rectangle(
        x - length * 0.22,
        y,
        42.,
        72.,
        Color::new(0.04, 0.05, 0.08, 1.0),
    );
    if matches!(weapon.kind, WeaponKind::Rocket | WeaponKind::Grenade) {
        draw_circle(x + length * 0.20, y - height * 0.55, height * 0.33, accent);
        draw_circle(x + length * 0.20, y - height * 0.55, height * 0.18, BLACK);
    }
}

fn panel(x: f32, y: f32, w: f32, h: f32) {
    draw_rectangle(x, y, w, h, Color::from_rgba(5, 7, 18, 220));
    draw_rectangle_lines(x, y, w, h, 2., Color::new(0.25, 0.55, 0.9, 0.65));
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
fn axis(positive: KeyCode, negative: KeyCode) -> f32 {
    is_key_down(positive) as u8 as f32 - is_key_down(negative) as u8 as f32
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
