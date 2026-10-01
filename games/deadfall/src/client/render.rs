//! Drawing a match: the map, the soldiers with their weapons, dropped weapons, grenades, smoke and fire, tracers
//! and flashes, and the first-person viewmodel (weapon and arms) in its own depth pass so it never pokes through
//! a wall.
use super::arms::{first_person_arms, ArmPose};
use super::character::{Hold, Pose, Rig};
use super::level_view::{self, LevelScene};
use super::weapon_models::{self, WeaponModel};
use crate::hands::{Busy, Hands};
use crate::level::Level;
use crate::netgame::{flag, DroppedView, PlayerView, ProjView, ZoneView};
use crate::team::Team;
use crate::weapons::{self, Class, Sight};
use macroquad::prelude::*;
use macroquad::texture::{render_target_ex, RenderTarget, RenderTargetParams};
use std::collections::HashMap;
use vesper3d::math::V;
use vesper3d::viewer::kit::{hud, Batch, Fx, Materials, PointLight, Template, Tint, View};

type KitMesh = macroquad::prelude::Mesh;

pub fn v3(v: V) -> Vec3 {
    vec3(v.0, v.1, v.2)
}

/// Horizontal field of view of the normal view, degrees.
pub const HFOV: f32 = 90.;

/// Vertical field of view (radians) that gives `hfov_deg` horizontally on a window of this shape.
pub fn vfov(hfov_deg: f32, aspect: f32) -> f32 {
    2. * ((hfov_deg.to_radians() * 0.5).tan() / aspect.max(0.5)).atan()
}

/// One soldier as it should be drawn this frame.
#[derive(Clone, Copy, Debug)]
pub struct Figure {
    pub slot: usize,
    pub team: Team,
    pub view: PlayerView,
}

/// Per-soldier animation memory: distance walked, smoothed crouch, time since death.
#[derive(Clone, Copy, Debug, Default)]
struct Mem {
    last: Option<Vec3>,
    phase: f32,
    speed: f32,
    crouch: f32,
    dead: f32,
    fire: f32,
    reload: f32,
    was_reloading: bool,
}

#[derive(Clone, Copy)]
struct Tracer {
    from: Vec3,
    to: Vec3,
    life: f32,
    big: bool,
}

#[derive(Clone, Copy)]
struct Flash {
    pos: Vec3,
    life: f32,
}

pub struct Renderer {
    pub materials: Materials,
    pub scene: LevelScene,
    static_meshes: Vec<KitMesh>,
    decor_meshes: Vec<KitMesh>,
    glass_meshes: Vec<KitMesh>,
    sky_meshes: Vec<KitMesh>,
    level: Level,
    rigs: Vec<Rig>,
    models: Vec<Option<WeaponModel>>,
    pub world: Batch,
    pub alpha: Batch,
    pub add: Batch,
    pub fx: Fx,
    mem: HashMap<usize, Mem>,
    tracers: Vec<Tracer>,
    flashes: Vec<Flash>,
    vm_target: Option<(RenderTarget, u32, u32)>,
    pub time: f32,
    pub muzzle_flash: f32,
    arms_cache: Option<(u64, Template)>,
}

impl Renderer {
    pub fn new(level: &Level) -> Renderer {
        let scene = level_view::build(level);
        let to_meshes = |ts: &[Template]| ts.iter().flat_map(|t| t.to_meshes()).collect::<Vec<_>>();
        let static_meshes = to_meshes(&scene.solid);
        let decor_meshes = to_meshes(&scene.decor);
        let glass_meshes = scene.glass.to_meshes();
        let sky_meshes = scene.sky.to_meshes();
        let mut rigs = Vec::new();
        for team in Team::ALL {
            for skin in 0..4 {
                rigs.push(Rig::new(team, skin));
            }
        }
        let mut models: Vec<Option<WeaponModel>> = vec![None];
        for w in weapons::WEAPONS {
            models.push(weapon_models::build(w.key));
        }
        Renderer {
            materials: Materials::load().expect("the materials failed to compile"),
            scene,
            static_meshes,
            decor_meshes,
            glass_meshes,
            sky_meshes,
            level: level.clone(),
            rigs,
            models,
            world: Batch::new(),
            alpha: Batch::new(),
            add: Batch::new(),
            fx: Fx::new(7),
            mem: HashMap::new(),
            tracers: Vec::new(),
            flashes: Vec::new(),
            vm_target: None,
            time: 0.,
            muzzle_flash: 0.,
            arms_cache: None,
        }
    }

