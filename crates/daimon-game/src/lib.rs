//! Daimon: Smallworld — a wgpu game in which several real Daimon minds share a
//! world you can watch and tend. This module is the platform glue (winit +
//! wgpu) and the game loop; cognition is the published `daimon-*` crates.
//!
//! Runs natively (block-on GPU init) and on the web (async GPU init delivered
//! back through an [`EventLoopProxy`], the standard winit + wgpu wasm pattern).

pub mod evolve_mode;
pub mod fitness;
pub mod hell;
pub mod poet;
pub mod redqueen;
pub mod geo;
pub mod gfx;
pub mod math;
pub mod scene;
pub mod sim;
pub mod view;

use std::sync::Arc;
use web_time::Instant;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use gfx::Gfx;
use sim::GameWorld;
use view::{Camera, Hud};

/// Delivered when the (async) GPU context finishes initialising.
#[allow(dead_code)]
enum AppEvent {
    GfxReady(Gfx),
}

/// All non-GPU game state.
struct Game {
    world: GameWorld,
    cam: Camera,
    hud: Hud,
    /// Present in `--evolve` mode: the live generational evolution driver. When
    /// `Some`, it owns the watched world and is fast-forwarded each frame; the
    /// village `world` field above is then unused.
    evolve: Option<evolve_mode::Evolution>,
    selected: Option<usize>,
    acc: f32,
    mouse: (f32, f32),
    mouse_down: bool,
    drag_dist: f32,
    /// Held WASD movement keys (w, a, s, d) — applied smoothly each frame so panning
    /// is continuous while held, not a jerky one-step-per-keypress. In first-person
    /// these same keys WALK the eye (forward/back + strafe) instead of panning.
    keys: [bool; 4],
    /// Held Shift — sprint (faster walk) in first-person.
    run: bool,
}

