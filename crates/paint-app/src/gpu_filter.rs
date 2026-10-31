//! Фильтры на видеокарте — только для предпросмотра (отрисовка через wgpu): вычислительный шейдер
//! считает то же ядро, что и `paint_core::filters`, но в числах f32, поэтому до бита не совпадает.
//! Итог, который попадает в файл, считает процессор. Любая неудача — `None`, считает процессор.

use eframe::wgpu;
use paint_core::filters::{gaussian_area, gaussian_kernel, write_back};
use paint_core::tile::ColorMap;
use std::sync::{Arc, Mutex, OnceLock};

const SHADER: &str = r#"
struct P { dir_x: i32, dir_y: i32, r: i32, y0: i32, y1: i32, w: i32, h: i32, f: i32, sw: i32, sh: i32, pad0: i32, pad1: i32 };
@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var dst: texture_storage_2d<rgba32float, write>;
@group(0) @binding(2) var<storage, read> k: array<f32>;
@group(0) @binding(3) var<uniform> p: P;
fn at(x: i32, y: i32) -> vec4<f32> {
    return textureLoad(src, vec2<i32>(clamp(x, 0, p.sw - 1), clamp(y, 0, p.sh - 1)), 0);
}
@compute @workgroup_size(64, 1, 1)
fn blur(@builtin(global_invocation_id) id: vec3<u32>) {
    let x = i32(id.x);
    let y = i32(id.y) + p.y0;
    if (x >= p.w || y >= p.y1) { return; }
    let dir = vec2<i32>(p.dir_x, p.dir_y);
    var acc = vec4<f32>(0.0);
    for (var j = -p.r; j <= p.r; j++) {
        let q = vec2<i32>(x, y) + dir * j;
        if (q.x < 0 || q.y < 0 || q.x >= p.w || q.y >= p.h) { continue; }
        acc += textureLoad(src, q, 0) * k[j + p.r];
    }
    textureStore(dst, vec2<i32>(x, y), acc);
}
// уменьшение вдвое: среднее четырёх точек
@compute @workgroup_size(64, 1, 1)
fn down(@builtin(global_invocation_id) id: vec3<u32>) {
    let x = i32(id.x);
    let y = i32(id.y) + p.y0;
    if (x >= p.w || y >= p.y1) { return; }
    let c = at(2 * x, 2 * y) + at(2 * x + 1, 2 * y) + at(2 * x, 2 * y + 1) + at(2 * x + 1, 2 * y + 1);
    textureStore(dst, vec2<i32>(x, y), c * 0.25);
}
// увеличение в f раз, билинейно
@compute @workgroup_size(64, 1, 1)
fn up(@builtin(global_invocation_id) id: vec3<u32>) {
    let x = i32(id.x);
    let y = i32(id.y) + p.y0;
    if (x >= p.w || y >= p.y1) { return; }
    let u = (vec2<f32>(f32(x), f32(y)) + 0.5) / f32(p.f) - 0.5;
    let b = floor(u);
    let t = u - b;
    let x0 = i32(b.x);
    let y0 = i32(b.y);
    let top = mix(at(x0, y0), at(x0 + 1, y0), t.x);
    let bot = mix(at(x0, y0 + 1), at(x0 + 1, y0 + 1), t.x);
    textureStore(dst, vec2<i32>(x, y), mix(top, bot, t.y));
}
"#;

/// Строк за одну отправку: длинная работа одним куском — сброс драйвера (TDR) в Windows.
const BAND: u32 = 128;

/// Видеокарта отрисовки окна.
#[derive(Clone)]
pub struct Gpu {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    pipelines: Arc<OnceLock<[wgpu::ComputePipeline; 3]>>,
}

/// Устройство, которое задаёт окно при отрисовке через wgpu.
static CURRENT: Mutex<Option<Gpu>> = Mutex::new(None);

pub fn set_current(device: &Arc<wgpu::Device>, queue: &Arc<wgpu::Queue>) {
    let mut g = CURRENT.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_none() && std::env::var("PAINT_GPU_FILTER").as_deref() != Ok("off") {
        *g = Some(Gpu { device: device.clone(), queue: queue.clone(), pipelines: Arc::default() });
    }
}