    pub fn model(&self, weapon: u8) -> Option<&WeaponModel> {
        self.models.get(weapon as usize).and_then(|m| m.as_ref())
    }

    pub fn clear_match(&mut self) {
        self.mem.clear();
        self.tracers.clear();
        self.flashes.clear();
        self.fx.clear();
    }

    pub fn tracer(&mut self, from: V, to: V, big: bool) {
        self.tracers.push(Tracer { from: v3(from), to: v3(to), life: 0.07, big });
    }

    pub fn flash(&mut self, pos: V) {
        self.flashes.push(Flash { pos: v3(pos), life: 0.06 });
    }

    pub fn update(&mut self, dt: f32) {
        self.time += dt;
        self.muzzle_flash = (self.muzzle_flash - dt).max(0.);
        self.fx.update(dt);
        for t in &mut self.tracers {
            t.life -= dt;
        }
        self.tracers.retain(|t| t.life > 0.);
        for f in &mut self.flashes {
            f.life -= dt;
        }
        self.flashes.retain(|f| f.life > 0.);
    }

    /// Advance everyone's animation from their latest positions.
    fn animate(&mut self, figures: &[Figure], dt: f32) {
        for f in figures {
            let m = self.mem.entry(f.slot).or_default();
            let pos = v3(V(f.view.eye.0, f.view.feet, f.view.eye.2));
            let alive = f.view.has(flag::ALIVE);
            if let Some(last) = m.last {
                let d = pos - last;
                let moved = vec2(d.x, d.z).length();
                if moved < 3. {
                    m.speed += (moved / dt.max(0.001) - m.speed) * (1. - (-dt * 10.).exp());
                    m.phase += moved * std::f32::consts::PI / 0.75;
                } else {
                    m.speed = 0.;
                }
            }
            m.last = Some(pos);
            let crouch_target = if f.view.has(flag::CROUCH) { 1. } else { 0. };
            m.crouch += (crouch_target - m.crouch) * (1. - (-dt * 14.).exp());
            m.dead = if alive { 0. } else { (m.dead + dt * 1.6).min(1.) };
            m.fire = if f.view.has(flag::FIRING) { 1. } else { (m.fire - dt * 8.).max(0.) };
            let reloading = f.view.has(flag::RELOAD);
            if reloading {
                if !m.was_reloading {
                    m.reload = 0.001;
                }
                m.reload = (m.reload + dt / 1.8).min(0.999);
            } else {
                m.reload = 0.;
            }
            m.was_reloading = reloading;
        }
        let alive: Vec<usize> = figures.iter().map(|f| f.slot).collect();
        self.mem.retain(|k, _| alive.contains(k));
    }

    /// The hold a weapon wants.
    pub fn hold_for(weapon: u8) -> Hold {
        match weapons::get(weapon).map(|d| d.class) {
            Some(Class::Pistol) => Hold::Pistol,
            Some(Class::Smg) => Hold::Smg,
            Some(Class::AssaultRifle | Class::Dmr | Class::Lmg | Class::Shotgun) => Hold::Rifle,
            Some(Class::Sniper) => Hold::Sniper,
            Some(Class::Launcher) => Hold::Launcher,
            Some(Class::Grenade) => Hold::Grenade,
            Some(Class::Melee) => Hold::Melee,
            None => Hold::Unarmed,
        }
    }