impl Game {
    fn new() -> Self {
        // Run the *trained* policy the autogenesis loop proved reaches the end
        // goal (anticipation + commons-aware foraging), not the untuned default —
        // so the village showcases the believable behaviour, not the baseline.
        // Building is ENABLED for the live game so the village can wall itself in
        // for shelter: we clone showcase and flip the can_build gene (21) high here,
        // leaving `Genome::showcase()` itself bit-identical (building off) so every
        // AC, proof, and fitness run is unchanged.
        let mut genome = daimon_mind::Genome::showcase();
        genome.g[21] = 1.0; // can_build on — agents may build shelters in the game
        // MORTALITY + GRIEF on for the live game: minds can die for good and the
        // village can depopulate, and survivors who lose a *bonded* friend mourn.
        // Like building, we flip these on a clone of showcase; `Genome::showcase()`
        // itself stays bit-identical (both off) so every AC/proof/fitness run is
        // unchanged.
        genome.g[22] = 1.0; // can_die on — permadeath + fear of death
        genome.g[23] = 1.0; // can_grieve on — real bereavement for bonded losses
        // PROVISIONING on so minds adopt the gather goal — in the live game its targets
        // are BUILDING MATERIALS (wood from trees, stone from quarry rocks), hauled into
        // the village stockpile that every wall consumes (materials economy, below).
        // Like the others, flipped on a clone; `Genome::showcase()` stays bit-identical
        // (g[24]=0) so every AC/proof/fitness run is unchanged.
        genome.g[24] = 1.0; // can_provision on — minds gather materials for building
        // LIFE-CYCLE on for the live game: the village becomes a living lineage —
        // adults meet mates and form lasting pair-bonds, settled fed pairs have
        // INHERITED children, the young grow up, and elders pass of old age. Like the
        // others, flipped on a clone of showcase; `Genome::showcase()` itself stays
        // bit-identical (g[29..33]=0) so every AC/proof/fitness run is unchanged.
        genome.g[29] = 1.0; // can_mate on — seek a partner, form a romantic pair-bond
        genome.g[30] = 1.0; // can_reproduce on — a settled pair has inherited children
        genome.g[31] = 1.0; // can_age on — minds age and die a peaceful natural death
        genome.g[32] = 1.0; // feel_happiness on — surface felt contentment per-mind
        // SOCIETY on for the live game: minds belong to VILLAGES that form alliances
        // and rivalries. Like the others, flipped on a clone of showcase; the preset
        // itself stays bit-identical (g[33]=0) so every AC/proof/fitness run is unchanged.
        genome.g[33] = 1.0; // village_affinity on — feel a settlement identity, be wary of enemies
        // Seed 0x61 was chosen (from a sweep) for the believable mortality arc: with
        // the softened stalker, the village bonds and persists, then ~2 minutes in
        // loses one bonded member to the stalker — whom the survivors genuinely
        // grieve — and the remaining five live on. An occasional, meaningful loss,
        // never a bloodbath (verified stable at 5/6 over 18k ticks).
        // A big, teeming island: many more minds on a much larger landmass than the
        // original 6-on-40x26, so the showcase reads as a living world, not a hamlet.
        // Density is kept near the tuned village's (≈1 mind / 150 cells) so behaviour
        // stays believable; the camera frames the whole island below.
        const VILLAGE_POP: usize = 64;
        const VILLAGE_W: i32 = 124;
        const VILLAGE_H: i32 = 84;
        let mut world = GameWorld::with_genome_sized(0x61, VILLAGE_POP, &genome, VILLAGE_W, VILLAGE_H, 7);
        // PERPETUAL GROWING SEASON for the big showcase. The seasonal year's winter
        // halts food and relies on a single central granary the village stocks in the
        // good months — tuned for a tight 6-mind hamlet, it cannot feed 64 minds
        // spread across a large island (the hauls are too long, food too dispersed),
        // so the first winter wiped the village. Left as a year-round growing season,
        // the 64-mind village thrives (~63/64 stable over thousands of ticks). Death
        // is still real and grievable — the softened stalker takes the occasional
        // member, whom the survivors mourn. (Seasonal provisioning at scale is a
        // future sim fix; the harness/AC46 still exercise the full seasonal year.)
        world.soften_stalker();
        // MATERIALS ECONOMY on for the live showcase: a grove of trees + quarry rocks
        // the minds harvest into a shared village stockpile, and every wall they build
        // consumes wood + stone from it — no materials, no building. Live-only: the
        // seeded harness/AC/proof paths never call this (they keep building free and
        // byte-identical). Seeds the resource nodes off side-RNGs so the main stream is
        // untouched, plus a starter stockpile so the first buildings rise straight away.
        world.set_materials_world(true);
        // NATURAL ECOSYSTEM on for the live showcase: wolves roam in loose packs and
        // hunt, a solitary bear roams slowly, and a deer herd grazes and flees. The
        // minds perceive wolves & bears as predators and flee them through the EXISTING
        // cognition — a wolf-kill is grieved exactly like a stalker-kill. Live-only: the
        // seeded harness/AC/proof paths never call this (they keep only the single
        // stalker and stay byte-identical). All wildlife is seeded + stepped off a
        // dedicated side-RNG so the main deterministic stream is untouched. Tuned so the
        // village mostly thrives with the occasional grievable loss, not a bloodbath.
        world.set_wildlife(true);
        // LIFE-CYCLE on for the live showcase: aging, pair-bonds, inherited children,
        // natural death. Live-only and seeded off a dedicated side-RNG, so the seeded
        // harness/AC/proof paths (which never call this) stay byte-identical. The
        // population is capped so the lineage turns over without exploding — a few
        // hundred at most on this island (~3× the founding 64).
        world.set_lifecycle(true, 90);
        // SOCIETY on for the live showcase: cluster the founding 64 into distinct
        // VILLAGES (kinship keeps each coherent as the lineage turns over), whose
        // ALLIANCES and RIVALRIES emerge + shift from how they interact. Live-only and
        // seeded off a dedicated side-RNG, so the seeded harness/AC/proof paths (which
        // never call this) stay byte-identical. Four villages reads as a small society
        // on this island without fragmenting the population below viability.
        world.set_society(true, 4);
        let mut cam = Camera::new(world.w as f32 * 0.5, world.h as f32 * 0.5);
        // A closer cinematic frame than "whole island" so the minds and their built
        // structures read with real detail on load (buildings are tiny at full-island
        // zoom); the slow camera orbit + scroll-zoom still reveal the island's full
        // sprawl. ≈0.34× the larger axis shows a generous, legible slice of the village
        // — close enough that the wolves, bears, and deer of the ecosystem read clearly
        // on load, while the orbit + scroll-zoom still reveal the whole island.
        cam.zoom = (world.w.max(world.h) as f32) * 0.34;
        Game {
            world,
            cam,
            hud: Hud { paused: false, speed: 3.0, quantum: false },
            evolve: None,
            selected: None,
            acc: 0.0,
            mouse: (0.0, 0.0),
            mouse_down: false,
            drag_dist: 0.0,
            keys: [false; 4],
            run: false,
        }
    }

