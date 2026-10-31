//! Слой 3D на видеокарте — только предпросмотр, пока сцену двигают (отрисовка через wgpu). Вершины
//! в мире лежат в памяти видеокарты, поэтому поворот камеры не пересчитывает их на процессоре.
//! Без сглаживания; итог, который попадает в файл, считает процессор. Контур — тоже процессор.
//! Любая неудача — `None`.

use crate::gpu_filter;
use eframe::wgpu;
use paint_3d::model::Model;
use paint_3d::render::{gpu_camera, gpu_mesh, GpuDraw, GpuMesh};
use paint_3d::scene::{Display, Scene};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use wgpu::util::DeviceExt;

const SHADER: &str = r#"
struct G { vp: mat4x4<f32>, light: vec4<f32> };
struct M { color: vec4<f32>, ink: vec4<f32>, cutoff: f32, mode: u32, pad0: u32, pad1: u32 };
@group(0) @binding(0) var<uniform> g: G;
@group(0) @binding(1) var<uniform> m: M;
@group(0) @binding(2) var tex: texture_2d<f32>;
@group(0) @binding(3) var smp: sampler;
struct V {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) front: f32,
    @location(2) back: f32,
};
fn shade(n: vec3<f32>) -> f32 { return 0.32 + 0.68 * max(dot(n, g.light.xyz), 0.0); }
@vertex
fn vs(@location(0) p: vec3<f32>, @location(1) n: vec3<f32>, @location(2) uv: vec2<f32>) -> V {
    var o: V;
    o.pos = g.vp * vec4<f32>(p, 1.0);
    o.uv = uv;
    o.front = shade(n);
    o.back = shade(-n);
    return o;
}
@fragment
fn fs(i: V, @builtin(front_facing) face: bool) -> @location(0) vec4<f32> {
    var c = m.color * textureSample(tex, smp, i.uv);
    if (m.cutoff >= 0.0 && c.a < m.cutoff) { discard; }
    var l = i.back;
    if (face) { l = i.front; }
    switch m.mode {
        case 0u: { return vec4<f32>(c.rgb * l, 1.0); }
        case 1u: { return vec4<f32>(vec3<f32>(0.82, 0.8, 0.78) * l, 1.0); }
        default: { return m.ink; }
    }
}
"#;

/// С какого числа треугольников видеокарта выгоднее (меньше — процессор и так быстр).
const MIN_TRIS: usize = 100_000;

struct Scene3dGpu {
    gpu: gpu_filter::Gpu,
    /// Обе стороны и только передняя.
    pipelines: [wgpu::RenderPipeline; 2],
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    white: wgpu::TextureView,
    /// Что сейчас лежит в буферах: сцена без камеры и модели (модели держим, чтобы адреса не повторились).
    key: Option<(String, Vec<Arc<Model>>)>,
    buffers: Option<(wgpu::Buffer, wgpu::Buffer, GpuMesh)>,
    textures: HashMap<(usize, usize), wgpu::TextureView>,
}

static STATE: Mutex<Option<Scene3dGpu>> = Mutex::new(None);

