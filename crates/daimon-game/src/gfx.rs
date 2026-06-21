//! GPU context (`Gfx`, surface-bound) and the `Renderer`.
//!
//! Daimon renders its village as **real 3-D isometric geometry** into a LOW-RES
//! offscreen target — terrain + actors lit per-fragment off screen-space
//! derivative normals (the faceted low-poly look is free), a moving water plane,
//! and additive glows for the minds' moods. That target is then upscaled NEAREST
//! onto the swapchain for a cohesive, painterly pixel look. The **HUD** (the mind
//! inspector + top bar) draws crisp at full resolution on top, via the original
//! SDF quad pipeline + glyphon text. Runs windowed and headless (offscreen PNG).

use std::sync::Arc;
use winit::window::Window;

use wgpu::util::DeviceExt;

use crate::geo::{AddVertex, LitVertex};
use crate::scene::{Quad, Scene};

/// HUD uniform (the SDF pipeline): screen size + time.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct HudU {
    screen: [f32; 2],
    time: f32,
    _pad: f32,
}

/// The world uniform block (matches `world.wgsl`'s `U`).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct WorldU {
    view_proj: [f32; 16],
    cam_pos_time: [f32; 4],
    sun_dir: [f32; 4],
    sun_color_strength: [f32; 4],
    ambient: [f32; 4],
    horizon_fogfar: [f32; 4],
    season_tint: [f32; 4],
    misc: [f32; 4],
}

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
/// The pixel-art internal resolution: a fixed line count, width follows aspect.
const LOW_H: u32 = 360;
const MAX_DYN_LIT: usize = 131_072;
const MAX_ADD: usize = 16_384;

fn low_res_size(w: u32, h: u32) -> (u32, u32) {
    let aspect = w.max(1) as f32 / h.max(1) as f32;
    let lw = ((LOW_H as f32 * aspect).round() as u32).clamp(160, 1280);
    (lw, LOW_H)
}

/// The format-agnostic renderer: world (3-D, low-res RT + blit) + HUD (SDF + text).
pub struct Renderer {
    format: wgpu::TextureFormat,
    // --- world ---
    world_uniform: wgpu::Buffer,
    world_bg: wgpu::BindGroup,
    lit_pipeline: wgpu::RenderPipeline,
    water_pipeline: wgpu::RenderPipeline,
    add_pipeline: wgpu::RenderPipeline,
    terrain: Option<(wgpu::Buffer, u32)>,
    terrain_key: (i32, i32),
    water_vbuf: wgpu::Buffer,
    water_len: u32,
    dyn_lit: wgpu::Buffer,
    add_vbuf: wgpu::Buffer,
    // --- low-res RT + nearest blit ---
    rt_view: wgpu::TextureView,
    rt_depth: wgpu::TextureView,
    rt_size: (u32, u32),
    blit_bgl: wgpu::BindGroupLayout,
    blit_bind: wgpu::BindGroup,
    blit_sampler: wgpu::Sampler,
    blit_pipeline: wgpu::RenderPipeline,
    // --- HUD ---
    hud_pipeline: wgpu::RenderPipeline,
    hud_uniform: wgpu::Buffer,
    hud_bg: wgpu::BindGroup,
    instances: wgpu::Buffer,
    instance_cap: usize,
    font_system: glyphon::FontSystem,
    swash: glyphon::SwashCache,
    viewport: glyphon::Viewport,
    atlas: glyphon::TextAtlas,
    text_renderer: glyphon::TextRenderer,
}