    /// `--evolve` mode: a big harsh island of `pop` minds running generational
    /// natural selection at max speed. Live-only; touches none of the harness
    /// constructors. The camera is framed to fit the whole island.
    fn evolve(pop: usize) -> Self {
        let ev = evolve_mode::Evolution::new(0xE001u64, pop);
        let (w, h, _) = ev.dims;
        let mut cam = Camera::new(w as f32 * 0.5, h as f32 * 0.5);
        // frame the whole island: vertical half-extent ≈ half the larger axis.
        cam.zoom = (w.max(h) as f32) * 0.62;
        // a placeholder village world (unused while `evolve` is Some) so the field
        // stays a real GameWorld without a second code path everywhere.
        let world = GameWorld::new(0x61, 1);
        Game {
            world,
            cam,
            hud: Hud { paused: false, speed: 8.0, quantum: false },
            evolve: Some(ev),
            selected: None,
            acc: 0.0,
            mouse: (0.0, 0.0),
            mouse_down: false,
            drag_dist: 0.0,
            keys: [false; 4],
            run: false,
        }
    }

    /// The world the renderer should draw — the evolution island if in that mode,
    /// else the village.
    fn view_world(&self) -> &GameWorld {
        match &self.evolve {
            Some(ev) => &ev.world,
            None => &self.world,
        }
    }

    /// Smooth per-frame WASD pan from the held keys. Direction is normalised (so a
    /// diagonal isn't faster) and the speed scales with zoom + `dt`, so movement is
    /// continuous and frame-rate independent rather than one jerky step per keypress.
    fn pan_held(&mut self, dt: f32) {
        let rx = (self.keys[3] as i32 - self.keys[1] as i32) as f32; // d - a
        let fy = (self.keys[0] as i32 - self.keys[2] as i32) as f32; // w - s
        if rx == 0.0 && fy == 0.0 {
            return;
        }
        let inv = 1.0 / (rx * rx + fy * fy).sqrt();
        let (right, fwd) = crate::math::pan_basis(self.cam.yaw);
        let step = 1.4 * self.cam.zoom * dt * inv;
        self.cam.cx += (right.0 * rx + fwd.0 * fy) * step;
        self.cam.cy += (right.1 * rx + fwd.1 * fy) * step;
    }

    /// Enter first-person "drop in" mode: stand at the current god-view centre,
    /// looking out along the god-view yaw at a gentle slightly-downward tilt — so
    /// you drop in exactly where you were looking. Render+input only; the iso
    /// camera fields are preserved so Esc restores the god-view exactly.
    fn enter_fp(&mut self) {
        if self.cam.is_fp() {
            return;
        }
        self.cam.fp = Some(view::FpView {
            eye_x: self.cam.cx,
            eye_z: self.cam.cy,
            yaw: self.cam.yaw,
            pitch: -0.12, // a touch downward so the ground reads on entry
        });
    }

    /// Exit first-person, restoring the (untouched) iso god-view.
    fn exit_fp(&mut self) {
        self.cam.fp = None;
    }

    /// Per-frame first-person WALK: WASD moves the eye along the look's GROUND
    /// plane (forward/back + strafe), Shift sprints. Diagonals are normalised so
    /// they aren't faster, and the eye-height tracks the terrain via `ground_height`
    /// at render time — so you walk over the island's hills. Sim-untouching.
    fn walk_held(&mut self, dt: f32) {
        let Some(fp) = self.cam.fp.as_mut() else { return };
        let rx = (self.keys[3] as i32 - self.keys[1] as i32) as f32; // strafe: d - a
        let fy = (self.keys[0] as i32 - self.keys[2] as i32) as f32; // forward: w - s
        if rx == 0.0 && fy == 0.0 {
            return;
        }
        let inv = 1.0 / (rx * rx + fy * fy).sqrt();
        // Walk along the GROUND-projected look direction (ignore pitch) so looking
        // up/down never changes your pace, and forward/right are level.
        let (forward, right) = crate::math::fp_basis(fp.yaw, 0.0);
        let pace = if self.run { 14.0 } else { 6.0 }; // sim cells / second
        let step = pace * dt * inv;
        fp.eye_x += (forward.x * fy + right.x * rx) * step;
        fp.eye_z += (forward.z * fy + right.z * rx) * step;
        // Keep the walker on the island (clamp to the world bounds with a margin).
        let (w, h) = (self.world.w as f32, self.world.h as f32);
        fp.eye_x = fp.eye_x.clamp(0.5, w - 1.5);
        fp.eye_z = fp.eye_z.clamp(0.5, h - 1.5);
    }

