//! GPU renderer (wgpu). One pipeline serves the live window (Rune shares its device and blits the
//! result into the UI) and headless recording (own device, readback to RGB for ffmpeg).
//!
//! Frame: shadow map -> scene pass at the low pixel resolution (flat shading, procedural materials,
//! corner AO, PCF shadows; colour + object id + depth) -> post pass (pixel outlines, labels).
//! Consumers upscale the small result with nearest-neighbour.

use std::collections::HashMap;

use anyhow::{Result, anyhow};
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};

use crate::camera::Proj;
use crate::font;
use crate::mesh::{Mesh, Placed, Vertex, push_model};
use crate::world::World;

const SHADOW_RES: u32 = 2048;
const COLOR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const ID: wgpu::TextureFormat = wgpu::TextureFormat::R32Uint;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// A device of our own for headless rendering (CLI recording, tests). On a machine without a GPU
/// wgpu falls back to a software Vulkan driver (lavapipe) when one is installed.
pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub adapter: String,
}

impl Gpu {
    pub fn headless() -> Result<Gpu> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .map_err(|e| anyhow!("no GPU adapter (install a Vulkan driver, e.g. mesa-vulkan-drivers for lavapipe): {e}"))?;
        let name = adapter.get_info().name;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("r3dsims"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
            experimental_features: Default::default(),
            memory_hints: Default::default(),
            trace: Default::default(),
        }))
        .map_err(|e| anyhow!("no GPU device: {e}"))?;
        Ok(Gpu { device, queue, adapter: name })
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    light_vp: [[f32; 4]; 4],
    sun_dir: [f32; 4],
    sun_color: [f32; 4],
    ambient: [f32; 4],
    ground: [f32; 4],
    sky_top: [f32; 4],
    sky_bottom: [f32; 4],
    misc: [f32; 4],
    depth: [f32; 4],
}

fn v4(v: Vec3, w: f32) -> [f32; 4] {
    [v.x, v.y, v.z, w]
}

#[derive(Default)]
struct MeshBuf {
    v: Option<wgpu::Buffer>,
    i: Option<wgpu::Buffer>,
    vcap: u64,
    icap: u64,
    n: u32,
}

impl MeshBuf {
    fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, mesh: &Mesh) {
        self.n = mesh.idx.len() as u32;
        if self.n == 0 {
            return;
        }
        let (vb, ib) = ((mesh.verts.len() * std::mem::size_of::<Vertex>()) as u64, (mesh.idx.len() * 4) as u64);
        if self.v.is_none() || vb > self.vcap {
            self.vcap = vb.next_power_of_two().max(1 << 16);
            self.v = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("vertices"),
                size: self.vcap,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        if self.i.is_none() || ib > self.icap {
            self.icap = ib.next_power_of_two().max(1 << 16);
            self.i = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("indices"),
                size: self.icap,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        queue.write_buffer(self.v.as_ref().unwrap(), 0, bytemuck::cast_slice(&mesh.verts));
        queue.write_buffer(self.i.as_ref().unwrap(), 0, bytemuck::cast_slice(&mesh.idx));
    }

    fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.n == 0 {
            return;
        }
        pass.set_vertex_buffer(0, self.v.as_ref().unwrap().slice(..));
        pass.set_index_buffer(self.i.as_ref().unwrap().slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.n, 0, 0..1);
    }
}

struct Targets {
    _color: wgpu::Texture,
    color_v: wgpu::TextureView,
    _id: wgpu::Texture,
    id_v: wgpu::TextureView,
    _depth: wgpu::Texture,
    depth_v: wgpu::TextureView,
    final_tex: wgpu::Texture,
    final_v: wgpu::TextureView,
    overlay_tex: wgpu::Texture,
    post_bg: wgpu::BindGroup,
    readback: wgpu::Buffer,
    padded_row: u32,
}