impl Renderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        // ---------- world ----------
        let world_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("world-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("world.wgsl").into()),
        });
        let world_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("world-uniforms"),
            size: std::mem::size_of::<WorldU>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let world_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("world-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let world_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("world-bg"),
            layout: &world_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: world_uniform.as_entire_binding() }],
        });
        let world_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("world-layout"),
            bind_group_layouts: &[Some(&world_bgl)],
            immediate_size: 0,
        });
        let lit_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<LitVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4],
        };
        let add_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<AddVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4, 2 => Float32x2],
        };
        let make = |label: &str,
                    vs: &str,
                    fs: &str,
                    buffers: &[wgpu::VertexBufferLayout],
                    blend: wgpu::BlendState,
                    depth_write: bool|
         -> wgpu::RenderPipeline {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&world_layout),
                vertex: wgpu::VertexState {
                    module: &world_shader,
                    entry_point: Some(vs),
                    buffers,
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &world_shader,
                    entry_point: Some(fs),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(blend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(depth_write),
                    depth_compare: Some(if depth_write {
                        wgpu::CompareFunction::Less
                    } else {
                        wgpu::CompareFunction::LessEqual
                    }),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let additive = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let lit_pipeline = make(
            "lit",
            "vs_lit",
            "fs_lit",
            std::slice::from_ref(&lit_layout),
            wgpu::BlendState::ALPHA_BLENDING,
            true,
        );
        let water_pipeline = make(
            "water",
            "vs_lit",
            "fs_water",
            std::slice::from_ref(&lit_layout),
            wgpu::BlendState::ALPHA_BLENDING,
            false,
        );
        let add_pipeline = make("add", "vs_add", "fs_add", &[add_layout], additive, false);

        let water = crate::geo::build_water(400.0);
        let water_len = water.len() as u32;
        let water_vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("water"),
            contents: bytemuck::cast_slice(&water),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let dyn_lit = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dyn-lit"),
            size: (MAX_DYN_LIT * std::mem::size_of::<LitVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let add_vbuf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("add"),
            size: (MAX_ADD * std::mem::size_of::<AddVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ---------- blit ----------
        let blit_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        // The post/blit pass also reads the world uniform (sun direction + daylight)
        // so it can place god-rays and tune the colour grade with the time-of-day.
        let blit_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blit-layout"),
            bind_group_layouts: &[Some(&blit_bgl), Some(&world_bgl)],
            immediate_size: 0,
        });
        let blit_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blit"),
            layout: Some(&blit_layout),
            vertex: wgpu::VertexState {
                module: &world_shader,
                entry_point: Some("vs_blit"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &world_shader,
                entry_point: Some("fs_blit"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        // Linear sampling so the post pass can take smooth blurred bloom taps off
        // the low-res RT (and the upscale gains a soft painterly edge rather than
        // hard stair-steps — reads more cinematic than chunky here).
        let blit_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("blit-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let rt_size = (640, LOW_H);
        let (rt_view, rt_depth) = make_rt(device, format, rt_size);
        let blit_bind = make_blit_bind(device, &blit_bgl, &rt_view, &blit_sampler);

        // ---------- HUD (SDF quads) ----------
        let hud_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hud-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let hud_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hud-uniforms"),
            size: std::mem::size_of::<HudU>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let hud_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hud-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let hud_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hud-bg"),
            layout: &hud_bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: hud_uniform.as_entire_binding() }],
        });
        let hud_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hud-layout"),
            bind_group_layouts: &[Some(&hud_bgl)],
            immediate_size: 0,
        });
        let quad_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Quad>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4],
        };
        let hud_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("hud-pipeline"),
            layout: Some(&hud_layout),
            vertex: wgpu::VertexState {
                module: &hud_shader,
                entry_point: Some("vs"),
                buffers: &[quad_layout],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &hud_shader,
                entry_point: Some("fs"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let instance_cap = 8192;
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instances"),
            size: (instance_cap * std::mem::size_of::<Quad>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let font_data = include_bytes!("../assets/Inter-Regular.ttf").to_vec();
        let source = glyphon::fontdb::Source::Binary(std::sync::Arc::new(font_data));
        let mut font_system = glyphon::FontSystem::new_with_fonts(std::iter::once(source));
        let family = font_system
            .db_mut()
            .faces()
            .next()
            .and_then(|f| f.families.first().map(|(name, _)| name.clone()));
        if let Some(fam) = family {
            let db = font_system.db_mut();
            db.set_sans_serif_family(fam.clone());
            db.set_serif_family(fam.clone());
            db.set_monospace_family(fam.clone());
            db.set_cursive_family(fam.clone());
            db.set_fantasy_family(fam);
        }
        let swash = glyphon::SwashCache::new();
        let cache = glyphon::Cache::new(device);
        let viewport = glyphon::Viewport::new(device, &cache);
        let mut atlas = glyphon::TextAtlas::new(device, queue, &cache, format);
        let text_renderer =
            glyphon::TextRenderer::new(&mut atlas, device, wgpu::MultisampleState::default(), None);

        Self {
            format,
            world_uniform,
            world_bg,
            lit_pipeline,
            water_pipeline,
            add_pipeline,
            terrain: None,
            terrain_key: (0, 0),
            water_vbuf,
            water_len,
            dyn_lit,
            add_vbuf,
            rt_view,
            rt_depth,
            rt_size,
            blit_bgl,
            blit_bind,
            blit_sampler,
            blit_pipeline,
            hud_pipeline,
            hud_uniform,
            hud_bg,
            instances,
            instance_cap,
            font_system,
            swash,
            viewport,
            atlas,
            text_renderer,
        }
    }

    fn ensure_instances(&mut self, device: &wgpu::Device, n: usize) {
        if n <= self.instance_cap {
            return;
        }
        let cap = n.next_power_of_two();
        self.instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instances"),
            size: (cap * std::mem::size_of::<Quad>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.instance_cap = cap;
    }

    fn ensure_terrain(&mut self, device: &wgpu::Device, scene: &Scene) {
        let key = scene.world_dims;
        if self.terrain.is_some() && self.terrain_key == key {
            return;
        }
        let verts = crate::geo::build_terrain(key.0, key.1, 56);
        let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("terrain"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        self.terrain = Some((buf, verts.len() as u32));
        self.terrain_key = key;
    }

    fn ensure_rt(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let want = low_res_size(width, height);
        if want == self.rt_size {
            return;
        }
        self.rt_size = want;
        let (v, d) = make_rt(device, self.format, want);
        self.rt_view = v;
        self.rt_depth = d;
        self.blit_bind = make_blit_bind(device, &self.blit_bgl, &self.rt_view, &self.blit_sampler);
    }

    /// Record one full frame into `encoder`, targeting `view` (swapchain or PNG).
    #[allow(clippy::too_many_arguments)]
    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        width: u32,
        height: u32,
        scene: &Scene,
        time: f32,
    ) {
        self.ensure_rt(device, width, height);
        self.ensure_terrain(device, scene);

        let sky = &scene.sky;
        let wu = WorldU {
            view_proj: sky.view_proj,
            cam_pos_time: [sky.cam_pos[0], sky.cam_pos[1], sky.cam_pos[2], time],
            sun_dir: [sky.sun_dir[0], sky.sun_dir[1], sky.sun_dir[2], sky.daylight],
            sun_color_strength: [sky.sun_color[0], sky.sun_color[1], sky.sun_color[2], sky.sun_strength],
            ambient: [sky.ambient[0], sky.ambient[1], sky.ambient[2], 0.0],
            horizon_fogfar: [sky.horizon[0], sky.horizon[1], sky.horizon[2], sky.fog_far],
            season_tint: sky.season_tint,
            misc: [sky.weather, sky.weather_kind, sky.fog_target[0], sky.fog_target[1]],
        };
        queue.write_buffer(&self.world_uniform, 0, bytemuck::bytes_of(&wu));

        let lit_n = scene.lit.len().min(MAX_DYN_LIT);
        if lit_n > 0 {
            queue.write_buffer(&self.dyn_lit, 0, bytemuck::cast_slice(&scene.lit[..lit_n]));
        }
        let add_n = scene.add.len().min(MAX_ADD);
        if add_n > 0 {
            queue.write_buffer(&self.add_vbuf, 0, bytemuck::cast_slice(&scene.add[..add_n]));
        }

        // ---- pass 1: the 3-D world into the low-res RT ----
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("world"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.rt_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: sky.horizon[0] as f64,
                            g: sky.horizon[1] as f64,
                            b: sky.horizon[2] as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.rt_depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.world_bg, &[]);
            pass.set_pipeline(&self.lit_pipeline);
            if let Some((buf, len)) = &self.terrain {
                pass.set_vertex_buffer(0, buf.slice(..));
                pass.draw(0..*len, 0..1);
            }
            if lit_n > 0 {
                pass.set_vertex_buffer(0, self.dyn_lit.slice(..));
                pass.draw(0..lit_n as u32, 0..1);
            }
            pass.set_pipeline(&self.water_pipeline);
            pass.set_vertex_buffer(0, self.water_vbuf.slice(..));
            pass.draw(0..self.water_len, 0..1);
            if add_n > 0 {
                pass.set_pipeline(&self.add_pipeline);
                pass.set_vertex_buffer(0, self.add_vbuf.slice(..));
                pass.draw(0..add_n as u32, 0..1);
            }
        }

        // ---- HUD geometry + text prep (drawn in pass 2) ----
        queue.write_buffer(
            &self.hud_uniform,
            0,
            bytemuck::bytes_of(&HudU { screen: [width as f32, height as f32], time, _pad: 0.0 }),
        );
        let qn = scene.quads.len();
        if qn > 0 {
            self.ensure_instances(device, qn);
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&scene.quads));
        }
        self.viewport.update(queue, glyphon::Resolution { width, height });
        let mut buffers: Vec<glyphon::Buffer> = Vec::with_capacity(scene.texts.len());
        for t in &scene.texts {
            let mut buf = glyphon::Buffer::new(
                &mut self.font_system,
                glyphon::Metrics::new(t.size, t.size * 1.25),
            );
            buf.set_size(&mut self.font_system, t.wrap, None);
            buf.set_text(
                &mut self.font_system,
                &t.content,
                &glyphon::Attrs::new().family(glyphon::Family::SansSerif),
                glyphon::Shaping::Advanced,
                None,
            );
            buf.shape_until_scroll(&mut self.font_system, false);
            buffers.push(buf);
        }
        let areas: Vec<glyphon::TextArea> = scene
            .texts
            .iter()
            .zip(buffers.iter())
            .map(|(t, buf)| glyphon::TextArea {
                buffer: buf,
                left: t.x,
                top: t.y,
                scale: 1.0,
                bounds: glyphon::TextBounds { left: 0, top: 0, right: width as i32, bottom: height as i32 },
                default_color: glyphon::Color::rgba(t.color[0], t.color[1], t.color[2], t.color[3]),
                custom_glyphs: &[],
            })
            .collect();
        let _ = self.text_renderer.prepare(
            device,
            queue,
            &mut self.font_system,
            &mut self.atlas,
            &self.viewport,
            areas,
            &mut self.swash,
        );

        // ---- pass 2: blit the world up, then the crisp HUD on top ----
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("composite"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.blit_pipeline);
            pass.set_bind_group(0, &self.blit_bind, &[]);
            pass.set_bind_group(1, &self.world_bg, &[]);
            pass.draw(0..3, 0..1);

            if qn > 0 {
                pass.set_pipeline(&self.hud_pipeline);
                pass.set_bind_group(0, &self.hud_bg, &[]);
                pass.set_vertex_buffer(0, self.instances.slice(..));
                pass.draw(0..6, 0..qn as u32);
            }
            let _ = self.text_renderer.render(&self.atlas, &self.viewport, &mut pass);
        }
        self.atlas.trim();
    }
}