pub fn current() -> Option<Gpu> {
    CURRENT.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

fn texture(d: &wgpu::Device, w: u32, h: u32, usage: wgpu::TextureUsages) -> wgpu::Texture {
    d.create_texture(&wgpu::TextureDescriptor {
        label: Some("gpu_filter"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba32Float,
        usage,
        view_formats: &[],
    })
}

impl Gpu {
    /// Размытие, уменьшение, увеличение.
    fn pipelines(&self) -> &[wgpu::ComputePipeline; 3] {
        self.pipelines.get_or_init(|| {
            let module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("gpu_filter"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
            // явно: сам wgpu считает текстуру фильтруемой, а Rgba32Float такой не бывает
            let entry = |binding, ty| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::COMPUTE, ty, count: None };
            let bgl = self.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("gpu_filter"),
                entries: &[
                    entry(0, wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }),
                    entry(1, wgpu::BindingType::StorageTexture { access: wgpu::StorageTextureAccess::WriteOnly, format: wgpu::TextureFormat::Rgba32Float, view_dimension: wgpu::TextureViewDimension::D2 }),
                    entry(2, wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None }),
                    entry(3, wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }),
                ],
            });
            let layout = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("gpu_filter"), bind_group_layouts: &[&bgl], push_constant_ranges: &[] });
            ["blur", "down", "up"].map(|entry_point| {
                self.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("gpu_filter"),
                    layout: Some(&layout),
                    module: &module,
                    entry_point,
                    compilation_options: Default::default(),
                    cache: None,
                })
            })
        })
    }

    /// Один проход шейдера из `from` в `to` размера `w`×`h`, полосами строк.
    fn pass(&self, which: usize, from: &wgpu::Texture, to: &wgpu::Texture, kernel: &wgpu::Buffer, [dx, dy, r, f]: [i32; 4]) {
        use wgpu::util::DeviceExt;
        let (w, h) = (to.width(), to.height());
        let (sw, sh) = (from.width() as i32, from.height() as i32);
        let pipeline = &self.pipelines()[which];
        let (src, dst) = (from.create_view(&Default::default()), to.create_view(&Default::default()));
        let layout = pipeline.get_bind_group_layout(0);
        let mut y0 = 0;
        while y0 < h {
            let y1 = (y0 + BAND).min(h);
            let params: [i32; 12] = [dx, dy, r, y0 as i32, y1 as i32, w as i32, h as i32, f, sw, sh, 0, 0];
            let bytes: Vec<u8> = params.iter().flat_map(|v| v.to_le_bytes()).collect();
            let uniform = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("gpu_filter"), contents: &bytes, usage: wgpu::BufferUsages::UNIFORM });
            let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("gpu_filter"),
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&src) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&dst) },
                    wgpu::BindGroupEntry { binding: 2, resource: kernel.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 3, resource: uniform.as_entire_binding() },
                ],
            });
            let mut enc = self.device.create_command_encoder(&Default::default());
            {
                let mut pass = enc.begin_compute_pass(&Default::default());
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &bind, &[]);
                pass.dispatch_workgroups(w.div_ceil(64), y1 - y0, 1);
            }
            let sub = self.queue.submit(Some(enc.finish()));
            self.device.poll(wgpu::Maintain::WaitForSubmissionIndex(sub));
            y0 = y1;
        }
    }

    /// Размытие по Гауссу; `None` — считать на процессоре.
    pub fn gaussian(&self, src: &ColorMap, w: u32, h: u32, radius: f64, sel: &(dyn Fn(i32, i32) -> u32 + Sync)) -> Option<ColorMap> {
        use wgpu::util::DeviceExt;
        let (_, r) = gaussian_kernel(radius);
        let (x0, y0, bw, bh) = gaussian_area(src, w, h, r)?;
        let (tw, th) = (bw as u32, bh as u32);
        let limits = self.device.limits();
        if tw.max(th) > limits.max_texture_dimension_2d || limits.max_storage_textures_per_shader_stage == 0 {
            return None;
        }
        let timing = std::env::var_os("PAINT_TIMING").is_some();
        let t0 = std::time::Instant::now();
        self.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let k = 1.0 / paint_core::pixel::ONE as f32;
        let px = src.read_rect(x0, y0, bw, bh);
        let mut upload = vec![0u8; bw * bh * 16];
        let threads = paint_core::threads().max(1);
        std::thread::scope(|scope| {
            for (o, p) in upload.chunks_mut((bw * bh).div_ceil(threads).max(1) * 16).zip(px.chunks((bw * bh).div_ceil(threads).max(1))) {
                scope.spawn(move || {
                    for (o, p) in o.as_chunks_mut::<16>().0.iter_mut().zip(p) {
                        for c in 0..4 {
                            o[c * 4..c * 4 + 4].copy_from_slice(&(p[c] as f32 * k).to_le_bytes());
                        }
                    }
                });
            }
        });
        drop(px);
        use wgpu::TextureUsages as U;
        let a = texture(&self.device, tw, th, U::TEXTURE_BINDING | U::COPY_DST);
        let mid = U::TEXTURE_BINDING | U::STORAGE_BINDING;
        let c = texture(&self.device, tw, th, U::STORAGE_BINDING | U::COPY_SRC | U::TEXTURE_BINDING);
        let size = wgpu::Extent3d { width: tw, height: th, depth_or_array_layers: 1 };
        self.queue.write_texture(a.as_image_copy(), &upload, wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(tw * 16), rows_per_image: Some(th) }, size);
        drop(upload);
        let t1 = t0.elapsed();
        // Большой радиус — на уменьшенной копии: длина ядра падает в f раз, для предпросмотра
        // разницы не видно (итог всё равно считает процессор). Уменьшение и увеличение сами немного
        // размывают — это вычтено из сигмы.
        let sigma = radius.max(0.1);
        let mut f = 1u32;
        while sigma / (f as f64 * 2.0) >= 2.5 && (tw / (f * 2)).min(th / (f * 2)) >= 8 {
            f *= 2;
        }
        let small = ((sigma * sigma - (f * f) as f64 / 4.0).max(0.01)).sqrt() / f as f64;
        let (kernel, kr) = gaussian_kernel(if f > 1 { small } else { sigma });
        let kbytes: Vec<u8> = kernel.iter().flat_map(|v| (*v as f32).to_le_bytes()).collect();
        let kbuf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("gpu_filter"), contents: &kbytes, usage: wgpu::BufferUsages::STORAGE });
        const BLUR: usize = 0;
        const DOWN: usize = 1;
        const UP: usize = 2;
        if f == 1 {
            let b = texture(&self.device, tw, th, mid);
            self.pass(BLUR, &a, &b, &kbuf, [1, 0, kr, 1]);
            self.pass(BLUR, &b, &c, &kbuf, [0, 1, kr, 1]);
        } else {
            let mut cur = a;
            let mut n = 1;
            while n < f {
                n *= 2;
                let next = texture(&self.device, tw.div_ceil(n), th.div_ceil(n), mid);
                self.pass(DOWN, &cur, &next, &kbuf, [0, 0, 0, 1]);
                cur = next;
            }
            let b1 = texture(&self.device, cur.width(), cur.height(), mid);
            let b2 = texture(&self.device, cur.width(), cur.height(), mid);
            self.pass(BLUR, &cur, &b1, &kbuf, [1, 0, kr, 1]);
            self.pass(BLUR, &b1, &b2, &kbuf, [0, 1, kr, 1]);
            self.pass(UP, &b2, &c, &kbuf, [0, 0, 0, f as i32]);
        }
        let t2 = t0.elapsed();
        let row = (tw * 16).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let read = self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("gpu_filter"), size: row as u64 * th as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        let mut enc = self.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(c.as_image_copy(), wgpu::ImageCopyBuffer { buffer: &read, layout: wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(th) } }, size);
        self.queue.submit(Some(enc.finish()));
        let slice = read.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r.is_ok());
        });
        self.device.poll(wgpu::Maintain::Wait);
        let mapped = rx.recv().unwrap_or(false);
        let (e1, e2) = (pollster::block_on(self.device.pop_error_scope()), pollster::block_on(self.device.pop_error_scope()));
        if let Some(e) = e1.iter().chain(&e2).next() {
            eprintln!("фильтр на видеокарте: {e}");
            return None;
        }
        if !mapped {
            return None;
        }
        let t3 = t0.elapsed();
        let data = slice.get_mapped_range();
        let mut out = vec![[0f64; 4]; bw * bh];
        let lines = bh.div_ceil(threads).max(1);
        std::thread::scope(|scope| {
            for (part, rows) in out.chunks_mut(lines * bw).zip(data.chunks(lines * row as usize)) {
                scope.spawn(move || {
                    for (o, line) in part.chunks_mut(bw).zip(rows.chunks(row as usize)) {
                        for (o, px) in o.iter_mut().zip(line.as_chunks::<16>().0) {
                            *o = std::array::from_fn(|c| f32::from_le_bytes([px[c * 4], px[c * 4 + 1], px[c * 4 + 2], px[c * 4 + 3]]) as f64);
                        }
                    }
                });
            }
        });
        let res = write_back(src, x0, y0, bw, &out, sel);
        if timing {
            eprintln!("видеокарта: загрузка {t1:?}, счёт {:?}, чтение {:?}, в плитки {:?}", t2 - t1, t3 - t2, t0.elapsed() - t3);
        }
        Some(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paint_core::pixel::from_rgba8;

    fn device() -> Option<Gpu> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::PRIMARY, ..Default::default() });
        // PAINT_GPU_TEST_NAME — выбрать устройство для замера
        let name = std::env::var("PAINT_GPU_TEST_NAME").unwrap_or_default();
        let adapter = instance.enumerate_adapters(wgpu::Backends::PRIMARY).into_iter().find(|a| a.get_info().name.contains(&name))?;
        eprintln!("устройство: {}", adapter.get_info().name);
        let (d, q) = pollster::block_on(adapter.request_device(&Default::default(), None)).ok()?;
        Some(Gpu { device: Arc::new(d), queue: Arc::new(q), pipelines: Arc::default() })
    }

    fn picture(w: i32, h: i32) -> ColorMap {
        let mut m = ColorMap::new();
        for y in 0..h {
            for x in 0..w {
                let a = ((x * 7 + y * 13) % 256) as u8;
                m.set(x, y, from_rgba8([a, (x % 256) as u8, (y % 256) as u8, if (x / 16 + y / 16) % 3 == 0 { 0 } else { 255 - a / 2 }]));
            }
        }
        m
    }

    /// На видеокарте выходит почти то же, что на процессоре (без устройства — пропуск).
    #[test]
    fn gaussian_matches_cpu() {
        let Some(gpu) = device() else { return };
        let src = picture(300, 200);
        let half = |x: i32, _y: i32| if x < 150 { 0x8000u32 } else { 0x4000 };
        for radius in [1.0, 4.5, 20.0] {
            for sel in [&half as &(dyn Fn(i32, i32) -> u32 + Sync)] {
                let g = gpu.gaussian(&src, 300, 200, radius, sel).expect("считается");
                let c = paint_core::filters::gaussian_blur(&src, 300, 200, radius, sel);
                // (худшее внутри, худшее у края холста — там ядро обрезано)
                let (mut inner, mut edge) = (0i32, 0i32);
                let m = (radius * 3.0).ceil() as i32;
                for y in 0..200 {
                    for x in 0..300 {
                        let (a, b) = (g.get(x, y), c.get(x, y));
                        for i in 0..4 {
                            let d = (a[i] as i32 - b[i] as i32).abs();
                            if x >= m && y >= m && x < 300 - m && y < 200 - m {
                                inner = inner.max(d);
                            } else {
                                edge = edge.max(d);
                            }
                        }
                    }
                }
                eprintln!("радиус {radius}: внутри {inner}, у края {edge} из {}", paint_core::pixel::ONE);
                // без уменьшения — до 1/255, на уменьшенной копии — внутри до 1/128, у края до 1/16
                let (li, le) = if radius < 5.0 { (paint_core::pixel::ONE / 255, paint_core::pixel::ONE / 255) } else { (paint_core::pixel::ONE / 128, paint_core::pixel::ONE / 16) };
                assert!(inner <= li as i32 && edge <= le as i32, "радиус {radius}: внутри {inner}, у края {edge}");
            }
        }
    }

    /// Замер: `cargo test --release -p paint-app gaussian_speed -- --ignored --nocapture`.
    #[test]
    #[ignore = "замер"]
    fn gaussian_speed() {
        let Some(gpu) = device() else { return };
        let src = picture(3840, 2160);
        let all = |_: i32, _: i32| paint_core::pixel::ONE;
        for radius in [5.0, 20.0, 60.0] {
            let t = std::time::Instant::now();
            let _ = gpu.gaussian(&src, 3840, 2160, radius, &all).expect("считается");
            let g = t.elapsed();
            let t = std::time::Instant::now();
            let _ = paint_core::filters::gaussian_blur(&src, 3840, 2160, radius, &all);
            eprintln!("радиус {radius}: видеокарта {g:?}, процессор {:?}", t.elapsed());
        }
    }
}