pub struct SceneRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    w: u32,
    h: u32,
    globals: wgpu::Buffer,
    _shadow_tex: wgpu::Texture,
    shadow_v: wgpu::TextureView,
    g0: wgpu::BindGroup,
    g0_shadow: wgpu::BindGroup,
    bgl_post: wgpu::BindGroupLayout,
    p_shadow: wgpu::RenderPipeline,
    p_scene: wgpu::RenderPipeline,
    p_sky: wgpu::RenderPipeline,
    p_post: wgpu::RenderPipeline,
    t: Targets,
    overlay: Vec<u8>,
    overlay_dirty: bool,
    terrain: MeshBuf,
    terrain_key: usize,
    dynamic: MeshBuf,
    scratch: Mesh,
    mats: Vec<Mat4>,
}

fn texture(device: &wgpu::Device, label: &str, w: u32, h: u32, format: wgpu::TextureFormat, usage: wgpu::TextureUsages) -> (wgpu::Texture, wgpu::TextureView) {
    let t = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    });
    let v = t.create_view(&Default::default());
    (t, v)
}

fn entry(binding: u32, visibility: wgpu::ShaderStages, ty: wgpu::BindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry { binding, visibility, ty, count: None }
}

fn tex_ty(sample_type: wgpu::TextureSampleType) -> wgpu::BindingType {
    wgpu::BindingType::Texture { sample_type, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }
}

const ATTRS: [wgpu::VertexAttribute; 8] = wgpu::vertex_attr_array![
    0 => Float32x3, 1 => Float32, 2 => Float32x3, 3 => Uint32, 4 => Float32x3, 5 => Uint32, 6 => Float32x2, 7 => Uint32
];

fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Vertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &ATTRS }
}

fn color_target(format: wgpu::TextureFormat) -> Option<wgpu::ColorTargetState> {
    Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })
}