fn make_rt(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    size: (u32, u32),
) -> (wgpu::TextureView, wgpu::TextureView) {
    let extent = wgpu::Extent3d { width: size.0.max(1), height: size.1.max(1), depth_or_array_layers: 1 };
    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("rt-color"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("rt-depth"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    (
        color.create_view(&wgpu::TextureViewDescriptor::default()),
        depth.create_view(&wgpu::TextureViewDescriptor::default()),
    )
}

fn make_blit_bind(
    device: &wgpu::Device,
    bgl: &wgpu::BindGroupLayout,
    rt_view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("blit-bind"),
        layout: bgl,
        entries: &[
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(rt_view) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(sampler) },
        ],
    })
}

/// The windowed context: a surface plus a [`Renderer`].
pub struct Gfx {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
}

impl Gfx {
    pub async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);

        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let surface = instance.create_surface(window.clone()).expect("create surface");
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("request adapter");
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("daimon-device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults(),
                ..Default::default()
            })
            .await
            .expect("request device");

        let mut config = surface.get_default_config(&adapter, width, height).expect("surface config");
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&device, &config);

        let renderer = Renderer::new(&device, &queue, config.format);
        log::warn!("daimon gfx ready: {}x{} {:?}", config.width, config.height, config.format);

        Self { window, surface, device, queue, config, renderer }
    }

    pub fn window(&self) -> &Arc<Window> {
        &self.window
    }
    pub fn size(&self) -> (f32, f32) {
        (self.config.width as f32, self.config.height as f32)
    }
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn render(&mut self, scene: &Scene, time: f32) {
        use wgpu::CurrentSurfaceTexture as Cst;
        let frame = match self.surface.get_current_texture() {
            Cst::Success(f) | Cst::Suboptimal(f) => f,
            Cst::Outdated | Cst::Lost => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            _ => return,
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });
        self.renderer.encode(
            &self.device,
            &self.queue,
            &mut encoder,
            &view,
            self.config.width,
            self.config.height,
            scene,
            time,
        );
        self.queue.submit(Some(encoder.finish()));
        frame.present();
    }
}
