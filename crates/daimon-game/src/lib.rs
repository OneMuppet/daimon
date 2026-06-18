//! Daimon: Smallworld — a wgpu game in which several real Daimon minds share a
//! world you can watch and tend. This module is the platform glue (winit +
//! wgpu) and the game loop; cognition is the published `daimon-*` crates.
//!
//! Runs natively (block-on GPU init) and on the web (async GPU init delivered
//! back through an [`EventLoopProxy`], the standard winit + wgpu wasm pattern).

pub mod evolve_mode;
pub mod fitness;
pub mod poet;
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
        // OPEN WORLD: the live village runs the seasonal year. can_provision lets the
        // minds stock the granary in the good months and draw it down through winter;
        // `world.set_open_world(true)` below turns the seasons/cold/granary on. As with
        // the other live-only genes, `Genome::showcase()` itself stays bit-identical
        // (provision off) so every AC/proof/fitness run is unchanged.
        genome.g[24] = 1.0; // can_provision on — seasonal stockpiling for winter
        // Seed 0x61 was chosen (from a sweep) for the believable mortality arc: with
        // the softened stalker, the village bonds and persists, then ~2 minutes in
        // loses one bonded member to the stalker — whom the survivors genuinely
        // grieve — and the remaining five live on. An occasional, meaningful loss,
        // never a bloodbath (verified stable at 5/6 over 18k ticks).
        let mut world = GameWorld::with_genome(0x61, 6, &genome);
        // turn the year on: seasons, winter cold, the granary and gather/store.
        world.set_open_world(true);
        // Tune the live world so death is a meaningful, occasional event — not a
        // bloodbath. The fair world's stalker already moves every other tick; we
        // ease its bite so a village bonds and persists for minutes, then now and
        // then loses someone (whom the others grieve), rather than churning.
        world.soften_stalker();
        let cam = Camera::new(world.w as f32 * 0.5, world.h as f32 * 0.5);
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

    /// Advance the simulation on a fixed cognitive tick, scaled by speed.
    fn update(&mut self, dt: f32) {
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
        let game = match evolve_pop {
            Some(pop) => Game::evolve(pop),
            None => Game::new(),
        };
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

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(gfx) = self.gfx.as_mut() else { return };
        let (sw, sh) = gfx.size();
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => gfx.resize(size.width, size.height),

            WindowEvent::CursorMoved { position, .. } => {
                let new = (position.x as f32, position.y as f32);
                if self.game.mouse_down {
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
                    if self.game.drag_dist < 6.0 {
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

            WindowEvent::MouseWheel { delta, .. } => {
                let amt = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 * 0.02,
                };
                // scroll up (amt > 0) zooms IN → smaller half-extent.
                let factor = (1.0 - amt * 0.12).clamp(0.6, 1.5);
                self.game.cam.zoom = (self.game.cam.zoom * factor).clamp(6.0, 32.0);
            }

            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match event.logical_key.as_ref() {
                    Key::Named(NamedKey::Space) => self.game.hud.paused = !self.game.hud.paused,
                    Key::Named(NamedKey::Escape) => self.game.selected = None,
                    Key::Named(NamedKey::Tab) => {
                        let n = self.game.world.agents.len();
                        if n > 0 {
                            self.game.selected =
                                Some(self.game.selected.map(|i| (i + 1) % n).unwrap_or(0));
                        }
                    }
                    Key::Character("]") => {
                        self.game.hud.speed = (self.game.hud.speed + 1.0).min(8.0)
                    }
                    Key::Character("[") => {
                        self.game.hud.speed = (self.game.hud.speed - 1.0).max(1.0)
                    }
                    Key::Character("f") | Key::Character("F") => {
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
                    Key::Character("q") | Key::Character("Q") => {
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