impl SceneRenderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, w: u32, h: u32) -> SceneRenderer {
        let (device, queue) = (device.clone(), queue.clone());
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("r3dsims"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let vf = wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT;
        let frag = wgpu::ShaderStages::FRAGMENT;
        let uniform = wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None };
        let bgl0 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[
                entry(0, vf, uniform),
                entry(1, frag, tex_ty(wgpu::TextureSampleType::Depth)),
                entry(2, frag, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison)),
            ],
        });
        let bgl0_shadow = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals (shadow pass)"),
            entries: &[entry(0, wgpu::ShaderStages::VERTEX, uniform)],
        });
        let bgl_post = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("post"),
            entries: &[
                entry(0, frag, tex_ty(wgpu::TextureSampleType::Float { filterable: false })),
                entry(1, frag, tex_ty(wgpu::TextureSampleType::Uint)),
                entry(2, frag, tex_ty(wgpu::TextureSampleType::Depth)),
                entry(3, frag, tex_ty(wgpu::TextureSampleType::Float { filterable: false })),
            ],
        });
        let layout = |label: &str, bgls: &[Option<&wgpu::BindGroupLayout>]| {
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some(label), bind_group_layouts: bgls, immediate_size: 0 })
        };
        let l_shadow = layout("shadow", &[Some(&bgl0_shadow)]);
        let l_scene = layout("scene", &[Some(&bgl0)]);
        let l_post = layout("post", &[Some(&bgl0), Some(&bgl_post)]);

        let depth_state = |write: bool, compare: wgpu::CompareFunction, bias: wgpu::DepthBiasState| wgpu::DepthStencilState {
            format: DEPTH,
            depth_write_enabled: Some(write),
            depth_compare: Some(compare),
            stencil: Default::default(),
            bias,
        };
        let p_shadow = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow"),
            layout: Some(&l_shadow),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("vs_shadow"), compilation_options: Default::default(), buffers: &[Some(vertex_layout())] },
            primitive: Default::default(),
            depth_stencil: Some(depth_state(true, wgpu::CompareFunction::Less, wgpu::DepthBiasState { constant: 2, slope_scale: 2.0, clamp: 0.0 })),
            multisample: Default::default(),
            fragment: None,
            multiview_mask: None,
            cache: None,
        });
        let p_scene = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("scene"),
            layout: Some(&l_scene),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("vs_scene"), compilation_options: Default::default(), buffers: &[Some(vertex_layout())] },
            primitive: Default::default(),
            depth_stencil: Some(depth_state(true, wgpu::CompareFunction::Less, Default::default())),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_scene"),
                compilation_options: Default::default(),
                targets: &[color_target(COLOR), color_target(ID)],
            }),
            multiview_mask: None,
            cache: None,
        });
        let p_sky = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sky"),
            layout: Some(&l_scene),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("vs_full"), compilation_options: Default::default(), buffers: &[] },
            primitive: Default::default(),
            depth_stencil: Some(depth_state(false, wgpu::CompareFunction::Always, Default::default())),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_sky"),
                compilation_options: Default::default(),
                targets: &[color_target(COLOR), color_target(ID)],
            }),
            multiview_mask: None,
            cache: None,
        });
        let p_post = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("post"),
            layout: Some(&l_post),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("vs_full"), compilation_options: Default::default(), buffers: &[] },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_post"),
                compilation_options: Default::default(),
                targets: &[color_target(COLOR)],
            }),
            multiview_mask: None,
            cache: None,
        });

        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let (shadow_tex, shadow_v) = texture(
            &device,
            "shadow map",
            SHADOW_RES,
            SHADOW_RES,
            DEPTH,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let smp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let g0 = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &bgl0,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&shadow_v) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&smp) },
            ],
        });
        let g0_shadow = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals (shadow pass)"),
            layout: &bgl0_shadow,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() }],
        });
        let t = Self::targets(&device, &bgl_post, w, h);
        SceneRenderer {
            device,
            queue,
            w,
            h,
            globals,
            _shadow_tex: shadow_tex,
            shadow_v,
            g0,
            g0_shadow,
            bgl_post,
            p_shadow,
            p_scene,
            p_sky,
            p_post,
            t,
            overlay: vec![0; (w * h * 4) as usize],
            overlay_dirty: true,
            terrain: MeshBuf::default(),
            terrain_key: usize::MAX,
            dynamic: MeshBuf::default(),
            scratch: Mesh::default(),
            mats: vec![],
        }
    }

    fn targets(device: &wgpu::Device, bgl_post: &wgpu::BindGroupLayout, w: u32, h: u32) -> Targets {
        let rt = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        let (color, color_v) = texture(device, "scene colour", w, h, COLOR, rt);
        let (id, id_v) = texture(device, "scene id", w, h, ID, rt);
        let (depth, depth_v) = texture(device, "scene depth", w, h, DEPTH, rt);
        let (final_tex, final_v) = texture(device, "frame", w, h, COLOR, rt | wgpu::TextureUsages::COPY_SRC);
        let (overlay_tex, overlay_v) = texture(device, "overlay", w, h, COLOR, wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST);
        let post_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("post"),
            layout: bgl_post,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&color_v) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&id_v) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&depth_v) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&overlay_v) },
            ],
        });
        let padded_row = (w * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (padded_row * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Targets { _color: color, color_v, _id: id, id_v, _depth: depth, depth_v, final_tex, final_v, overlay_tex, post_bg, readback, padded_row }
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        if (w, h) == (self.w, self.h) {
            return;
        }
        self.w = w;
        self.h = h;
        self.t = Self::targets(&self.device, &self.bgl_post, w, h);
        self.overlay = vec![0; (w * h * 4) as usize];
        self.overlay_dirty = true;
    }

    pub fn size(&self) -> (u32, u32) {
        (self.w, self.h)
    }

    /// The finished low-resolution frame (sRGB-encoded RGBA8).
    pub fn view(&self) -> &wgpu::TextureView {
        &self.t.final_v
    }

    /// Records every pass for one frame of `world` into `enc`.
    pub fn render(&mut self, world: &World, enc: &mut wgpu::CommandEncoder) {
        let aspect = self.w as f32 / self.h as f32;
        let mut cam = world.rig.cam;
        cam.target += world.cam_shake;
        let pr = cam.proj(aspect);
        let sky = &world.sky;
        let l = sky.light_dir.normalize();

        let radius = world.terrain.radius() + 8.0;
        let center = world.terrain.center();
        let up = if l.y.abs() > 0.99 { Vec3::Z } else { Vec3::Y };
        let lview = Mat4::look_at_rh(center - l * radius * 2.0, center, up);
        let lproj = Mat4::orthographic_rh(-radius, radius, -radius, radius, 0.1, radius * 4.0);
        let globals = Globals {
            view_proj: pr.vp().to_cols_array_2d(),
            light_vp: (lproj * lview).to_cols_array_2d(),
            sun_dir: v4(l, 0.0),
            sun_color: v4(sky.sun_color, 0.0),
            ambient: v4(sky.ambient, 0.0),
            ground: v4(sky.ground, 0.0),
            sky_top: v4(sky.top, 0.0),
            sky_bottom: v4(sky.bottom, 0.0),
            misc: [world.time, sky.glow, if pr.ortho { 1.0 } else { 0.0 }, 1.0 / SHADOW_RES as f32],
            depth: [pr.near, pr.far, self.w as f32, self.h as f32],
        };
        self.queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));

        let key = world.terrain.quads.as_ptr() as usize ^ world.terrain.quads.len().rotate_left(48);
        if key != self.terrain_key {
            self.terrain.upload(&self.device, &self.queue, &Mesh::from_quads(&world.terrain.quads));
            self.terrain_key = key;
        }
        self.scratch.clear();
        for (i, e) in world.entities.iter().enumerate() {
            if !world.entity_visible(e) {
                continue;
            }
            let root = Mat4::from_translation(e.pos + Vec3::new(0.0, e.elevation, 0.0))
                * Mat4::from_rotation_y(e.yaw.to_radians())
                * Mat4::from_scale(Vec3::splat(e.model.voxel * e.scale));
            let placed = Placed {
                model: &e.model,
                colors: &e.colors,
                root,
                pose: &e.pose,
                id: i as u32 + 1,
                parts_shown: usize::MAX,
                facing: 0,
                lower_interior: false,
                cutaway: false,
            };
            push_model(&placed, &mut self.mats, &mut self.scratch);
        }
        world.push_buildings(cam.yaw, &mut self.mats, &mut self.scratch);
        self.dynamic.upload(&self.device, &self.queue, &self.scratch);

        self.build_overlay(world, &pr);

        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.shadow_v,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.p_shadow);
            pass.set_bind_group(0, &self.g0_shadow, &[]);
            self.terrain.draw(&mut pass);
            self.dynamic.draw(&mut pass);
        }
        {
            let clear = |c: wgpu::Color| wgpu::Operations { load: wgpu::LoadOp::Clear(c), store: wgpu::StoreOp::Store };
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment { view: &self.t.color_v, depth_slice: None, resolve_target: None, ops: clear(wgpu::Color::BLACK) }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: &self.t.id_v,
                        depth_slice: None,
                        resolve_target: None,
                        ops: clear(wgpu::Color { r: u32::MAX as f64, g: 0.0, b: 0.0, a: 0.0 }),
                    }),
                ],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.t.depth_v,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.g0, &[]);
            pass.set_pipeline(&self.p_sky);
            pass.draw(0..3, 0..1);
            pass.set_pipeline(&self.p_scene);
            self.terrain.draw(&mut pass);
            self.dynamic.draw(&mut pass);
        }
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("post"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.t.final_v,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.p_post);
            pass.set_bind_group(0, &self.g0, &[]);
            pass.set_bind_group(1, &self.t.post_bg, &[]);
            pass.draw(0..3, 0..1);
        }
    }

    /// Name tags and speech bubbles, drawn on the CPU into a small RGBA texture.
    fn build_overlay(&mut self, world: &World, pr: &Proj) {
        let (w, h) = (self.w as usize, self.h as usize);
        self.overlay.fill(0);
        let mut any = false;
        for e in &world.entities {
            if (e.label.is_none() && e.speech.is_none()) || !world.entity_visible(e) {
                continue;
            }
            let head = e.pos + Vec3::new(0.0, e.elevation + e.model.height * e.scale + 0.2, 0.0);
            let Some((sx, sy)) = pr.to_screen(head, w, h) else { continue };
            let (sx, mut sy) = (sx as i32, sy as i32);
            let buf = &mut self.overlay;
            if let Some(label) = &e.label {
                let tw = font::text_width(label);
                let x = sx - tw / 2;
                font::draw_text(buf, w, h, x + 1, sy - 5, label, [20, 20, 30]);
                font::draw_text(buf, w, h, x, sy - 6, label, [255, 255, 255]);
                sy -= 8;
                any = true;
            }
            if let Some(text) = &e.speech {
                let lines = font::wrap(text, 16);
                let tw = lines.iter().map(|s| font::text_width(s)).max().unwrap_or(0);
                let bw = tw + 5;
                let bh = lines.len() as i32 * font::LINE_H + 3;
                let bx = (sx - bw / 2).clamp(2, (w as i32 - bw - 2).max(2));
                let by = sy - 4 - bh;
                font::rect(buf, w, h, bx - 1, by - 1, bw + 2, bh + 2, [30, 28, 40]);
                font::rect(buf, w, h, bx, by, bw, bh, [255, 255, 250]);
                font::rect(buf, w, h, sx - 1, by + bh + 1, 3, 1, [30, 28, 40]);
                font::put(buf, w, h, sx, by + bh + 2, [30, 28, 40]);
                font::put(buf, w, h, sx, by + bh, [255, 255, 250]);
                for (li, line) in lines.iter().enumerate() {
                    font::draw_text(buf, w, h, bx + 2, by + 2 + li as i32 * font::LINE_H, line, [30, 28, 40]);
                }
                any = true;
            }
        }
        if any || self.overlay_dirty {
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo { texture: &self.t.overlay_tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                &self.overlay,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(self.w * 4), rows_per_image: Some(self.h) },
                wgpu::Extent3d { width: self.w, height: self.h, depth_or_array_layers: 1 },
            );
            self.overlay_dirty = any;
        }
    }

    /// Renders one frame and reads it back as tightly packed RGB (headless recording and tests).
    pub fn render_rgb(&mut self, world: &World) -> Result<Vec<u8>> {
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("r3dsims frame") });
        self.render(world, &mut enc);
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &self.t.final_tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.t.readback,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(self.t.padded_row), rows_per_image: Some(self.h) },
            },
            wgpu::Extent3d { width: self.w, height: self.h, depth_or_array_layers: 1 },
        );
        self.queue.submit([enc.finish()]);
        self.t.readback.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| anyhow!("gpu poll: {e:?}"))?;
        let mut rgb = Vec::with_capacity((self.w * self.h * 3) as usize);
        {
            let data = self.t.readback.slice(..).get_mapped_range().map_err(|e| anyhow!("map readback: {e:?}"))?;
            for row in 0..self.h as usize {
                let line = &data[row * self.t.padded_row as usize..][..self.w as usize * 4];
                for px in line.chunks_exact(4) {
                    rgb.extend_from_slice(&px[..3]);
                }
            }
        }
        self.t.readback.unmap();
        Ok(rgb)
    }
}

