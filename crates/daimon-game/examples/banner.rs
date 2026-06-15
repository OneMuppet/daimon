//! Render a wide, cinematic hero banner of the living island — the real 3-D
//! world at golden hour, no HUD — for the README.
//!
//!   cargo run -p daimon-game --example banner --release   -> assets/banner.png

use daimon_game::gfx::Renderer;
use daimon_game::sim::GameWorld;
use daimon_game::view::{self, Camera, Hud};

const W: u32 = 2048; // within the downlevel 2048 texture limit; W*4 is 256-aligned
const H: u32 = 640;

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
            label: Some("banner"),
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

    // a village that has lived a while, so trails + worn paths are present.
    let mut world = GameWorld::with_genome(0xDA13, 6, &daimon_mind::Genome::showcase());
    for _ in 0..300 {
        world.step();
        for _ in 0..3 {
            world.animate(0.05);
        }
    }
    // golden hour (sun low + warm), high summer, clear sky.
    world.day = 0.70;
    world.tick = (0.28 * 6000.0) as u64;

    // fill the frame with the lush island; the coastline rides the edges.
    let mut cam = Camera::new(world.w as f32 * 0.5, world.h as f32 * 0.5);
    cam.zoom = 11.5;

    // build the world, then strip ALL HUD chrome + labels for a clean hero shot.
    let hud = Hud { paused: false, speed: 1.0, quantum: false };
    let mut scene = view::build(&world, &cam, None, &hud, W as f32, H as f32, 6.0, 1.0);
    scene.quads.clear();
    scene.texts.clear();
    scene.sky.weather = 0.0;
    scene.sky.fog_far = 80.0; // push the haze back so the whole island reads crisp

    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("enc") });
    renderer.encode(&device, &queue, &mut encoder, &tview, W, H, &scene, 6.0);

    let bpr = W * 4; // 9728 = 256 × 38, already aligned
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (bpr * H) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bpr),
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

    std::fs::create_dir_all("assets").unwrap();
    let path = "assets/banner.png";
    let file = std::fs::File::create(path).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), W, H);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&data).unwrap();
    println!("wrote {path} ({W}×{H})");
}