fn image_texture(gpu: &gpu_filter::Gpu, w: u32, h: u32, rgba: &[u8]) -> wgpu::TextureView {
    gpu.device
        .create_texture_with_data(
            &gpu.queue,
            &wgpu::TextureDescriptor {
                label: Some("gpu_3d"),
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            Default::default(),
            rgba,
        )
        .create_view(&Default::default())
}

impl Scene3dGpu {
    fn new(gpu: gpu_filter::Gpu) -> Self {
        let d = &gpu.device;
        let module = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("gpu_3d"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let uniform = |binding, visibility| wgpu::BindGroupLayoutEntry { binding, visibility, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None };
        let layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gpu_3d"),
            entries: &[
                uniform(0, wgpu::ShaderStages::VERTEX_FRAGMENT),
                uniform(1, wgpu::ShaderStages::FRAGMENT),
                wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
                wgpu::BindGroupLayoutEntry { binding: 3, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let pl = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("gpu_3d"), bind_group_layouts: &[&layout], push_constant_ranges: &[] });
        let pipeline = |cull| {
            d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("gpu_3d"),
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: "vs",
                    compilation_options: Default::default(),
                    buffers: &[wgpu::VertexBufferLayout { array_stride: 32, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2] }],
                },
                primitive: wgpu::PrimitiveState { front_face: wgpu::FrontFace::Ccw, cull_mode: cull, ..Default::default() },
                depth_stencil: Some(wgpu::DepthStencilState { format: wgpu::TextureFormat::Depth32Float, depth_write_enabled: true, depth_compare: wgpu::CompareFunction::Less, stencil: Default::default(), bias: Default::default() }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState { module: &module, entry_point: "fs", compilation_options: Default::default(), targets: &[Some(wgpu::TextureFormat::Rgba8Unorm.into())] }),
                multiview: None,
                cache: None,
            })
        };
        let pipelines = [pipeline(None), pipeline(Some(wgpu::Face::Back))];
        let sampler = d.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let white = image_texture(&gpu, 1, 1, &[255; 4]);
        Scene3dGpu { pipelines, layout, sampler, white, key: None, buffers: None, textures: HashMap::new(), gpu }
    }

    fn draw(&mut self, scene: &Scene, models: &[Arc<Model>], w: u32, h: u32) -> Option<Vec<u8>> {
        let mode = match scene.display {
            Display::Lit => 0u32,
            Display::Clay => 1,
            Display::Silhouette => 2,
            Display::Outline => return None,
        };
        let limits = self.gpu.device.limits();
        if w.max(h) > limits.max_texture_dimension_2d {
            return None;
        }
        self.gpu.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        self.gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let done = self.draw_inner(scene, models, w, h, mode);
        let (e1, e2) = (pollster::block_on(self.gpu.device.pop_error_scope()), pollster::block_on(self.gpu.device.pop_error_scope()));
        if let Some(e) = e1.iter().chain(&e2).next() {
            eprintln!("3D на видеокарте: {e}");
            self.key = None;
            return None;
        }
        done
    }

    fn draw_inner(&mut self, scene: &Scene, models: &[Arc<Model>], w: u32, h: u32, mode: u32) -> Option<Vec<u8>> {
        let device = self.gpu.device.clone();
        let queue = self.gpu.queue.clone();
        // поза, морфы, состав сцены — без камеры, света и вида
        let mut bare = scene.clone();
        bare.camera = Default::default();
        bare.light = [0.0; 2];
        bare.display = Display::Lit;
        bare.ink = [0; 3];
        let text = bare.to_text();
        let same = self.key.as_ref().is_some_and(|(t, m)| *t == text && m.len() == models.len() && m.iter().zip(models).all(|(a, b)| Arc::ptr_eq(a, b)));
        if !same {
            if !self.key.as_ref().is_some_and(|(_, m)| m.len() == models.len() && m.iter().zip(models).all(|(a, b)| Arc::ptr_eq(a, b))) {
                self.textures.clear();
            }
            let mesh = gpu_mesh(scene, models);
            if mesh.indices.is_empty() {
                self.key = None;
                self.buffers = None;
                return Some(vec![0; w as usize * h as usize * 4]);
            }
            let max = device.limits().max_buffer_size;
            if mesh.vertices.len() as u64 * 32 > max || mesh.indices.len() as u64 * 4 > max {
                self.key = None;
                self.buffers = None;
                return None;
            }
            // SAFETY: f32 и u32 без промежутков
            let vb: &[u8] = unsafe { std::slice::from_raw_parts(mesh.vertices.as_ptr() as *const u8, mesh.vertices.len() * 32) };
            let ib: &[u8] = unsafe { std::slice::from_raw_parts(mesh.indices.as_ptr() as *const u8, mesh.indices.len() * 4) };
            let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("gpu_3d"), contents: vb, usage: wgpu::BufferUsages::VERTEX });
            let ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("gpu_3d"), contents: ib, usage: wgpu::BufferUsages::INDEX });
            self.buffers = Some((vbuf, ibuf, mesh));
            self.key = Some((text, models.to_vec()));
        }
        let max_tex = device.limits().max_texture_dimension_2d;
        let needed: Vec<(usize, usize)> = self.buffers.as_ref()?.2.draws.iter().filter_map(|d| d.tex).collect();
        for k in needed {
            if !self.textures.contains_key(&k) {
                let img = models.get(k.0)?.images.get(k.1)?;
                if img.w == 0 || img.h == 0 || img.w.max(img.h) > max_tex || img.rgba.len() != img.w as usize * img.h as usize * 4 {
                    return None;
                }
                let view = image_texture(&self.gpu, img.w, img.h, &img.rgba);
                self.textures.insert(k, view);
            }
        }
        let (vbuf, ibuf, mesh) = self.buffers.as_ref()?;
        let (vp, light) = gpu_camera(scene, mesh, w, h);
        let mut gbytes = Vec::with_capacity(80);
        for v in vp.to_cols_array().into_iter().chain([light.x, light.y, light.z, 0.0]) {
            gbytes.extend_from_slice(&v.to_le_bytes());
        }
        let globals = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("gpu_3d"), contents: &gbytes, usage: wgpu::BufferUsages::UNIFORM });
        let ink = scene.ink.map(|c| c as f32 / 255.0);
        let bind_of = |d: &GpuDraw| {
            let mut b = Vec::with_capacity(48);
            for v in d.color.into_iter().chain([ink[0], ink[1], ink[2], 1.0, d.cutoff.unwrap_or(-1.0)]) {
                b.extend_from_slice(&v.to_le_bytes());
            }
            b.extend_from_slice(&mode.to_le_bytes());
            b.extend_from_slice(&[0u8; 8]);
            let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("gpu_3d"), contents: &b, usage: wgpu::BufferUsages::UNIFORM });
            let view = d.tex.and_then(|k| self.textures.get(&k)).unwrap_or(&self.white);
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("gpu_3d"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: buf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(view) },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                ],
            })
        };
        let binds: Vec<_> = mesh.draws.iter().map(bind_of).collect();
        let size = wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 };
        let target = |format, usage| device.create_texture(&wgpu::TextureDescriptor { label: Some("gpu_3d"), size, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format, usage, view_formats: &[] });
        let color = target(wgpu::TextureFormat::Rgba8Unorm, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC);
        let depth = target(wgpu::TextureFormat::Depth32Float, wgpu::TextureUsages::RENDER_ATTACHMENT);
        let (cv, dv) = (color.create_view(&Default::default()), depth.create_view(&Default::default()));
        let row = (w * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let read = device.create_buffer(&wgpu::BufferDescriptor { label: Some("gpu_3d"), size: row as u64 * h as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        let mut enc = device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("gpu_3d"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &cv, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store } })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &dv, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }), stencil_ops: None }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_vertex_buffer(0, vbuf.slice(..));
            pass.set_index_buffer(ibuf.slice(..), wgpu::IndexFormat::Uint32);
            for (d, b) in mesh.draws.iter().zip(&binds) {
                pass.set_pipeline(&self.pipelines[if d.double { 0 } else { 1 }]);
                pass.set_bind_group(0, b, &[]);
                pass.draw_indexed(d.first..d.first + d.count, 0, 0..1);
            }
        }
        enc.copy_texture_to_buffer(color.as_image_copy(), wgpu::ImageCopyBuffer { buffer: &read, layout: wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: None } }, size);
        let sub = queue.submit(Some(enc.finish()));
        let slice = read.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r.is_ok());
        });
        device.poll(wgpu::Maintain::WaitForSubmissionIndex(sub));
        if !rx.recv().unwrap_or(false) {
            return None;
        }
        let data = slice.get_mapped_range();
        let mut out = vec![0u8; w as usize * h as usize * 4];
        for (o, line) in out.chunks_mut(w as usize * 4).zip(data.chunks(row as usize)) {
            o.copy_from_slice(&line[..w as usize * 4]);
        }
        Some(out)
    }
}

