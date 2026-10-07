//! The on-screen display: deliberately tiny. A dot of crosshair, health and ammunition in the corners, the score
//! and clock at the top, a few words when something needs saying. The scoreboard (hold Tab) and the scope overlay
//! are the only big things.
use super::ui::{self, team_colour, ACCENT, DIM, TEXT};
use crate::netgame::{PlayerView, Snapshot};
use crate::sim::RosterEntry;
use crate::weapons::{self, WeaponDef};
use macroquad::prelude::*;
use vesper3d::math::V;
use vesper3d::viewer::kit::hud;

const HEALTH_OK: Color = Color::new(0.92, 0.93, 0.94, 1.);
const HEALTH_LOW: Color = Color::new(0.95, 0.3, 0.25, 1.);

pub struct Feed {
    pub text: String,
    pub age: f32,
    pub mine: bool,
    pub team: usize,
}

pub fn crosshair(gap: f32, scoped: bool) {
    if scoped {
        return;
    }
    let (cx, cy) = (screen_width() * 0.5, screen_height() * 0.5);
    let ui = hud::ui_scale();
    let g = (3. + gap) * ui;
    let l = 6. * ui;
    let c = Color::new(0.95, 0.97, 1., 0.85);
    let t = 2. * ui;
    draw_rectangle(cx - g - l, cy - t * 0.5, l, t, c);
    draw_rectangle(cx + g, cy - t * 0.5, l, t, c);
    draw_rectangle(cx - t * 0.5, cy - g - l, t, l, c);
    draw_rectangle(cx - t * 0.5, cy + g, t, l, c);
    draw_circle(cx, cy, 1.2 * ui, c);
}

pub fn hit_marker(amount: f32, head: bool) {
    if amount <= 0. {
        return;
    }
    let (cx, cy) = (screen_width() * 0.5, screen_height() * 0.5);
    let ui = hud::ui_scale();
    let c = if head { Color::new(1., 0.35, 0.3, amount) } else { Color::new(1., 1., 1., amount) };
    let (a, b) = (7. * ui, 13. * ui);
    for (sx, sy) in [(-1., -1.), (1., -1.), (-1., 1.), (1., 1.)] {
        draw_line(cx + sx * a, cy + sy * a, cx + sx * b, cy + sy * b, 2. * ui, c);
    }
}

/// A red wedge at the screen edge pointing at who hurt you, fading.
pub fn damage_arc(angle: f32, amount: f32) {
    if amount <= 0. {
        return;
    }
    let (cx, cy) = (screen_width() * 0.5, screen_height() * 0.5);
    let r = cy.min(cx) * 0.78;
    let (s, c) = angle.sin_cos();
    let ui = hud::ui_scale();
    let p = vec2(cx + s * r, cy - c * r);
    let side = vec2(c, s) * 26. * ui;
    let tip = vec2(cx + s * (r + 38. * ui), cy - c * (r + 38. * ui));
    draw_triangle(p - side, p + side, tip, Color::new(0.95, 0.15, 0.1, 0.75 * amount));
}

#[allow(clippy::too_many_arguments)] // Established scalar geometry/gameplay interface.
pub fn health_and_ammo(
    health: f32,
    armor: f32,
    def: Option<&WeaponDef>,
    mag: u16,
    reserve: u16,
    reloading: bool,
    grenades: usize,
    name: &str,
) {
    let ui = hud::ui_scale();
    let (w, h) = (screen_width(), screen_height());
    let low = health <= 30.;
    hud::text_outlined(
        &format!("{}", health.ceil() as i32),
        28. * ui,
        h - 30. * ui,
        54. * ui,
        if low { HEALTH_LOW } else { HEALTH_OK },
    );
    if armor > 0. {
        hud::text_outlined(&format!("+{}", armor.ceil() as i32), 28. * ui + 92. * ui, h - 30. * ui, 28. * ui, DIM);
    }
    if let Some(d) = def {
        let melee = matches!(d.class, weapons::Class::Melee);
        let throwable = matches!(d.class, weapons::Class::Grenade);
        if !melee && !throwable {
            let mag_text = if reloading { "--".to_string() } else { format!("{mag}") };
            hud::text_right(
                &mag_text,
                w - 120. * ui,
                h - 30. * ui,
                54. * ui,
                if mag == 0 && reserve == 0 { HEALTH_LOW } else { HEALTH_OK },
            );
            hud::text_right(&format!("{reserve}"), w - 28. * ui, h - 30. * ui, 28. * ui, DIM);
        }
        hud::text_right(name, w - 28. * ui, h - 78. * ui, 20. * ui, DIM);
    }
    for i in 0..grenades.min(2) {
        draw_circle(w - 40. * ui - i as f32 * 22. * ui, h - 104. * ui, 7. * ui, Color::new(0.9, 0.9, 0.9, 0.85));
    }
}

