//! Menus: a list of buttons, toggles, choices, sliders and text fields that works with the mouse, the keyboard
//! and a controller alike. Immediate mode: call [`Menu::run`] every frame.
use macroquad::prelude::*;
use vesper3d::viewer::kit::hud;

pub const PANEL: Color = Color::new(0.04, 0.05, 0.06, 0.78);
pub const ACCENT: Color = Color::new(0.93, 0.78, 0.36, 1.);
pub const TEXT: Color = Color::new(0.92, 0.93, 0.94, 1.);
pub const DIM: Color = Color::new(0.62, 0.65, 0.68, 1.);
pub const IRONCLAD: Color = Color::new(0.47, 0.6, 0.3, 1.);
pub const NIGHTWATCH: Color = Color::new(0.3, 0.45, 0.78, 1.);

pub fn team_colour(team: usize) -> Color {
    if team == 0 {
        IRONCLAD
    } else {
        NIGHTWATCH
    }
}

/// What the devices did this frame, gathered once.
#[derive(Clone, Default)]
pub struct Nav {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub accept: bool,
    pub back: bool,
    pub typed: Vec<char>,
    pub backspace: bool,
    pub mouse: Vec2,
    pub moved: bool,
    pub click: bool,
}

impl Nav {
    /// `step` is the engine's menu step (D-pad or stick flick with repeat); `select` and `back` are the controller's
    /// confirm and cancel buttons.
    pub fn gather(step: vesper3d::viewer::devkit::MenuStep, select: bool, back: bool, last_mouse: &mut Vec2) -> Nav {
        let mouse = Vec2::from(mouse_position());
        let moved = (mouse - *last_mouse).length() > 1.5;
        *last_mouse = mouse;
        let mut typed = Vec::new();
        while let Some(c) = get_char_pressed() {
            if !c.is_control() {
                typed.push(c);
            }
        }
        Nav {
            up: step.up || is_key_pressed(KeyCode::Up),
            down: step.down || is_key_pressed(KeyCode::Down) || is_key_pressed(KeyCode::Tab),
            left: step.left || is_key_pressed(KeyCode::Left),
            right: step.right || is_key_pressed(KeyCode::Right),
            accept: select || is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter),
            back: back || is_key_pressed(KeyCode::Escape),
            typed,
            backspace: is_key_pressed(KeyCode::Backspace),
            mouse,
            moved,
            click: is_mouse_button_pressed(MouseButton::Left),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Button(String),
    Toggle(String, bool),
    Choice(String, Vec<String>, usize),
    Slider(String, f32, f32, f32),
    Text(String, String, &'static str),
    Label(String),
    Gap,
}

impl Item {
    fn selectable(&self) -> bool {
        !matches!(self, Item::Label(_) | Item::Gap)
    }
}

/// What happened in a menu this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    None,
    /// A button was pressed or an item was changed.
    Item(usize),
    Back,
}

pub struct Menu {
    pub sel: usize,
    pub time: f32,
}

impl Menu {
    pub fn new() -> Menu {
        Menu { sel: 0, time: 0. }
    }