    /// Apply a raw mouse delta to the first-person look (yaw/pitch). Pitch is
    /// clamped to ±85° so you can't flip over. Sensitivity is in radians per pixel.
    fn look(&mut self, dx: f32, dy: f32) {
        let Some(fp) = self.cam.fp.as_mut() else { return };
        const SENS: f32 = 0.0024; // radians per pixel — reasonable, tunable
        const PITCH_LIMIT: f32 = 85.0 * std::f32::consts::PI / 180.0;
        fp.yaw += dx * SENS;
        fp.pitch = (fp.pitch - dy * SENS).clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }

    /// Advance the simulation on a fixed cognitive tick, scaled by speed.
    fn update(&mut self, dt: f32) {
        // camera movement runs every frame, even while paused. In first-person the
        // WASD keys WALK the eye; in the god-view they pan the framed centre.
        if self.cam.is_fp() {
            self.walk_held(dt);
        } else {
            self.pan_held(dt);
        }
        if let Some(ev) = self.evolve.as_mut() {
            ev.world.animate(dt);
            if self.hud.paused {
                return;
            }
            // MAX SPEED: this is a fast-forward evolution view, not a gentle watch.
            // Step many cognitive ticks per frame so generations advance in seconds.
            for _ in 0..256 {
                ev.tick();
            }
            return;
        }
        self.world.animate(dt);
        if self.hud.paused {
            return;
        }
        let rate = 2.5 * self.hud.speed; // cognitive ticks / second
        self.acc += dt;
        let step = 1.0 / rate;
        let mut budget = 8;
        while self.acc >= step && budget > 0 {
            self.world.step();
            self.acc -= step;
            budget -= 1;
        }
    }
}

struct App {
    gfx: Option<Gfx>,
    game: Game,
    start: Instant,
    last: Instant,
    #[allow(dead_code)]
    proxy: EventLoopProxy<AppEvent>,
    init_started: bool,
}

impl App {
    fn new(proxy: EventLoopProxy<AppEvent>, evolve_pop: Option<usize>) -> Self {
        let now = Instant::now();
        let mut game = match evolve_pop {
            Some(pop) => Game::evolve(pop),
            None => Game::new(),
        };
        // SELF-VERIFY HOOK: start in first-person at a fixed eye/look so a headless
        // screenshot can confirm the walk-through framing without interactive input
        // (the live V-toggle is the real entry point). Native: env `DAIMON_FP`; web:
        // url `?fp=1`. A no-op unless the flag is present.
        if let Some(fp) = fp_debug_start(&game) {
            game.cam.fp = Some(fp);
        }
        Self {
            gfx: None,
            game,
            start: now,
            last: now,
            proxy,
            init_started: false,
        }
    }

    fn create_window(&self, event_loop: &ActiveEventLoop) -> Arc<Window> {
        let attrs = Window::default_attributes()
            .with_title("Daimon: Smallworld")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 800.0));
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));

        #[cfg(target_arch = "wasm32")]
        {
            use winit::dpi::PhysicalSize;
            use winit::platform::web::WindowExtWebSys;
            if let (Some(canvas), Some(web_win)) = (window.canvas(), web_sys::window()) {
                if let Some(body) = web_win.document().and_then(|d| d.body()) {
                    let _ = body.append_child(&canvas);
                }
                // Size the drawing buffer to the viewport × device-pixel-ratio.
                // Without this winit's canvas comes up tiny and the scene renders
                // off-screen (a black page).
                let dpr = web_win.device_pixel_ratio().max(1.0);
                let vw = web_win.inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(1280.0);
                let vh = web_win.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(800.0);
                let pw = (vw * dpr).max(1.0) as u32;
                let ph = (vh * dpr).max(1.0) as u32;
                let _ = window.request_inner_size(PhysicalSize::new(pw, ph));
                log::warn!("daimon: viewport {vw}x{vh} dpr {dpr} -> backing {pw}x{ph}");
            }
        }
        window
    }
}