/// Team scores and the clock (or the kill target), top centre.
pub fn score_strip(snap: &Snapshot, my_team: usize) {
    let ui = hud::ui_scale();
    let cx = screen_width() * 0.5;
    let y = 34. * ui;
    if snap.mode == crate::modes::GameMode::FreeForAll {
        let leader = snap.players.iter().max_by_key(|p| p.kills).map_or(0, |p| p.kills);
        hud::text_centered(&format!("FFA  /  LEAD {leader}  /  FIRST TO {}", snap.kill_target), cx, y, 24. * ui, TEXT);
        return;
    }
    let (a, b) = (snap.scores[0], snap.scores[1]);
    hud::text_right(&format!("{a}"), cx - 34. * ui, y, 34. * ui, team_colour(0));
    hud::text_outlined(&format!("{b}"), cx + 34. * ui, y, 34. * ui, team_colour(1));
    let mid = match snap.time_left {
        Some(t) => ui::clock(t),
        None => {
            if snap.mode == crate::modes::GameMode::TeamDeathmatch {
                format!("{}", snap.kill_target)
            } else {
                format!("{}", snap.objective_target)
            }
        }
    };
    hud::text_centered(&mid, cx, y - 2. * ui, 22. * ui, DIM);
    if snap.mode == crate::modes::GameMode::CaptureFlag {
        let own = snap.objective.flags[my_team.min(1)];
        let enemy = snap.objective.flags[1 - my_team.min(1)];
        let words = if enemy.carrier != 255 {
            "ENEMY FLAG TAKEN — RETURN TO YOUR BASE"
        } else if own.carrier != 255 {
            "YOUR FLAG HAS BEEN TAKEN"
        } else if own.dropped_at != 0 {
            "RETURN YOUR DROPPED FLAG"
        } else {
            "TAKE THE ENEMY FLAG / DEFEND YOUR BASE"
        };
        hud::text_centered(words, cx, 60. * ui, 17. * ui, DIM);
    } else if snap.mode == crate::modes::GameMode::SearchDestroy {
        let o = snap.objective;
        let attack = (o.round as usize - 1) % 2;
        let role = if my_team == attack { "ATTACK" } else { "DEFEND" };
        let phase = match o.phase {
            0 => "GET READY",
            1 => "PLANT AT A OR B",
            2 => "BOMB PLANTED",
            _ => "ROUND COMPLETE",
        };
        let left = o.deadline.saturating_sub(snap.tick) as f32 / 60.;
        hud::text_centered(
            &format!("ROUND {} / {role} / {phase} / {}", o.round, ui::clock(left)),
            cx,
            60. * ui,
            17. * ui,
            DIM,
        );
        if let Some(me) = &snap.me {
            if me.alive {
                let feet = V(me.ctrl.position.0, me.ctrl.position.1 - 1.68, me.ctrl.position.2);
                let can = if o.phase == 1 {
                    my_team == attack
                        && snap
                            .players
                            .iter()
                            .find(|p| p.slot == o.carrier)
                            .is_some_and(|p| (p.eye - me.ctrl.position).length() < 0.1)
                        && snap.map.sites().iter().any(|p| (*p - feet).length() < 2.)
                } else {
                    o.phase == 2 && my_team != attack && (o.bomb - feet).length() < 2.
                };
                if can {
                    let action = if o.phase == 1 { "PLANT" } else { "DEFUSE" };
                    let goal = if o.phase == 1 { 180. } else { 300. };
                    prompt(&format!("HOLD E / RB TO {action}  {}%", (o.progress as f32 / goal * 100.) as u32));
                }
            }
        }
    }
}

pub fn killfeed(feed: &[Feed]) {
    let ui = hud::ui_scale();
    let mut y = 70. * ui;
    for f in feed.iter().rev().take(4) {
        let a = (1. - (f.age - 5.) / 2.).clamp(0., 1.);
        let mut c = if f.mine { ACCENT } else { team_colour(f.team) };
        c.a = a;
        hud::text_right(&f.text, screen_width() - 24. * ui, y, 20. * ui, c);
        y += 24. * ui;
    }
}