    /// Draw `items` in a column centred at `cx` starting at `top`, handle input, and say what was used.
    /// Text items take typed characters while selected.
    pub fn run(&mut self, nav: &Nav, items: &mut [Item], cx: f32, top: f32, width: f32, dt: f32) -> Hit {
        self.time += dt;
        let ui = hud::ui_scale();
        let row = 52. * ui;
        let w = width * ui;
        let mut hit = Hit::None;
        let n = items.len();
        if n == 0 {
            return Hit::None;
        }
        self.sel = self.sel.min(n - 1);
        if !items[self.sel].selectable() {
            self.sel = items.iter().position(Item::selectable).unwrap_or(0);
        }
        let step = |from: usize, dir: i32, items: &[Item]| {
            let mut i = from as i32;
            for _ in 0..items.len() {
                i = (i + dir).rem_euclid(items.len() as i32);
                if items[i as usize].selectable() {
                    return i as usize;
                }
            }
            from
        };
        if nav.down {
            self.sel = step(self.sel, 1, items);
        }
        if nav.up {
            self.sel = step(self.sel, -1, items);
        }
        // Rows.
        let mut y = top * ui;
        let mut rects = Vec::with_capacity(n);
        for it in items.iter() {
            let h = match it {
                Item::Gap => row * 0.5,
                Item::Label(_) => row * 0.8,
                _ => row,
            };
            rects.push(Rect::new(cx - w * 0.5, y, w, h - 6. * ui));
            y += h;
        }
        if nav.moved {
            for (i, r) in rects.iter().enumerate() {
                if items[i].selectable() && r.contains(nav.mouse) {
                    self.sel = i;
                }
            }
        }
        let clicked = nav
            .click
            .then(|| rects.iter().position(|r| r.contains(nav.mouse)))
            .flatten()
            .filter(|i| items[*i].selectable());
        if let Some(i) = clicked {
            self.sel = i;
        }
        for (i, it) in items.iter_mut().enumerate() {
            let r = rects[i];
            let on = i == self.sel;
            let activate = (on && nav.accept) || clicked == Some(i);
            match it {
                Item::Gap => {}
                Item::Label(t) => hud::text_centered(t, cx, r.y + r.h * 0.7, 22. * ui, DIM),
                Item::Button(t) => {
                    draw_row(r, on, ui);
                    hud::text_centered(t, cx, r.y + r.h * 0.68, 28. * ui, if on { ACCENT } else { TEXT });
                    if activate {
                        hit = Hit::Item(i);
                    }
                }
                Item::Toggle(t, v) => {
                    draw_row(r, on, ui);
                    hud::text_outlined(t, r.x + 18. * ui, r.y + r.h * 0.68, 26. * ui, if on { ACCENT } else { TEXT });
                    hud::text_right(
                        if *v { "ON" } else { "OFF" },
                        r.x + r.w - 18. * ui,
                        r.y + r.h * 0.68,
                        26. * ui,
                        if *v { ACCENT } else { DIM },
                    );
                    if activate || (on && (nav.left || nav.right)) {
                        *v = !*v;
                        hit = Hit::Item(i);
                    }
                }
                Item::Choice(t, opts, k) => {
                    draw_row(r, on, ui);
                    hud::text_outlined(t, r.x + 18. * ui, r.y + r.h * 0.68, 26. * ui, if on { ACCENT } else { TEXT });
                    let label = opts.get(*k).cloned().unwrap_or_default();
                    hud::text_right(
                        &format!("<  {label}  >"),
                        r.x + r.w - 18. * ui,
                        r.y + r.h * 0.68,
                        26. * ui,
                        if on { ACCENT } else { TEXT },
                    );
                    let mut d = 0i32;
                    if on && nav.left {
                        d = -1;
                    }
                    if on && (nav.right || nav.accept) {
                        d = 1;
                    }
                    if clicked == Some(i) {
                        d = if nav.mouse.x < r.x + r.w * 0.55 { -1 } else { 1 };
                    }
                    if d != 0 && !opts.is_empty() {
                        *k = (*k as i32 + d).rem_euclid(opts.len() as i32) as usize;
                        hit = Hit::Item(i);
                    }
                }
                Item::Slider(t, v, lo, hi) => {
                    draw_row(r, on, ui);
                    hud::text_outlined(t, r.x + 18. * ui, r.y + r.h * 0.68, 26. * ui, if on { ACCENT } else { TEXT });
                    let track = Rect::new(r.x + r.w * 0.5, r.y + r.h * 0.45, r.w * 0.45, 8. * ui);
                    draw_rectangle(track.x, track.y, track.w, track.h, Color::new(1., 1., 1., 0.15));
                    let f = ((*v - *lo) / (*hi - *lo)).clamp(0., 1.);
                    draw_rectangle(track.x, track.y, track.w * f, track.h, if on { ACCENT } else { DIM });
                    let old = *v;
                    if on && nav.left {
                        *v = (*v - (*hi - *lo) * 0.05).max(*lo);
                    }
                    if on && nav.right {
                        *v = (*v + (*hi - *lo) * 0.05).min(*hi);
                    }
                    if is_mouse_button_down(MouseButton::Left)
                        && Rect::new(track.x - 6., r.y, track.w + 12., r.h).contains(nav.mouse)
                    {
                        *v = *lo + ((nav.mouse.x - track.x) / track.w).clamp(0., 1.) * (*hi - *lo);
                    }
                    if (*v - old).abs() > f32::EPSILON {
                        hit = Hit::Item(i);
                    }
                }
                Item::Text(t, v, hint) => {
                    draw_row(r, on, ui);
                    hud::text_outlined(t, r.x + 18. * ui, r.y + r.h * 0.68, 26. * ui, if on { ACCENT } else { TEXT });
                    let caret = if on && (self.time * 2.).fract() < 0.5 { "|" } else { "" };
                    let shown = if v.is_empty() && !on { hint.to_string() } else { format!("{v}{caret}") };
                    hud::text_right(
                        &shown,
                        r.x + r.w - 18. * ui,
                        r.y + r.h * 0.68,
                        26. * ui,
                        if v.is_empty() && !on { DIM } else { TEXT },
                    );
                    if on {
                        for c in &nav.typed {
                            if v.chars().count() < 60 {
                                v.push(*c);
                                hit = Hit::Item(i);
                            }
                        }
                        if nav.backspace {
                            v.pop();
                            hit = Hit::Item(i);
                        }
                    }
                }
            }
        }
        if hit == Hit::None && nav.back {
            hit = Hit::Back;
        }
        hit
    }
}

impl Default for Menu {
    fn default() -> Self {
        Self::new()
    }
}

fn draw_row(r: Rect, on: bool, ui: f32) {
    draw_rectangle(r.x, r.y, r.w, r.h, if on { Color::new(0.14, 0.13, 0.08, 0.9) } else { PANEL });
    if on {
        draw_rectangle(r.x, r.y, 4. * ui, r.h, ACCENT);
    }
}

/// A translucent panel with a title.
pub fn panel(x: f32, y: f32, w: f32, h: f32, title: &str) {
    let ui = hud::ui_scale();
    draw_rectangle(x, y, w, h, PANEL);
    draw_rectangle(x, y, w, 3. * ui, ACCENT);
    if !title.is_empty() {
        hud::text_outlined(title, x + 16. * ui, y + 30. * ui, 24. * ui, ACCENT);
    }
}

/// `seconds` as `m:ss`.
pub fn clock(seconds: f32) -> String {
    let s = seconds.max(0.) as u32;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Play time as `12h 05m`.
pub fn duration(seconds: u64) -> String {
    let (h, m) = (seconds / 3600, seconds % 3600 / 60);
    if h > 0 {
        format!("{h}h {m:02}m")
    } else {
        format!("{m}m {:02}s", seconds % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_read_naturally() {
        assert_eq!(clock(605.), "10:05");
        assert_eq!(clock(-3.), "0:00");
        assert_eq!(duration(3 * 3600 + 7 * 60 + 9), "3h 07m");
        assert_eq!(duration(75), "1m 15s");
    }
}