impl ApplicationHandler<AppEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gfx.is_some() || self.init_started {
            return;
        }
        self.init_started = true;
        let window = self.create_window(event_loop);

        #[cfg(not(target_arch = "wasm32"))]
        {
            let gfx = pollster::block_on(Gfx::new(window.clone()));
            window.request_redraw();
            self.gfx = Some(gfx);
        }
        #[cfg(target_arch = "wasm32")]
        {
            let proxy = self.proxy.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let gfx = Gfx::new(window).await;
                let _ = proxy.send_event(AppEvent::GfxReady(gfx));
            });
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: AppEvent) {
        let AppEvent::GfxReady(gfx) = event;
        gfx.window().request_redraw();
        self.gfx = Some(gfx);
    }

    /// Raw (unaccelerated, un-clamped) mouse motion — the first-person MOUSE-LOOK
    /// source. winit reports this on native AND web (under pointer-lock), so the
    /// same path turns the head everywhere; in the god-view it's ignored.
    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        if let winit::event::DeviceEvent::MouseMotion { delta } = event {
            if self.game.cam.is_fp() {
                self.game.look(delta.0 as f32, delta.1 as f32);
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(gfx) = self.gfx.as_mut() else { return };
        let (sw, sh) = gfx.size();
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => gfx.resize(size.width, size.height),

            WindowEvent::CursorMoved { position, .. } => {
                let new = (position.x as f32, position.y as f32);
                // First-person turns with raw mouse motion (DeviceEvent::MouseMotion),
                // not absolute cursor drag — so the ground-plane pan is god-view only.
                if self.game.mouse_down && !self.game.cam.is_fp() {
                    let dx = new.0 - self.game.mouse.0;
                    let dy = new.1 - self.game.mouse.1;
                    self.game.drag_dist += (dx * dx + dy * dy).sqrt();
                    // Pan along the ground plane so the world tracks the cursor.
                    let (right, fwd) = crate::math::pan_basis(self.game.cam.yaw);
                    let k = 2.0 * self.game.cam.zoom / sh;
                    self.game.cam.cx += (-right.0 * dx + fwd.0 * dy) * k;
                    self.game.cam.cy += (-right.1 * dx + fwd.1 * dy) * k;
                }
                self.game.mouse = new;
            }

            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => match state {
                ElementState::Pressed => {
                    self.game.mouse_down = true;
                    self.game.drag_dist = 0.0;
                }
                ElementState::Released => {
                    self.game.mouse_down = false;
                    // Click-to-select is a god-view affordance; in first-person the
                    // pointer is locked for look, so skip picking.
                    if self.game.drag_dist < 6.0 && !self.game.cam.is_fp() {
                        let (wx, wy) = self.game.cam.pick(
                            self.game.mouse.0,
                            self.game.mouse.1,
                            sw,
                            sh,
                            self.game.world.w,
                            self.game.world.h,
                        );
                        self.game.selected = self.game.world.pick_agent(wx, wy, 1.3);
                    }
                }
            },

            WindowEvent::MouseWheel { delta, .. } if !self.game.cam.is_fp() => {
                let amt = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 * 0.02,
                };
                // Zoom toward the CURSOR: capture the world point under the mouse,
                // change the zoom, then pan so that same point is back under the
                // cursor. scroll up (amt > 0) zooms IN → smaller half-extent. Range
                // widened so you can pull right out past the whole island.
                let (w, h) = (self.game.world.w, self.game.world.h);
                let before = self.game.cam.pick(self.game.mouse.0, self.game.mouse.1, sw, sh, w, h);
                let factor = (1.0 - amt * 0.12).clamp(0.6, 1.5);
                self.game.cam.zoom = (self.game.cam.zoom * factor).clamp(4.0, 95.0);
                let after = self.game.cam.pick(self.game.mouse.0, self.game.mouse.1, sw, sh, w, h);
                self.game.cam.cx += before.0 - after.0;
                self.game.cam.cy += before.1 - after.1;
            }

            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                match event.logical_key.as_ref() {
                    // held WASD movement → smooth per-frame pan (set on press AND release)
                    Key::Character("w") | Key::Character("W") => self.game.keys[0] = pressed,
                    Key::Character("a") | Key::Character("A") => self.game.keys[1] = pressed,
                    Key::Character("s") | Key::Character("S") => self.game.keys[2] = pressed,
                    Key::Character("d") | Key::Character("D") => self.game.keys[3] = pressed,
                    // Shift = sprint while walking in first-person (set on press+release)
                    Key::Named(NamedKey::Shift) => self.game.run = pressed,
                    // V = toggle first-person "drop in" mode. Drops the eye at the
                    // current god-view centre; on the web grabs pointer-lock for
                    // mouse-look, releasing it again on exit.
                    Key::Character("v") | Key::Character("V") if pressed => {
                        if self.game.cam.is_fp() {
                            self.game.exit_fp();
                            release_pointer_lock();
                        } else {
                            self.game.enter_fp();
                            request_pointer_lock(gfx.window());
                        }
                    }
                    // one-shot toggles (on key-down only)
                    Key::Named(NamedKey::Space) if pressed => self.game.hud.paused = !self.game.hud.paused,
                    // Esc: leave first-person if in it (back to the god-view); else
                    // clear the selection as before.
                    Key::Named(NamedKey::Escape) if pressed => {
                        if self.game.cam.is_fp() {
                            self.game.exit_fp();
                            release_pointer_lock();
                        } else {
                            self.game.selected = None;
                        }
                    }
                    Key::Named(NamedKey::Tab) if pressed => {
                        let n = self.game.world.agents.len();
                        if n > 0 {
                            self.game.selected =
                                Some(self.game.selected.map(|i| (i + 1) % n).unwrap_or(0));
                        }
                    }
                    Key::Character("]") if pressed => {
                        self.game.hud.speed = (self.game.hud.speed + 1.0).min(8.0)
                    }
                    Key::Character("[") if pressed => {
                        self.game.hud.speed = (self.game.hud.speed - 1.0).max(1.0)
                    }
                    // feed-at-cursor is a god-view affordance (it needs the cursor
                    // ground-pick); ignored in first-person.
                    Key::Character("f") | Key::Character("F") if pressed && !self.game.cam.is_fp() => {
                        let (wx, wy) = self.game.cam.pick(
                            self.game.mouse.0,
                            self.game.mouse.1,
                            sw,
                            sh,
                            self.game.world.w,
                            self.game.world.h,
                        );
                        self.game.world.feed(wx, wy);
                    }
                    Key::Character("q") | Key::Character("Q") if pressed => {
                        // toggle quantum-cognitive decision mode across the village
                        self.game.hud.quantum = !self.game.hud.quantum;
                        let q = self.game.hud.quantum;
                        for a in &mut self.game.world.agents {
                            a.mind.set_quantum(q);
                        }
                    }
                    _ => {}
                }
            }

            WindowEvent::RedrawRequested => {
                // On the web, keep the surface matched to the live viewport every
                // frame — Resized isn't always delivered, and a 0-size surface is
                // exactly a black screen.
                #[cfg(target_arch = "wasm32")]
                if let Some(web_win) = web_sys::window() {
                    let dpr = web_win.device_pixel_ratio().max(1.0);
                    let vw = web_win.inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(1280.0);
                    let vh = web_win.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(800.0);
                    let pw = (vw * dpr).max(1.0) as u32;
                    let ph = (vh * dpr).max(1.0) as u32;
                    let (cw, ch) = gfx.size();
                    if cw as u32 != pw || ch as u32 != ph {
                        gfx.resize(pw, ph);
                    }
                }
                let (sw, sh) = gfx.size();

                // HUD scale is screen-relative: the world fills the full backing
                // buffer (which already includes the device-pixel-ratio), so we
                // size HUD chrome as a constant fraction of the buffer height —
                // the inspector/top bar stay legible on Retina and large displays
                // alike, regardless of what dpr the browser reports.
                let ui_scale = (sh / 760.0).clamp(1.0, 3.5);

                let now = Instant::now();
                let dt = (now - self.last).as_secs_f32().min(0.05);
                self.last = now;
                self.game.update(dt);

                let t = self.start.elapsed().as_secs_f32();
                let evo = self.game.evolve.as_ref().map(|ev| view::EvoHud {
                    generation: ev.generation,
                    alive: ev.alive(),
                    pop: ev.pop,
                    cycle: ev.cycle(),
                    last: ev.last,
                });
                // Fully user-driven camera: WASD pan, Q/E rotate, scroll-zoom to the
                // cursor. No auto-orbit, so manual control never fights a drift.
                let scene = view::build_with(
                    self.game.view_world(),
                    &self.game.cam,
                    self.game.selected,
                    &self.game.hud,
                    evo.as_ref(),
                    sw,
                    sh,
                    t,
                    ui_scale,
                );
                gfx.render(&scene, t);
                gfx.window().request_redraw();
            }
            _ => {}
        }
    }
}