pub fn prompt(text: &str) {
    let ui = hud::ui_scale();
    hud::text_centered(text, screen_width() * 0.5, screen_height() * 0.62, 22. * ui, TEXT);
}

pub fn notice(text: &str, alpha: f32) {
    let ui = hud::ui_scale();
    let mut c = TEXT;
    c.a = alpha.clamp(0., 1.);
    hud::text_centered(text, screen_width() * 0.5, screen_height() * 0.3, 30. * ui, c);
}

/// White-out from a flashbang.
pub fn flash(amount: f32) {
    if amount > 0. {
        draw_rectangle(0., 0., screen_width(), screen_height(), Color::new(1., 1., 1., amount.clamp(0., 1.)));
    }
}

/// A dark lens with a cross-hair and a dot, for looking through a scope.
pub fn scope(zoom: f32, amount: f32) {
    if amount <= 0.01 {
        return;
    }
    let (w, h) = (screen_width(), screen_height());
    let (cx, cy) = (w * 0.5, h * 0.5);
    let r = h * 0.46;
    let black = Color::new(0., 0., 0., amount);
    // Everything outside the lens circle: a ring of triangles from the circle out past the screen corners.
    let big = (w * w + h * h).sqrt();
    let n = 96;
    for i in 0..n {
        let (a0, a1) = (i as f32 / n as f32 * std::f32::consts::TAU, (i + 1) as f32 / n as f32 * std::f32::consts::TAU);
        let p = |a: f32, rad: f32| vec2(cx + a.cos() * rad, cy + a.sin() * rad);
        draw_triangle(p(a0, r), p(a1, r), p(a0, big), black);
        draw_triangle(p(a1, r), p(a1, big), p(a0, big), black);
    }
    draw_circle_lines(cx, cy, r, 3., Color::new(0., 0., 0., amount));
    let line = Color::new(0., 0., 0., 0.85 * amount);
    draw_line(cx - r, cy, cx + r, cy, 1.5, line);
    draw_line(cx, cy - r, cx, cy + r, 1.5, line);
    draw_circle(cx, cy, 2., Color::new(0.9, 0.1, 0.1, amount));
    hud::text_centered(
        &format!("{zoom:.0}x"),
        cx + r * 0.75,
        cy + r * 0.85,
        20. * hud::ui_scale(),
        Color::new(1., 1., 1., 0.6 * amount),
    );
}

/// The killcam frame: bars, who killed you with what, and the countdown.
pub fn killcam(killer: &str, weapon: &str, seconds_left: f32, headshot: bool, progress: f32, round_elimination: bool) {
    let ui = hud::ui_scale();
    let (w, h) = (screen_width(), screen_height());
    let bar = (h * 0.075).max(48. * ui);
    draw_rectangle(0., 0., w, bar, Color::new(0., 0., 0., 0.85));
    draw_rectangle(0., h - bar, w, bar, Color::new(0., 0., 0., 0.85));
    let how = if headshot { format!("{weapon}  (headshot)") } else { weapon.to_string() };
    let label =
        if killer == "the fall" { "ELIMINATED".to_string() } else { format!("KILLED BY {}", killer.to_uppercase()) };
    hud::text_centered(&label, w * 0.5, bar * 0.66, 24. * ui, TEXT);
    hud::text_outlined(
        if round_elimination && seconds_left <= 0. {
            "SPECTATING"
        } else if progress < 1. {
            "REPLAY"
        } else {
            "REPLAY COMPLETE"
        },
        24. * ui,
        bar * 0.66,
        15. * ui,
        ACCENT,
    );
    draw_rectangle(0., bar - 2. * ui, w * progress.clamp(0., 1.), 2. * ui, ACCENT);
    hud::text_centered(&how, w * 0.5, h - bar * 0.34, 22. * ui, DIM);
    hud::text_right(
        &if round_elimination {
            "NEXT ROUND".to_string()
        } else {
            format!("RESPAWN IN {}", seconds_left.ceil() as i32)
        },
        w - 24. * ui,
        h - bar * 0.34,
        18. * ui,
        ACCENT,
    );
}

pub struct Row {
    pub slot: u8,
    pub name: String,
    pub team: usize,
    pub kills: u16,
    pub deaths: u16,
    pub bot: bool,
    pub alive: bool,
}