/// Nearest-neighbour blit of a `SceneRenderer` frame into any render pass (the Rune UI pass).
pub struct Blit {
    device: wgpu::Device,
    layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    module: wgpu::ShaderModule,
    sampler: wgpu::Sampler,
    pipelines: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
}

impl Blit {
    pub fn new(device: &wgpu::Device) -> Blit {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blit"),
            source: wgpu::ShaderSource::Wgsl(include_str!("blit.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit"),
            entries: &[
                entry(0, wgpu::ShaderStages::FRAGMENT, tex_ty(wgpu::TextureSampleType::Float { filterable: true })),
                entry(1, wgpu::ShaderStages::FRAGMENT, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blit"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("nearest"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        Blit { device: device.clone(), layout, pipeline_layout, module, sampler, pipelines: HashMap::new() }
    }

    pub fn bind_group(&self, view: &wgpu::TextureView) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blit"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        })
    }

    /// Draws into `rect` = [x, y, w, h] in the pass's own pixels.
    pub fn draw(&mut self, pass: &mut wgpu::RenderPass<'_>, format: wgpu::TextureFormat, bind: &wgpu::BindGroup, rect: [f32; 4]) {
        let pipeline = self.pipelines.entry(format).or_insert_with(|| {
            self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("blit"),
                layout: Some(&self.pipeline_layout),
                vertex: wgpu::VertexState { module: &self.module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &self.module,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[color_target(format)],
                }),
                multiview_mask: None,
                cache: None,
            })
        });
        pass.set_viewport(rect[0], rect[1], rect[2].max(1.0), rect[3].max(1.0), 0.0, 1.0);
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, bind, &[]);
        pass.draw(0..3, 0..1);
    }
}