/// Self-verify hook: if the FP debug flag is set, return a fixed first-person
/// eye/look aimed across the island toward the village heart, so a headless shot
/// can inspect the walk-through framing. Native reads env `DAIMON_FP`; web reads
/// the url query `?fp=1`. Returns `None` (normal god-view start) when absent.
fn fp_debug_start(game: &Game) -> Option<view::FpView> {
    let on = {
        #[cfg(not(target_arch = "wasm32"))]
        {
            std::env::var("DAIMON_FP").map(|v| v != "0" && !v.is_empty()).unwrap_or(false)
        }
        #[cfg(target_arch = "wasm32")]
        {
            web_sys::window()
                .and_then(|w| w.location().search().ok())
                .map(|q: String| q.contains("fp=1") || q.contains("fp=true"))
                .unwrap_or(false)
        }
    };
    if !on {
        return None;
    }
    // Stand among the village (a third of the way in) looking across the heart —
    // so the shot reads as a ground-level view with buildings/minds/trees filling
    // the frame, not mostly empty grass.
    let (w, h) = (game.world.w as f32, game.world.h as f32);
    let eye_x = w * 0.34;
    let eye_z = h * 0.46;
    let cx = w * 0.62;
    let cz = h * 0.52;
    let yaw = (cz - eye_z).atan2(cx - eye_x); // aim across the heart
    Some(view::FpView { eye_x, eye_z, yaw, pitch: -0.04 })
}