pub fn rows(roster: &[RosterEntry], players: &[PlayerView]) -> Vec<Row> {
    roster
        .iter()
        .map(|r| {
            let p = players.iter().find(|p| p.slot == r.slot);
            Row {
                slot: r.slot,
                name: r.name.clone(),
                team: r.team as usize,
                kills: p.map_or(0, |p| p.kills),
                deaths: p.map_or(0, |p| p.deaths),
                bot: r.bot,
                alive: p.is_some_and(|p| p.has(crate::netgame::flag::ALIVE)),
            }
        })
        .collect()
}

/// Both teams side by side. `me` is highlighted.
pub fn scoreboard(rows: &[Row], scores: [u16; 2], me: Option<u8>, x: f32, y: f32, w: f32) {
    let ui = hud::ui_scale();
    let colw = (w - 24. * ui) / 2.;
    for (team, score) in scores.iter().enumerate() {
        let px = x + team as f32 * (colw + 24. * ui);
        let c = team_colour(team);
        draw_rectangle(px, y, colw, 40. * ui, Color::new(c.r * 0.35, c.g * 0.35, c.b * 0.35, 0.92));
        hud::text_outlined(crate::Team::from_index(team).name(), px + 14. * ui, y + 29. * ui, 26. * ui, c);
        hud::text_right(&format!("{score}"), px + colw - 14. * ui, y + 29. * ui, 30. * ui, TEXT);
        let mut mine: Vec<&Row> = rows.iter().filter(|r| r.team == team).collect();
        mine.sort_by(|a, b| b.kills.cmp(&a.kills).then(a.deaths.cmp(&b.deaths)));
        draw_rectangle(px, y + 40. * ui, colw, 34. * ui + mine.len() as f32 * 32. * ui, PANEL_DARK);
        hud::text_outlined("PLAYER", px + 14. * ui, y + 64. * ui, 17. * ui, DIM);
        hud::text_right("K", px + colw - 70. * ui, y + 64. * ui, 17. * ui, DIM);
        hud::text_right("D", px + colw - 16. * ui, y + 64. * ui, 17. * ui, DIM);
        for (i, r) in mine.iter().enumerate() {
            let ry = y + 74. * ui + i as f32 * 32. * ui;
            if Some(r.slot) == me {
                draw_rectangle(px, ry, colw, 30. * ui, Color::new(1., 1., 1., 0.1));
            }
            let name = if r.bot { format!("{} [bot]", r.name) } else { r.name.clone() };
            hud::text_outlined(&name, px + 14. * ui, ry + 22. * ui, 21. * ui, if r.alive { TEXT } else { DIM });
            hud::text_right(&format!("{}", r.kills), px + colw - 70. * ui, ry + 22. * ui, 21. * ui, TEXT);
            hud::text_right(&format!("{}", r.deaths), px + colw - 16. * ui, ry + 22. * ui, 21. * ui, TEXT);
        }
    }
}

const PANEL_DARK: Color = Color::new(0.03, 0.04, 0.05, 0.85);

/// Individual standings for free-for-all; team cosmetics never imply a friendly player.
pub fn ffa_scoreboard(rows: &[Row], me: Option<u8>, x: f32, y: f32, w: f32) {
    let ui = hud::ui_scale();
    let mut ranked = rows.iter().collect::<Vec<_>>();
    ranked.sort_by(|a, b| b.kills.cmp(&a.kills).then(a.deaths.cmp(&b.deaths)));
    draw_rectangle(x, y, w, 74. * ui + ranked.len() as f32 * 32. * ui, PANEL_DARK);
    hud::text_outlined("FREE FOR ALL", x + 14. * ui, y + 29. * ui, 26. * ui, TEXT);
    for (i, r) in ranked.iter().enumerate() {
        let ry = y + 50. * ui + i as f32 * 32. * ui;
        if Some(r.slot) == me {
            draw_rectangle(x, ry, w, 30. * ui, Color::new(1., 1., 1., 0.1));
        }
        hud::text_outlined(&r.name, x + 14. * ui, ry + 22. * ui, 21. * ui, if r.alive { TEXT } else { DIM });
        hud::text_right(&format!("{} K   {} D", r.kills, r.deaths), x + w - 16. * ui, ry + 22. * ui, 21. * ui, TEXT);
    }
}