/// Предпросмотр сцены `w`×`h`: RGBA8 без предумножения, как `paint_3d::render::render`.
/// `None` — лёгкая сцена, контур или нет видеокарты: считает процессор.
pub fn render(scene: &Scene, models: &[Arc<Model>], w: u32, h: u32) -> Option<Vec<u8>> {
    if std::env::var("PAINT_GPU_3D").as_deref() == Ok("off") || models.iter().map(|m| m.triangles).sum::<usize>() < MIN_TRIS {
        return None;
    }
    let gpu = gpu_filter::current()?;
    let t = std::time::Instant::now();
    let mut st = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let r = st.get_or_insert_with(|| Scene3dGpu::new(gpu)).draw(scene, models, w, h);
    if std::env::var_os("PAINT_TIMING").is_some() {
        eprintln!("3D на видеокарте: {:?}, вышло {}", t.elapsed(), r.is_some());
    }
    r
}

/// Правка сцены окончена: вершины и картинки больше не нужны видеокарте.
pub fn release() {
    *STATE.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{Quat, Vec2, Vec3};
    use paint_3d::model::{Image, Mesh, Node, Prim};
    use paint_3d::scene::Object;

    fn gpu() -> Option<gpu_filter::Gpu> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::PRIMARY, ..Default::default() });
        let name = std::env::var("PAINT_GPU_TEST_NAME").unwrap_or_default();
        let adapter = instance.enumerate_adapters(wgpu::Backends::PRIMARY).into_iter().find(|a| a.get_info().name.contains(&name))?;
        let (d, q) = pollster::block_on(adapter.request_device(&Default::default(), None)).ok()?;
        Some(gpu_filter::Gpu { device: Arc::new(d), queue: Arc::new(q), pipelines: Arc::default() })
    }

    /// Шар из `n`×`n` клеток радиуса 1.
    fn ball(n: u32, normals: bool, tex: bool, double: bool) -> Arc<Model> {
        let mut p = Prim { color: [0.9, 0.5, 0.3, 1.0], double, tex: tex.then_some(0), ..Default::default() };
        for j in 0..=n {
            for i in 0..=n {
                let (u, v) = (i as f32 / n as f32, j as f32 / n as f32);
                let (a, b) = (u * std::f32::consts::TAU, v * std::f32::consts::PI);
                let q = Vec3::new(a.cos() * b.sin(), b.cos(), a.sin() * b.sin());
                p.pos.push(q);
                if normals {
                    p.nrm.push(q);
                }
                p.uv.push(Vec2::new(u * 2.0, v * 2.0));
            }
        }
        for j in 0..n {
            for i in 0..n {
                let (a, b, c, d) = (j * (n + 1) + i, j * (n + 1) + i + 1, (j + 1) * (n + 1) + i, (j + 1) * (n + 1) + i + 1);
                p.idx.extend_from_slice(&[a, b, c, b, d, c]);
            }
        }
        let triangles = p.idx.len() / 3;
        let node = Node { name: "ball".into(), parent: None, children: vec![], t: Vec3::ZERO, r: Quat::IDENTITY, s: Vec3::ONE, mesh: Some(0), skin: None, joint: false };
        let images = if tex { vec![Image { w: 2, h: 2, rgba: vec![255, 255, 255, 255, 60, 60, 60, 255, 60, 60, 60, 255, 255, 255, 255, 255] }] } else { vec![] };
        Arc::new(Model { nodes: vec![node], roots: vec![0], meshes: vec![Mesh { name: "m".into(), prims: vec![p], morph_names: vec![], weights: vec![] }], skins: vec![], images, triangles })
    }

    /// Доля точек, расходящихся с процессором больше чем на 12 из 255 (край и 1/255 округления — не беда).
    fn off_share(a: &[u8], b: &[u8]) -> f32 {
        let bad = a.chunks(4).zip(b.chunks(4)).filter(|(x, y)| x.iter().zip(y.iter()).any(|(p, q)| (*p as i32 - *q as i32).abs() > 12)).count();
        bad as f32 / (a.len() / 4) as f32
    }

    #[test]
    fn preview_matches_cpu() {
        let Some(g) = gpu() else { return };
        let mut r = Scene3dGpu::new(g);
        for (normals, tex, double, display) in [
            (true, false, false, Display::Lit),
            (false, false, false, Display::Lit),
            (true, true, false, Display::Lit),
            (true, false, true, Display::Clay),
            (true, false, false, Display::Silhouette),
        ] {
            let models = vec![ball(64, normals, tex, double)];
            let mut scene = Scene::default();
            scene.objects.push(Object::new(0));
            scene.display = display;
            let got = r.draw(&scene, &models, 400, 300).expect("считается");
            let want = paint_3d::render::render(&scene, &models, 400, 300, 1);
            let share = off_share(&got, &want);
            let filled = want.chunks(4).filter(|p| p[3] != 0).count();
            eprintln!("нормали {normals}, текстура {tex}, две стороны {double}: расходится {:.3}%, закрашено {filled}", share * 100.0);
            assert!(filled > 2000, "шар в кадре");
            assert!(share < 0.01, "расходится {share}");
        }
    }

    /// Поворот камеры не загружает вершины заново.
    #[test]
    fn camera_keeps_buffers() {
        let Some(g) = gpu() else { return };
        let mut r = Scene3dGpu::new(g);
        let models = vec![ball(16, true, false, false)];
        let mut scene = Scene::default();
        scene.objects.push(Object::new(0));
        let a = r.draw(&scene, &models, 200, 200).unwrap();
        let p = r.buffers.as_ref().unwrap().0.global_id();
        scene.camera.yaw += 0.7;
        let b = r.draw(&scene, &models, 200, 200).unwrap();
        assert_eq!(r.buffers.as_ref().unwrap().0.global_id(), p);
        assert_eq!(a.len(), b.len());
        scene.objects[0].pos.x += 0.5;
        let c = r.draw(&scene, &models, 200, 200).unwrap();
        assert_ne!(r.buffers.as_ref().unwrap().0.global_id(), p);
        assert_ne!(b, c);
    }

    /// Замер: `cargo test --release -p paint-app scene_speed -- --ignored --nocapture`.
    #[test]
    #[ignore = "замер"]
    fn scene_speed() {
        let Some(g) = gpu() else { return };
        let mut r = Scene3dGpu::new(g);
        let models = vec![ball(708, true, false, false)];
        let mut scene = Scene::default();
        scene.objects.push(Object::new(0));
        eprintln!("треугольников: {}", models[0].triangles);
        for pass in 0..4 {
            scene.camera.yaw += 0.1;
            let t = std::time::Instant::now();
            let _ = r.draw(&scene, &models, 3840, 2160);
            eprintln!("видеокарта, проход {pass}: {:?}", t.elapsed());
        }
        for ss in [1, 2] {
            let t = std::time::Instant::now();
            let _ = paint_3d::render::render(&scene, &models, 3840, 2160, ss);
            eprintln!("процессор ×{ss}: {:?}", t.elapsed());
        }
    }
}