/// Grab the mouse for first-person look. On the WEB this calls the canvas's
/// `requestPointerLock()` so the cursor hides and raw `movementX/Y` flows in via
/// winit's `DeviceEvent::MouseMotion`. On native, winit already delivers raw
/// motion, so this is a no-op.
fn request_pointer_lock(_window: &Arc<Window>) {
    #[cfg(target_arch = "wasm32")]
    {
        use winit::platform::web::WindowExtWebSys;
        if let Some(canvas) = _window.canvas() {
            canvas.request_pointer_lock();
        }
    }
}

/// Release the mouse when leaving first-person (web `exitPointerLock`); no-op native.
fn release_pointer_lock() {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
            doc.exit_pointer_lock();
        }
    }
}

/// Parse `--evolve [--pop N]` from CLI args. Returns `Some(pop)` if the live
/// generational evolution mode was requested (default pop 1000), else `None` for
/// the normal village. Web has no args, so this is native-only in effect.
#[cfg(not(target_arch = "wasm32"))]
fn parse_evolve_args() -> Option<usize> {
    let args: Vec<String> = std::env::args().collect();
    if !args.iter().any(|a| a == "--evolve") {
        return None;
    }
    let mut pop = 1000usize;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--pop" {
            if let Some(n) = it.next().and_then(|s| s.parse::<usize>().ok()) {
                pop = n.max(2);
            }
        }
    }
    Some(pop)
}

/// Native + web entry point.
pub fn run() {
    #[cfg(not(target_arch = "wasm32"))]
    let evolve_pop = parse_evolve_args();
    #[cfg(target_arch = "wasm32")]
    let evolve_pop: Option<usize> = None;

    let event_loop = EventLoop::<AppEvent>::with_user_event()
        .build()
        .expect("event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let app = App::new(event_loop.create_proxy(), evolve_pop);

    #[cfg(not(target_arch = "wasm32"))]
    {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
        let mut app = app;
        event_loop.run_app(&mut app).expect("run app");
    }
    #[cfg(target_arch = "wasm32")]
    {
        use winit::platform::web::EventLoopExtWebSys;
        event_loop.spawn_app(app);
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn web_main() {
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Warn);
    run();
}
