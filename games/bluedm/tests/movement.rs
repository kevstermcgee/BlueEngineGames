use bluedm::{
    protocol::{ClientInput, WireMessage, PROTOCOL_VERSION},
    server::Server,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use vesper3d::viewer::{
    controller::Movement,
    net::{transport::DatagramTransport, UdpTransport},
};

#[test]
fn all_team_spawns_have_clear_routes_into_the_arena() {
    use vesper3d::{
        math::V,
        viewer::{authoring::MapDocument, simulation::HeadlessWorld},
    };
    let map = MapDocument::load(&bluedm::content_path()).unwrap();
    for (x, yaw) in [
        (-20., std::f32::consts::FRAC_PI_2),
        (22., -std::f32::consts::FRAC_PI_2),
    ] {
        for z in [-3.6, 3.6] {
            let mut world = HeadlessWorld::with_static_room(map.build().unwrap());
            assert!(world.join_at(1, V(x, 1.68, z)));
            world
                .player_mut(1)
                .unwrap()
                .set_physics_state(V(x, 1.68, z), 0., true);
            world.input(
                1,
                Movement {
                    forward: 1.,
                    ..Default::default()
                },
                yaw,
                0.,
            );
            for _ in 0..120 {
                world.step();
            }
            let pos = world.player(1).unwrap().position;
            assert!((pos.0 - x).abs() > 5.5, "blocked spawn {x},{z}: {pos:?}");
            assert!(
                (pos.1 - 1.68).abs() < 0.01,
                "spawn route lost floor support"
            );
        }
    }
}

#[test]
fn network_input_moves_player_out_of_spawn() {
    let mut server = Server::bind("127.0.0.1:0", "movement".into()).unwrap();
    let address = server.local_addr().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let done = stop.clone();
    let worker = std::thread::spawn(move || server.run(done, Some(240)).unwrap());
    let mut client = UdpTransport::bind("127.0.0.1:0").unwrap();
    client
        .send(
            address,
            &WireMessage::Join {
                version: PROTOCOL_VERSION,
                key: "movement".into(),
                name: "movement".into(),
            }
            .encode()
            .unwrap(),
        )
        .unwrap();
    let mut credentials = None;
    let mut start = None;
    let mut last = None;
    let mut sequence = 0;
    let mut last_tick = 0;
    let mut fired = false;
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        for packet in client.receive().unwrap() {
            match WireMessage::decode(&packet.data) {
                Some(WireMessage::Welcome { player_id, token }) => {
                    credentials = Some((player_id, token))
                }
                Some(WireMessage::Snapshot { state, .. }) => {
                    last_tick = state.tick;
                    if let Some((id, _)) = credentials {
                        if let Some(player) = state.players.iter().find(|p| p.id == id) {
                            start.get_or_insert(player.position);
                            last = Some(player.position);
                            fired |= player.magazine < 30;
                        }
                    }
                }
                _ => {}
            }
        }
        if let Some((_, token)) = credentials {
            sequence += 1;
            client
                .send(
                    address,
                    &WireMessage::Input {
                        token,
                        input: ClientInput {
                            sequence,
                            movement: Movement {
                                forward: 1.0,
                                ..Default::default()
                            },
                            yaw: std::f32::consts::FRAC_PI_2,
                            pitch: 0.0,
                            weapon_slot: 4,
                            fire_counter: 1,
                            reload_counter: 0,
                            fire_held: true,
                            ads: false,
                        },
                    }
                    .encode()
                    .unwrap(),
                )
                .unwrap();
        }
        std::thread::sleep(Duration::from_millis(8));
    }
    stop.store(true, Ordering::Relaxed);
    worker.join().unwrap();
    assert!(fired, "server never consumed ammunition for held fire");
    let start = start.expect("initial snapshot");
    let last = last.expect("final snapshot");
    println!(
        "3-second movement: tick {last_tick}, x displacement {} m",
        last.0 - start.0
    );
    assert!(
        last.0 - start.0 > 14.0,
        "movement stuck at tick {last_tick}: {start:?} -> {last:?}"
    );
}
