//! Render real frames of Daimon: Smallworld to PNGs — no window required.
//! Exercises the exact 3-D world pipeline + low-res blit + glyphon HUD the game
//! uses, so we can see (and regression-check) the look without a display. Emits a
//! spread of conditions — dawn/noon/dusk/night across the seasons + weather — so
//! the day/night, season and weather systems can all be inspected at once (the
//! "virtual David" critique loop reads these).
//!
//!   cargo run -p daimon-game --example headless --release

use daimon_game::gfx::Renderer;
use daimon_game::sim::GameWorld;
use daimon_game::view::{self, Camera, Hud};

const W: u32 = 1280;
const H: u32 = 800;

struct Shot {
    file: &'static str,
    day: f32,
    season: f32,
    weather: f32,
    weather_kind: f32,
}

fn main() {
    pollster::block_on(run());
}

async fn run() {
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        })
        .await
        .expect("adapter");
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("headless"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            ..Default::default()
        })
        .await
        .expect("device");

    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("target"),
        size: wgpu::Extent3d { width: W, height: H, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let tview = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut renderer = Renderer::new(&device, &queue, format);

    let mut world = GameWorld::with_genome(0xDA13, 6, &daimon_mind::Genome::showcase());
    for _ in 0..260 {
        world.step();
        for _ in 0..3 {
            world.animate(0.05);
        }
    }
    let cam = Camera::new(world.w as f32 * 0.5, world.h as f32 * 0.5);
    let hud = Hud { paused: false, speed: 3.0, quantum: false };

    let shots = [
        Shot { file: "/tmp/daimon_dawn_spring.png", day: 0.28, season: 0.05, weather: 0.0, weather_kind: 0.0 },
        Shot { file: "/tmp/daimon_noon_summer.png", day: 0.50, season: 0.28, weather: 0.0, weather_kind: 0.0 },
        Shot { file: "/tmp/daimon_dusk_autumn.png", day: 0.74, season: 0.52, weather: 0.45, weather_kind: 0.0 },
        Shot { file: "/tmp/daimon_night_winter.png", day: 0.02, season: 0.80, weather: 0.7, weather_kind: 1.0 },
        Shot { file: "/tmp/daimon_frame.png", day: 0.50, season: 0.28, weather: 0.0, weather_kind: 0.0 },
    ];

    for shot in &shots {
        world.day = shot.day;
        world.tick = (shot.season * 6000.0) as u64;
        let mut scene = view::build(&world, &cam, Some(2), &hud, W as f32, H as f32, 12.0, 1.0);
        // Force the exact weather for the still (climate is otherwise time-driven).
        scene.sky.weather = shot.weather;
        scene.sky.weather_kind = shot.weather_kind;
        render_png(&device, &queue, &mut renderer, &texture, &tview, &scene, shot.file).await;
    }
}

async fn render_png(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut Renderer,
    texture: &wgpu::Texture,
    tview: &wgpu::TextureView,
    scene: &daimon_game::scene::Scene,
    path: &str,
) {
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("enc") });
    renderer.encode(device, queue, &mut encoder, tview, W, H, scene, 12.0);

    let bytes_per_row = W * 4;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (bytes_per_row * H) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(H),
            },
        },
        wgpu::Extent3d { width: W, height: H, depth_or_array_layers: 1 },
    );
    queue.submit(Some(encoder.finish()));

    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    let data = slice.get_mapped_range().to_vec();

    let file = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), W, H);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&data).unwrap();
    println!("wrote {path}");
}