    /// Everything in the world from the camera `view` (after which the viewmodel and the HUD go on top).
    #[allow(clippy::too_many_arguments)]
    pub fn draw_world(
        &mut self,
        view: &View,
        figures: &[Figure],
        skip_slot: Option<usize>,
        loot: u64,
        dropped: &[DroppedView],
        projectiles: &[ProjView],
        zones: &[ZoneView],
        skins: &[u8; 16],
        dt: f32,
    ) {
        self.animate(figures, dt);
        let look = &self.scene.look;
        clear_background(look.clear_color());
        // Sky: drawn with the camera at the origin, no depth.
        set_camera(&view.sky_camera());
        gl_use_material(&self.materials.sky);
        for m in &self.sky_meshes {
            draw_mesh(m);
        }
        set_camera(&view.camera(0.05, 400.));
        // The nearest fixed lights, plus a flash of light at every muzzle that just fired.
        let mut spots: Vec<&crate::level::LightSpot> = self.level.lights.iter().collect();
        spots.sort_by(|a, b| {
            (v3(a.pos) - view.eye)
                .length()
                .partial_cmp(&(v3(b.pos) - view.eye).length())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut chosen: Vec<PointLight> = Vec::new();
        for l in self.flashes.iter().take(2) {
            if let Ok(p) = PointLight::new(l.pos, 6., [1., 0.85, 0.5], 2.5) {
                chosen.push(p);
            }
        }
        for s in spots.into_iter().take(4) {
            if let Ok(p) = PointLight::new(v3(s.pos), s.radius, s.rgb, s.intensity) {
                chosen.push(p);
            }
        }
        chosen.truncate(4);
        self.materials.set_scene(look, view.eye, self.time, 0.);
        let _ = self.materials.set_point_lights(&chosen);
        gl_use_material(&self.materials.world);
        for m in &self.static_meshes {
            draw_mesh(m);
        }
        for m in &self.decor_meshes {
            draw_mesh(m);
        }
        // Dynamic geometry.
        self.world.clear();
        self.alpha.clear();
        self.add.clear();
        let t = self.time;
        for f in figures {
            if Some(f.slot) == skip_slot {
                continue;
            }
            let skin = skins[f.slot.min(15)].min(3);
            let rig = &self.rigs[f.team.index() * 4 + skin as usize];
            let mem = self.mem.get(&f.slot).copied().unwrap_or_default();
            let weapon = f.view.weapon;
            let hold = Self::hold_for(weapon);
            let pose = Pose {
                speed: mem.speed,
                walk_phase: mem.phase,
                crouch: mem.crouch,
                pitch: f.view.pitch,
                aim: if f.view.has(flag::ADS) { 1. } else { 0. },
                hold,
                recoil: mem.fire,
                reload: mem.reload,
                swing: 0.,
                throwing: 0.,
                dead: mem.dead,
                airborne: f.view.has(flag::AIR),
            };
            let feet = vec3(f.view.eye.0, f.view.feet, f.view.eye.2);
            let protected = f.view.has(flag::PROTECT);
            let tint = if protected { Tint::flash(0.15 + 0.1 * (t * 12.).sin().abs()) } else { Tint::NONE };
            rig.draw(&mut self.world, feet, f.view.yaw, &pose, tint);
            if let Some(model) = self.models.get(weapon as usize).and_then(|m| m.as_ref()) {
                let mount = rig.weapon_mount(feet, f.view.yaw, &pose);
                self.world.add(&model.body, mount, Tint::NONE);
                if let Some((mag, at)) = &model.mag {
                    self.world.add(mag, mount * Mat4::from_translation(*at), Tint::NONE);
                }
                if let Some((slide, _)) = &model.slide {
                    self.world.add(slide, mount, Tint::NONE);
                }
                if mem.fire > 0.6 && f.view.has(flag::ALIVE) {
                    let muzzle = mount.transform_point3(model.anchors.muzzle);
                    let v = View::first_person(view.eye, view.yaw, view.pitch);
                    Self::star(&mut self.add, muzzle, v.right(), v.up(), view.eye, 0.28, [1., 0.75, 0.35]);
                }
            }
        }
        // Loot and dropped weapons, turning slowly above the floor.
        for (i, l) in self.level.loot.iter().enumerate() {
            if i < 64 && loot & (1 << i) != 0 {
                Self::draw_item(&self.models, &mut self.world, &mut self.add, l.weapon, v3(l.pos), t, i as f32);
            }
        }
        for d in dropped {
            Self::draw_item(&self.models, &mut self.world, &mut self.add, d.weapon, v3(d.pos), t, d.id as f32);
        }
        for p in projectiles {
            let pos = v3(p.pos);
            match weapons::get(p.weapon).map(|d| d.class) {
                Some(Class::Launcher) => {
                    let mut rocket = Template::new();
                    rocket.cylinder(vec3(0., -0.05, 0.), 0.04, 0.5, [0.25, 0.3, 0.2], 0., 8);
                    rocket.cone(vec3(0., 0.45, 0.), 0.045, 0., 0.15, [0.6, 0.2, 0.15], 0., 8);
                    self.world.add(
                        &rocket,
                        Mat4::from_translation(pos) * Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2),
                        Tint::NONE,
                    );
                    let v = View::first_person(view.eye, view.yaw, view.pitch);
                    Self::star(&mut self.add, pos, v.right(), v.up(), view.eye, 0.5, [1., 0.6, 0.2]);
                }
                _ => {
                    if let Some(m) = self.models.get(p.weapon as usize).and_then(|m| m.as_ref()) {
                        self.world.add(
                            &m.body,
                            Mat4::from_translation(pos) * Mat4::from_rotation_y(t * 9.) * Mat4::from_rotation_x(t * 7.),
                            Tint::NONE,
                        );
                    }
                }
            }
        }
        gl_use_material(&self.materials.world);
        self.world.draw();
        // Translucent: glass, smoke, then additive light.
        gl_use_material(&self.materials.fx_alpha);
        for m in &self.glass_meshes {
            draw_mesh(m);
        }
        let right = view.right();
        let up = view.up();
        for z in zones {
            let c = v3(z.pos);
            if z.kind == 0 {
                let fade = (z.seconds_left / 3.).clamp(0., 1.);
                for k in 0..9 {
                    let a = k as f32 * 2.4 + t * 0.15;
                    let r = z.radius * (0.25 + 0.55 * ((k * 7 % 5) as f32 / 5.));
                    let p = c + vec3(
                        a.cos() * r * 0.7,
                        0.2 + (k % 3) as f32 * 0.9 + (t * 0.3 + k as f32).sin() * 0.15,
                        a.sin() * r * 0.7,
                    );
                    self.alpha.billboard(
                        p,
                        right,
                        up,
                        z.radius * 1.3,
                        z.radius * 1.3,
                        [0.72, 0.74, 0.76],
                        0.55 * fade,
                        0.,
                    );
                }
            }
        }
        gl_use_material(&self.materials.fx_alpha);
        self.fx.draw(&mut self.add, &mut self.alpha, view.eye, right, up);
        self.alpha.draw();
        for tr in &self.tracers {
            let a = tr.life / 0.07;
            self.add.beam(
                tr.from,
                tr.to,
                view.eye,
                if tr.big { 0.06 } else { 0.025 },
                [1., 0.9, 0.6],
                0.0 + a * 0.15,
                a,
                1.,
            );
        }
        for z in zones {
            if z.kind == 1 {
                for k in 0..6 {
                    let a = k as f32 * 1.05 + t;
                    let r = z.radius * 0.6 * ((k * 5 % 6) as f32 / 6. + 0.2);
                    let p =
                        v3(z.pos) + vec3(a.cos() * r, 0.3 + (t * 9. + k as f32 * 2.).sin().abs() * 0.4, a.sin() * r);
                    self.add.billboard(
                        p,
                        right,
                        up,
                        0.8,
                        1.0,
                        [1., 0.5 + 0.2 * (t * 7. + k as f32).sin().abs(), 0.1],
                        0.6,
                        1.,
                    );
                }
            }
        }
        gl_use_material(&self.materials.fx_add);
        self.add.draw();
        gl_use_default_material();
        set_default_camera();
    }

    fn draw_item(
        models: &[Option<WeaponModel>],
        world: &mut Batch,
        add: &mut Batch,
        weapon: u8,
        at: Vec3,
        t: f32,
        phase: f32,
    ) {
        let Some(model) = models.get(weapon as usize).and_then(|m| m.as_ref()) else { return };
        let bob = (t * 2. + phase).sin() * 0.05;
        let m = Mat4::from_translation(at + vec3(0., 0.45 + bob, 0.))
            * Mat4::from_rotation_y(t * 0.8 + phase)
            * Mat4::from_rotation_z(0.15);
        world.add(&model.body, m, Tint::NONE);
        if let Some((mag, off)) = &model.mag {
            world.add(mag, m * Mat4::from_translation(*off), Tint::NONE);
        }
        // A faint glow on the floor so it can be spotted from across the yard.
        let mut glow = Template::new();
        glow.disc(vec3(0., 0.03, 0.), 0.45, [0.9, 0.85, 0.5], 0.8, 16);
        add.add(&glow, Mat4::from_translation(at), Tint::alpha(0.25));
    }

    /// The weapon and arms of the first-person view, drawn in their own depth pass and composited on top.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_viewmodel(
        &mut self,
        view: &View,
        hands: &Hands,
        weapon: u8,
        team: Team,
        skin: u8,
        speed: f32,
        walk_phase: f32,
        sway: (f32, f32),
        hide: f32,
    ) {
        let Some(model) = self.models.get(weapon as usize).and_then(|m| m.as_ref()) else { return };
        let Some(def) = weapons::get(weapon) else { return };
        let (w, h) = (screen_width() as u32, screen_height() as u32);
        if w < 16 || h < 16 {
            return;
        }
        if self.vm_target.as_ref().is_none_or(|(_, tw, th)| *tw != w || *th != h) {
            let rt = render_target_ex(w, h, RenderTargetParams { depth: true, ..Default::default() });
            rt.texture.set_filter(FilterMode::Linear);
            self.vm_target = Some((rt, w, h));
        }
        let (rt, _, _) = self.vm_target.as_ref().expect("the viewmodel target exists");
        let rt = rt.clone();

        let ads = hands.ads;
        let a = ads * ads * (3. - 2. * ads);
        let p = hands.progress();
        let anchors = model.anchors;
        // Where the grip sits on screen at the hip and when aiming (the sight point lands on the eye).
        let hip = vec3(0.16, -0.17, -0.32);
        // The eye sits a little behind the sight (eye relief), so the rear sight is not a wall across the screen.
        let relief = match (def.sight, def.class) {
            (Sight::Scope { .. }, _) => 0.1,
            (_, Class::Pistol) => 0.55,
            (Sight::Dot, _) => 0.3,
            _ => 0.4,
        };
        let aimed = -anchors.sight + vec3(0., 0., -relief);
        let mut pos = hip.lerp(aimed, a);
        let mut rot = Mat4::IDENTITY;
        // Walking bob and idle breathing, mostly gone when aiming.
        let bobk = (speed / 6.).clamp(0., 1.) * (1. - 0.85 * a);
        pos += vec3((walk_phase).cos() * 0.012 * bobk, (walk_phase * 2.).sin().abs() * -0.014 * bobk, 0.)
            + vec3(0., (self.time * 1.6).sin() * 0.0025, 0.);
        // Look sway: the weapon lags behind the view.
        pos += vec3(-sway.0 * 0.9, sway.1 * 0.9, 0.) * (1. - 0.6 * a);
        // Recoil kicks the weapon back and up.
        let kick = (hands.recoil / 4.).min(1.);
        pos += vec3(0., 0.008 * kick, 0.03 * kick * (1. - 0.5 * a));
        rot = Mat4::from_rotation_x(0.03 * kick) * rot;
        // What the hands are doing.
        let mut arm = ArmPose { ads: a, draw: 1., pin: hands.pin, ..Default::default() };
        let mut mag_shift = Vec3::ZERO;
        match hands.busy {
            Busy::Draw => {
                let d = p * p * (3. - 2. * p);
                arm.draw = d;
                pos += vec3(0.04 * (1. - d), -0.32 * (1. - d), 0.06 * (1. - d));
                rot = Mat4::from_rotation_x(-0.7 * (1. - d)) * rot;
            }
            Busy::Reload | Busy::ShellLoad => {
                arm.reload = p.clamp(0.001, 0.999);
                let s = (p * std::f32::consts::PI).sin();
                pos += vec3(-0.03 * s, -0.11 * s, 0.03 * s);
                rot = Mat4::from_rotation_x(-0.35 * s) * Mat4::from_rotation_z(0.25 * s) * rot;
                // The magazine drops out and is seated again.
                mag_shift = if p < 0.4 {
                    vec3(0., -0.3 * (p / 0.4), 0.)
                } else if p < 0.55 {
                    vec3(0., -0.3, 0.)
                } else {
                    vec3(0., -0.3 * (1. - (p - 0.55) / 0.3).clamp(0., 1.), 0.)
                };
            }
            Busy::Swing => {
                arm.swing = p;
                let s = (p * std::f32::consts::PI).sin();
                pos += vec3(-0.12 * s, -0.02 * s, -0.08 * s);
                let sweep = -1.0 + 2.2 * p;
                rot = Mat4::from_rotation_z(sweep * 0.8) * Mat4::from_rotation_y(-0.5 * s) * rot;
            }
            Busy::Throw => {
                arm.throwing = p;
                pos += vec3(0., 0.12 * (1. - p), -0.25 * (p * std::f32::consts::PI).sin());
            }
            Busy::Cycle => {
                let s = (p * std::f32::consts::PI).sin();
                pos += vec3(0.0, -0.01 * s, 0.03 * s);
                rot = Mat4::from_rotation_z(0.25 * s) * rot;
            }
            Busy::Idle => {}
        }
        if hands.pin {
            pos += vec3(0., 0.06, -0.04);
        }
        // Scoped weapons are hidden while looking through the scope (the overlay takes over).
        let scoped = matches!(def.sight, Sight::Scope { .. }) && ads > 0.92;
        if scoped || hide >= 1. {
            return;
        }
        let local = Mat4::from_translation(pos) * rot;
        let hold = Self::hold_for(weapon);
        let key = (p.to_bits() as u64) << 32
            | (a.to_bits() as u64)
                ^ (weapon as u64) << 8
                ^ (hands.busy as u64)
                ^ (team.index() as u64) << 4
                ^ (skin as u64) << 6;
        let arms = match &self.arms_cache {
            Some((k, t)) if *k == key => t.clone(),
            _ => {
                let t = first_person_arms(team, skin, &anchors, hold, &arm);
                self.arms_cache = Some((key, t.clone()));
                t
            }
        };
        // The viewmodel is drawn from the eye with the same angles as the world, so lighting matches.
        let to_world =
            Mat4::from_translation(view.eye) * Mat4::from_rotation_y(-view.yaw) * Mat4::from_rotation_x(view.pitch);
        let m = to_world * local;
        let vm_view = View { fov: 58f32.to_radians(), ..*view };
        let cam = Camera3D {
            position: vm_view.eye,
            target: vm_view.eye + vm_view.dir(),
            up: vm_view.up(),
            fovy: vm_view.fov,
            z_near: 0.03,
            z_far: 10.,
            render_target: Some(rt.clone()),
            ..Default::default()
        };
        set_camera(&cam);
        clear_background(Color::new(0., 0., 0., 0.));
        let mut look = self.scene.look.clone();
        look.fog_density = 0.;
        self.materials.set_scene(&look, view.eye, self.time, 0.);
        let _ = self.materials.set_point_lights(&[]);
        let mut batch = Batch::new();
        batch.add(&model.body, m, Tint::NONE);
        if let Some((mag, off)) = &model.mag {
            batch.add(mag, m * Mat4::from_translation(*off + mag_shift), Tint::NONE);
        }
        if let Some((slide, travel)) = &model.slide {
            batch.add(slide, m * Mat4::from_translation(*travel * (kick * 0.8)), Tint::NONE);
        }
        batch.add(&arms, m, Tint::NONE);
        gl_use_material(&self.materials.world);
        batch.draw();
        if self.muzzle_flash > 0. {
            let mut glow = Batch::new();
            let muzzle = m.transform_point3(anchors.muzzle);
            Self::star(&mut glow, muzzle, vm_view.right(), vm_view.up(), vm_view.eye, 0.16, [1., 0.8, 0.4]);
            gl_use_material(&self.materials.fx_add);
            glow.draw();
        }
        gl_use_default_material();
        set_default_camera();
        // Composite over the screen.
        draw_texture_ex(
            &rt.texture,
            0.,
            0.,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(screen_width(), screen_height())),
                flip_y: true,
                ..Default::default()
            },
        );
    }

    /// A spiky flash instead of a flat square: two crossed beams and a hot core.
    fn star(add: &mut Batch, centre: Vec3, right: Vec3, up: Vec3, eye: Vec3, size: f32, colour: [f32; 3]) {
        for k in 0..3 {
            let a = k as f32 * std::f32::consts::FRAC_PI_3;
            let d = right * a.cos() + up * a.sin();
            add.beam(centre - d * size, centre + d * size, eye, size * 0.28, colour, 0.0, 0.9, 1.);
        }
        add.billboard(centre, right, up, size * 0.5, size * 0.5, [1., 0.95, 0.8], 0.9, 1.);
    }

    pub fn vignette(&self) -> Texture2D {
        hud::make_vignette()
    }
}
