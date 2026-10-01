//! Deadfall: the game window.
//!
//!   deadfall                          the menu
//!   deadfall --solo                   start a solo match with bots at once
//!   deadfall --connect HOST:PORT      join a server ([--name N] [--key K])
//!   deadfall --capture DIR --frames 60,300 [--solo] [--screen stats|settings|host|join]
//!                                     save screenshots of those frames, then exit (for runs nobody watches)
//!   --size WxH   --mute   --novsync
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use deadfall::client::{app::App, platform};
use macroquad::prelude::*;
use vesper3d::viewer::{
    devkit::{flag_value, has_flag, parse_size},
    game_client,
};

fn window() -> macroquad::conf::Conf {
    platform::attach_console();
    let mut conf = game_client::window_config_with_icon(
        "Deadfall",
        game_client::icon_from_rgba(
            include_bytes!("../assets/icon_16.rgba"),
            include_bytes!("../assets/icon_32.rgba"),
            include_bytes!("../assets/icon_64.rgba"),
        ),
    );
    let args: Vec<String> = std::env::args().collect();
    if let Some((w, h)) = flag_value(&args, "--size").and_then(parse_size) {
        conf.miniquad_conf.window_width = w.clamp(320, 7680) as i32;
        conf.miniquad_conf.window_height = h.clamp(240, 4320) as i32;
    }
    if has_flag(&args, "--novsync") {
        conf.miniquad_conf.platform.swap_interval = Some(0);
    }
    conf
}

#[macroquad::main(window)]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if has_flag(&args, "--help") || has_flag(&args, "-h") {
        println!(
            "{}",
            include_str!("main.rs")
                .lines()
                .take_while(|l| l.starts_with("//!"))
                .map(|l| l.trim_start_matches("//! ").trim_start_matches("//!"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        return;
    }
    let mut app = App::new(&args).await;
    app.run().await;
}
