//! Сведение слоёв на видеокарте — только для показа, когда пересчитывается большая часть холста
//! (видимость, непрозрачность, режим слоя) и все слои обычные (`Document::gpu_stack`). Плитки слоёв
//! лежат в памяти видеокарты и узнаются по содержимому, поэтому загружается только изменённое.
//! Сохранение и сведение в файл считает процессор. Любая неудача — `None`, считает процессор.

use eframe::wgpu;
use paint_core::tile::{Key, TILE, TILE_LEN};
use paint_core::Document;
use std::collections::HashMap;

const SHADER: &str = r#"
struct E { slot: u32, mode: u32, op: f32, pad: u32 };
struct P { bg: vec4<f32>, one: f32, pages: u32, pad0: u32, pad1: u32 };
@group(0) @binding(0) var atlas: texture_2d_array<u32>;
@group(0) @binding(1) var<storage, read> tiles: array<vec2<u32>>;
@group(0) @binding(2) var<storage, read> entries: array<E>;
@group(0) @binding(3) var<storage, read_write> out: array<u32>;
@group(0) @binding(4) var<uniform> p: P;

fn dodge(b: f32, s: f32) -> f32 {
    if (b <= 0.0) { return 0.0; }
    if (s >= 1.0) { return 1.0; }
    return min(b / (1.0 - s), 1.0);
}
fn burn(b: f32, s: f32) -> f32 {
    if (b >= 1.0) { return 1.0; }
    if (s <= 0.0) { return 0.0; }
    return 1.0 - min((1.0 - b) / s, 1.0);
}
fn hard(b: f32, s: f32) -> f32 {
    if (s <= 0.5) { return b * 2.0 * s; }
    let s2 = 2.0 * s - 1.0;
    return b + s2 - b * s2;
}
fn vivid(b: f32, s: f32) -> f32 {
    if (s <= 0.5) { return burn(b, 2.0 * s); }
    return dodge(b, 2.0 * s - 1.0);
}
// номера — порядок BlendMode::ALL
fn channel(m: u32, b: f32, s: f32) -> f32 {
    switch m {
        case 0u: { return s; }
        case 1u: { return b * s; }
        case 2u: { return b + s - b * s; }
        case 3u: { return hard(s, b); }
        case 4u, 18u: { return min(b, s); }
        case 5u, 19u: { return max(b, s); }
        case 6u, 22u: { return dodge(b, s); }
        case 7u, 21u: { return burn(b, s); }
        case 8u, 16u: { return min(b + s, 1.0); }
        case 9u, 17u: { return max(b + s - 1.0, 0.0); }
        case 10u: { return hard(b, s); }
        case 11u: {
            if (s <= 0.5) { return b - (1.0 - 2.0 * s) * b * (1.0 - b); }
            var d = sqrt(b);
            if (b <= 0.25) { d = ((16.0 * b - 12.0) * b + 4.0) * b; }
            return b + (2.0 * s - 1.0) * (d - b);
        }
        case 12u: { return abs(b - s); }
        case 13u: { return b + s - 2.0 * b * s; }
        case 14u: { return max(b - s, 0.0); }
        case 15u: {
            if (s <= 0.0) { return 1.0; }
            return min(b / s, 1.0);
        }
        case 23u, 24u: { return vivid(b, s); }
        case 20u, 25u: { return clamp(b + 2.0 * s - 1.0, 0.0, 1.0); }
        case 26u: {
            if (s <= 0.5) { return min(b, 2.0 * s); }
            return max(b, 2.0 * s - 1.0);
        }
        default: {
            if (b + s >= 1.0) { return 1.0; }
            return 0.0;
        }
    }
}
fn lum(c: vec3<f32>) -> f32 { return 0.30 * c.r + 0.59 * c.g + 0.11 * c.b; }
fn composite(m: u32, dst: vec4<f32>, src: vec4<f32>, op: f32) -> vec4<f32> {
    let sa = src.a * op;
    if (sa <= 0.0) { return dst; }
    let da = dst.a;
    let oa = sa + da - sa * da;
    if (m == 16u) { return vec4<f32>(min(dst.rgb + src.rgb * op, vec3<f32>(oa)), oa); }
    if (m == 17u && da > 0.0) {
        let lack = sa - src.rgb * op;
        return vec4<f32>(max(dst.rgb - lack * da, vec3<f32>(0.0)) + src.rgb * op * (1.0 - da), oa);
    }
    if ((m >= 20u && m <= 23u) && da > 0.0) {
        var neutral = 0.5;
        var base = 25u;
        if (m == 21u) { neutral = 1.0; base = 7u; }
        if (m == 22u) { neutral = 0.0; base = 6u; }
        if (m == 23u) { base = 24u; }
        let c = src.rgb / src.a;
        let b = dst.rgb / da;
        let s = neutral + (c - vec3<f32>(neutral)) * sa;
        return vec4<f32>(sa * (1.0 - da) * c + da * vec3<f32>(channel(base, b.r, s.r), channel(base, b.g, s.g), channel(base, b.b, s.b)), oa);
    }
    if ((m == 18u || m == 19u) && da > 0.0) {
        let b = dst.rgb / da;
        let s = src.rgb / src.a;
        var mixed = b;
        if ((lum(s) < lum(b)) == (m == 18u)) { mixed = s; }
        return vec4<f32>(sa * (1.0 - da) * s + da * (1.0 - sa) * b + sa * da * mixed, oa);
    }
    if (m == 0u || da <= 0.0) { return vec4<f32>(src.rgb * op + dst.rgb * (1.0 - sa), oa); }
    let s = src.rgb / src.a;
    let b = dst.rgb / da;
    let mixed = vec3<f32>(channel(m, b.r, s.r), channel(m, b.g, s.g), channel(m, b.b, s.b));
    return vec4<f32>(sa * (1.0 - da) * s + da * (1.0 - sa) * b + sa * da * mixed, oa);
}
// id.x: 64 точки на плитку, id.y: строка
@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let t = id.x / 64u;
    if (t >= arrayLength(&tiles)) { return; }
    let lx = id.x % 64u;
    let ly = id.y;
    var c = p.bg;
    let r = tiles[t];
    for (var i = r.x; i < r.x + r.y; i++) {
        let e = entries[i];
        let at = vec2<u32>((e.slot % 64u) * 64u + lx, ((e.slot / 64u) % 64u) * 64u + ly);
        let v = textureLoad(atlas, at, e.slot / 4096u, 0);
        if (v.a == 0u) { continue; }
        c = composite(e.mode, c, vec4<f32>(v) * p.one, e.op);
    }
    // как to_rgba8(from_f32(..)): цвет не больше альфы, без предумножения
    let a = floor(clamp(c.a, 0.0, 1.0) * 32768.0 + 0.5);
    var px = 0u;
    if (a > 0.0) {
        let q = min(floor(clamp(c.rgb, vec3<f32>(0.0), vec3<f32>(1.0)) * 32768.0 + vec3<f32>(0.5)), vec3<f32>(a));
        let o = vec3<u32>(floor((q * 255.0 + vec3<f32>(floor(a / 2.0))) / a));
        px = o.r | (o.g << 8u) | (o.b << 16u) | (u32(floor((a * 255.0 + 16384.0) / 32768.0)) << 24u);
    }
    out[(t * 4096u + ly * 64u + lx)] = px;
}
"#;

/// Плиток в странице атласа (сторона 4096 точек).
const PER_PAGE: u32 = 4096;
/// Сколько плиток считается за одну отправку: длинная работа одним куском — сброс драйвера (TDR).
const CHUNK: usize = 256;

/// Отпечаток плитки: от него зависит, загружать ли её заново.
fn tile_hash(px: &[[u16; 4]]) -> u64 {
    let (mut a, mut b) = (0x9E37_79B9_7F4A_7C15u64, 0xC2B2_AE3D_27D4_EB4Fu64);
    for p in px {
        let w = p[0] as u64 | (p[1] as u64) << 16 | (p[2] as u64) << 32 | (p[3] as u64) << 48;
        a = (a ^ w).wrapping_mul(0xFF51_AFD7_ED55_8CCD).rotate_left(31);
        b = (b.wrapping_add(w)).wrapping_mul(0xC4CE_B9FE_1A85_EC53).rotate_left(29);
    }
    a ^ b.rotate_left(17)
}

pub struct Composer {
    device: std::sync::Arc<wgpu::Device>,
    queue: std::sync::Arc<wgpu::Queue>,
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    atlas: wgpu::Texture,
    atlas_view: wgpu::TextureView,
    /// Отпечаток → место в атласе и когда плитка нужна была в последний раз.
    slots: HashMap<u64, (u32, u64)>,
    free: Vec<u32>,
    frame: u64,
}

impl Composer {
    /// `pages` страниц по 134 МБ.
    pub fn new(device: std::sync::Arc<wgpu::Device>, queue: std::sync::Arc<wgpu::Queue>, pages: u32) -> Option<Self> {
        let limits = device.limits();
        if limits.max_texture_dimension_2d < 4096 || limits.max_storage_buffers_per_shader_stage < 3 {
            return None;
        }
        let pages = pages.min(limits.max_texture_array_layers).max(1);
        device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let atlas = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gpu_composite"),
            size: wgpu::Extent3d { width: 4096, height: 4096, depth_or_array_layers: pages },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Uint,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let atlas_view = atlas.create_view(&wgpu::TextureViewDescriptor { dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("gpu_composite"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::COMPUTE, ty, count: None };
        let storage = |read_only| wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only }, has_dynamic_offset: false, min_binding_size: None };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gpu_composite"),
            entries: &[
                entry(0, wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Uint, view_dimension: wgpu::TextureViewDimension::D2Array, multisampled: false }),
                entry(1, storage(true)),
                entry(2, storage(true)),
                entry(3, storage(false)),
                entry(4, wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }),
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("gpu_composite"), bind_group_layouts: &[&layout], push_constant_ranges: &[] });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("gpu_composite"),
            layout: Some(&pl),
            module: &module,
            entry_point: "main",
            compilation_options: Default::default(),
            cache: None,
        });
        let (e1, e2) = (pollster::block_on(device.pop_error_scope()), pollster::block_on(device.pop_error_scope()));
        if let Some(e) = e1.iter().chain(&e2).next() {
            eprintln!("сведение на видеокарте: {e}");
            return None;
        }
        Some(Composer { device, queue, pipeline, layout, atlas, atlas_view, slots: HashMap::new(), free: (0..pages * PER_PAGE).rev().collect(), frame: 0 })
    }

    /// Загрузить недостающее из `need` (отпечаток → пиксели). `false` — атлас мал, считает процессор.
    fn ensure(&mut self, need: &[(u64, &[[u16; 4]])]) -> bool {
        let frame = self.frame;
        for (h, px) in need {
            if let Some(s) = self.slots.get_mut(h) {
                s.1 = frame;
                continue;
            }
            if self.free.is_empty() {
                // плитки, не нужные этой пачке, уступают место (самые старые не ищем: дорого)
                let old: Vec<u64> = self.slots.iter().filter(|(_, v)| v.1 + 1 < frame).map(|(k, _)| *k).collect();
                for k in old {
                    if let Some((slot, _)) = self.slots.remove(&k) {
                        self.free.push(slot);
                    }
                }
            }
            let Some(slot) = self.free.pop() else { return false };
            self.slots.insert(*h, (slot, frame));
            let origin = wgpu::Origin3d { x: (slot % 64) * 64, y: ((slot / 64) % 64) * 64, z: slot / PER_PAGE };
            // SAFETY: [u16; 4] без промежутков, срез живёт весь вызов
            let bytes = unsafe { std::slice::from_raw_parts(px.as_ptr() as *const u8, px.len() * 8) };
            self.queue.write_texture(
                wgpu::ImageCopyTexture { texture: &self.atlas, mip_level: 0, origin, aspect: wgpu::TextureAspect::All },
                bytes,
                wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(64 * 8), rows_per_image: None },
                wgpu::Extent3d { width: 64, height: 64, depth_or_array_layers: 1 },
            );
        }
        true
    }

    /// Сведённые плитки `keys`: RGBA8 без предумножения, как `Document::render_tile`.
    pub fn render(&mut self, doc: &Document, keys: &[Key]) -> Option<Vec<(Key, Vec<u8>)>> {
        use wgpu::util::DeviceExt;
        let (bg, stack) = doc.gpu_stack()?;
        self.frame += 1;
        let timing = std::env::var_os("PAINT_TIMING").is_some();
        let t0 = std::time::Instant::now();
        // отпечатки — во всех потоках: это единственное, что приходится читать у каждой плитки
        struct Item<'a> {
            tile: usize,
            layer: usize,
            px: &'a [[u16; 4]],
            hash: u64,
        }
        let mut items: Vec<Item> = Vec::new();
        for (t, k) in keys.iter().enumerate() {
            for (l, (layer, _, _)) in stack.iter().enumerate() {
                if let Some(tile) = layer.pixels.tiles.get(k) {
                    items.push(Item { tile: t, layer: l, px: &tile.0, hash: 0 });
                }
            }
        }
        let threads = paint_core::threads().max(1);
        let part = items.len().div_ceil(threads).max(1);
        std::thread::scope(|s| {
            for chunk in items.chunks_mut(part) {
                s.spawn(move || {
                    for it in chunk {
                        it.hash = tile_hash(it.px);
                    }
                });
            }
        });
        let t1 = t0.elapsed();
        let need: Vec<(u64, &[[u16; 4]])> = items.iter().map(|i| (i.hash, i.px)).collect();
        self.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        if !self.ensure(&need) {
            let _ = pollster::block_on(self.device.pop_error_scope());
            let _ = pollster::block_on(self.device.pop_error_scope());
            return None;
        }
        self.queue.submit(None);
        let t2 = t0.elapsed();
        let mut ranges = vec![[0u32; 2]; keys.len()];
        let mut entries: Vec<[u32; 4]> = Vec::with_capacity(items.len());
        for it in &items {
            let r = &mut ranges[it.tile];
            if r[1] == 0 {
                r[0] = entries.len() as u32;
            }
            r[1] += 1;
            let (_, mode, op) = stack[it.layer];
            entries.push([self.slots[&it.hash].0, mode, op.to_bits(), 0]);
        }
        if entries.is_empty() {
            entries.push([0; 4]);
        }
        if entries.len() as u64 * 16 > self.device.limits().max_storage_buffer_binding_size as u64 {
            let _ = pollster::block_on(self.device.pop_error_scope());
            let _ = pollster::block_on(self.device.pop_error_scope());
            return None;
        }
        let mut params = Vec::with_capacity(48);
        for v in bg {
            params.extend_from_slice(&v.to_le_bytes());
        }
        params.extend_from_slice(&(1.0 / paint_core::pixel::ONE as f32).to_le_bytes());
        params.extend_from_slice(&[0u8; 12]);
        let uniform = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("gpu_composite"), contents: &params, usage: wgpu::BufferUsages::UNIFORM });
        let cast = |v: &[[u32; 4]]| v.iter().flatten().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
        let ebuf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("gpu_composite"), contents: &cast(&entries), usage: wgpu::BufferUsages::STORAGE });
        let tile_bytes = (TILE_LEN * 4) as u64;
        let mut out = Vec::with_capacity(keys.len());
        for (ci, chunk) in ranges.chunks(CHUNK).enumerate() {
            let rbytes: Vec<u8> = chunk.iter().flatten().flat_map(|x| x.to_le_bytes()).collect();
            let tbuf = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("gpu_composite"), contents: &rbytes, usage: wgpu::BufferUsages::STORAGE });
            let size = chunk.len() as u64 * tile_bytes;
            let obuf = self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("gpu_composite"), size, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
            let read = self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("gpu_composite"), size, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
            let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("gpu_composite"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&self.atlas_view) },
                    wgpu::BindGroupEntry { binding: 1, resource: tbuf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 2, resource: ebuf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 3, resource: obuf.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 4, resource: uniform.as_entire_binding() },
                ],
            });
            let mut enc = self.device.create_command_encoder(&Default::default());
            {
                let mut pass = enc.begin_compute_pass(&Default::default());
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &bind, &[]);
                pass.dispatch_workgroups(chunk.len() as u32 * 4, 4, 1);
            }
            enc.copy_buffer_to_buffer(&obuf, 0, &read, 0, size);
            let sub = self.queue.submit(Some(enc.finish()));
            let slice = read.slice(..);
            let (tx, rx) = std::sync::mpsc::channel();
            slice.map_async(wgpu::MapMode::Read, move |r| {
                let _ = tx.send(r.is_ok());
            });
            self.device.poll(wgpu::Maintain::WaitForSubmissionIndex(sub));
            if !rx.recv().unwrap_or(false) {
                let _ = pollster::block_on(self.device.pop_error_scope());
                let _ = pollster::block_on(self.device.pop_error_scope());
                return None;
            }
            let data = slice.get_mapped_range();
            for (i, part) in data.chunks(tile_bytes as usize).enumerate() {
                out.push((keys[ci * CHUNK + i], part.to_vec()));
            }
        }
        let (e1, e2) = (pollster::block_on(self.device.pop_error_scope()), pollster::block_on(self.device.pop_error_scope()));
        if let Some(e) = e1.iter().chain(&e2).next() {
            eprintln!("сведение на видеокарте: {e}");
            return None;
        }
        if timing {
            eprintln!("сведение на видеокарте: {} плиток, отпечатки {t1:?}, загрузка {:?}, счёт и чтение {:?}", keys.len(), t2 - t1, t0.elapsed() - t2);
        }
        Some(out)
    }
}

const _: () = assert!(TILE == 64);

/// С какой длины очереди плиток вообще пробовать видеокарту: при рисовании их единицы.
pub const MIN_QUEUE: usize = 512;

/// Плиток в пачке для видеокарты.
pub const BATCH: usize = 1024;

/// Кто быстрее, выясняется замером на этой машине и этой картинке: на сильном процессоре со
/// встроенной видеокартой выигрывает процессор (4K, 20 слоёв: 96 мс против 140 мс). Проигравшая
/// видеокарта освобождает память и больше не пробуется до конца сеанса.
#[derive(Default)]
pub struct Race {
    composer: Option<Composer>,
    off: bool,
    /// Время процессора на плитку в прошлой большой пачке, секунды.
    cpu_per_tile: Option<f64>,
    decided: bool,
}

impl Race {
    /// Нужно ли сейчас попробовать видеокарту при длине очереди `queue`.
    pub fn wanted(&self, queue: usize) -> bool {
        let forced = std::env::var("PAINT_GPU_COMPOSITE").as_deref() == Ok("force");
        !self.off && queue >= MIN_QUEUE && (forced || self.decided || self.cpu_per_tile.is_some())
    }

    /// Запомнить замер процессора на большой пачке.
    pub fn note_cpu(&mut self, per_tile: f64) {
        if !self.decided {
            self.cpu_per_tile = Some(per_tile);
        }
    }

    /// Свести на видеокарте; `None` — считает процессор.
    pub fn render(&mut self, doc: &Document, keys: &[Key]) -> Option<Vec<(Key, Vec<u8>)>> {
        if self.off || std::env::var("PAINT_GPU_COMPOSITE").as_deref() == Ok("off") {
            self.off = true;
            return None;
        }
        doc.gpu_stack()?;
        let forced = std::env::var("PAINT_GPU_COMPOSITE").as_deref() == Ok("force");
        if self.composer.is_none() {
            let Some(gpu) = crate::gpu_filter::current() else {
                self.off = true;
                return None;
            };
            let (_, stack) = doc.gpu_stack()?;
            let tiles: usize = stack.iter().map(|(l, _, _)| l.pixels.tiles.len()).sum();
            let pages = (tiles as u32 * 3 / 2 / PER_PAGE + 1).min(16);
            self.composer = Composer::new(gpu.device, gpu.queue, pages);
            if self.composer.is_none() {
                self.off = true;
                return None;
            }
        }
        let c = self.composer.as_mut()?;
        if self.decided || forced {
            let r = c.render(doc, keys);
            if r.is_none() {
                // не влезло или сбой — не занимать память заново на каждой пачке
                self.composer = None;
                self.off = !forced;
            }
            return r;
        }
        // проба: первый раз загружает плитки, сравнивается второй
        let first = c.render(doc, keys);
        let t = std::time::Instant::now();
        let second = first.and_then(|_| c.render(doc, keys));
        let per_tile = t.elapsed().as_secs_f64() / keys.len().max(1) as f64;
        let cpu = self.cpu_per_tile.unwrap_or(0.0);
        if std::env::var_os("PAINT_TIMING").is_some() {
            eprintln!("сведение: на плитку видеокарта {:.1} мкс, процессор {:.1} мкс", per_tile * 1e6, cpu * 1e6);
        }
        if second.is_some() && per_tile * 1.25 < cpu {
            self.decided = true;
            second
        } else {
            self.off = true;
            self.composer = None;
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use paint_core::{blend::BlendMode, layer::Layer, pixel::from_rgba8};

    fn composer() -> Option<Composer> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor { backends: wgpu::Backends::PRIMARY, ..Default::default() });
        let name = std::env::var("PAINT_GPU_TEST_NAME").unwrap_or_default();
        let adapter = instance.enumerate_adapters(wgpu::Backends::PRIMARY).into_iter().find(|a| a.get_info().name.contains(&name))?;
        eprintln!("устройство: {}", adapter.get_info().name);
        let (d, q) = pollster::block_on(adapter.request_device(&Default::default(), None)).ok()?;
        Composer::new(std::sync::Arc::new(d), std::sync::Arc::new(q), 16)
    }

    fn layer(seed: i32, w: i32, h: i32, mode: BlendMode, opacity: f32) -> Layer {
        let mut l = Layer::new("л");
        l.mode = mode;
        l.opacity = opacity;
        for y in 0..h {
            for x in 0..w {
                let a = ((x * 3 + y + seed * 37) % 256) as u8;
                if (x / 16 + y / 16 + seed) % 5 != 0 {
                    l.pixels.set(x, y, from_rgba8([(x * 5 + seed) as u8, (y * 7) as u8, (x + y + seed * 11) as u8, a]));
                }
            }
        }
        l
    }

    /// Все режимы на видеокарте — как на процессоре, с разной непрозрачностью и альфой.
    #[test]
    fn all_modes_match_cpu() {
        let Some(mut c) = composer() else { return };
        let keys: Vec<Key> = (0..2).flat_map(|y| (0..2).map(move |x| (x, y))).collect();
        for background in [Some([255, 255, 255]), Some([40, 90, 200])] {
            for (i, &mode) in BlendMode::ALL.iter().enumerate() {
                for opacity in [1.0, 0.45] {
                    let mut d = Document::new(128, 128);
                    d.background = background;
                    d.layers.push(layer(1, 128, 128, BlendMode::Normal, 0.8));
                    d.layers.push(layer(i as i32 + 2, 128, 128, mode, opacity));
                    d.layers.push(layer(99, 128, 128, BlendMode::Multiply, 0.3));
                    let got = c.render(&d, &keys).expect("считается");
                    let mut worst = 0;
                    for (k, px) in got {
                        let want = d.render_tile(k);
                        for (a, b) in px.iter().zip(&want) {
                            worst = worst.max((*a as i32 - *b as i32).abs());
                        }
                    }
                    assert!(worst <= 2, "{mode:?} {opacity}: разница {worst}");
                }
            }
        }
    }

    /// Смена содержимого плитки видна сразу, повторная загрузка не нужна для неизменного.
    #[test]
    fn changed_pixels_show_up() {
        let Some(mut c) = composer() else { return };
        let mut d = Document::new(64, 64);
        d.background = Some([255, 255, 255]);
        d.layers.push(layer(1, 64, 64, BlendMode::Normal, 1.0));
        let used = |c: &Composer| c.slots.len();
        let keys = [(0, 0)];
        let a = c.render(&d, &keys).unwrap();
        let n = used(&c);
        assert_eq!(c.render(&d, &keys).unwrap()[0].1, a[0].1);
        assert_eq!(used(&c), n, "то же содержимое не загружается заново");
        d.layers[0].pixels.set(5, 5, from_rgba8([1, 2, 3, 255]));
        let b = c.render(&d, &keys).unwrap();
        assert_eq!(b[0].1, d.render_tile((0, 0)), "новое содержимое");
        assert_ne!(b[0].1, a[0].1);
    }

    /// Замер: `cargo test --release -p paint-app composite_speed -- --ignored --nocapture`.
    #[test]
    #[ignore = "замер"]
    fn composite_speed() {
        let Some(mut c) = composer() else { return };
        let keys: Vec<Key> = (0..34).flat_map(|y| (0..60).map(move |x| (x, y))).collect();
        for n in [3, 20] {
            let mut d = Document::new(3840, 2160);
            d.background = Some([255, 255, 255]);
            for i in 0..n {
                d.layers.push(layer(i, 3840, 2160, if i % 3 == 1 { BlendMode::Multiply } else { BlendMode::Normal }, if i % 3 == 2 { 0.7 } else { 1.0 }));
            }
            for pass in ["первый", "второй"] {
                let t = std::time::Instant::now();
                let r = c.render(&d, &keys).map(|v| v.len());
                eprintln!("слоёв {n}, {pass}: {:?}, плиток {r:?}", t.elapsed());
            }
            let t = std::time::Instant::now();
            let next = std::sync::atomic::AtomicUsize::new(0);
            std::thread::scope(|s| {
                for _ in 0..paint_core::threads() {
                    s.spawn(|| loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(k) = keys.get(i) else { break };
                        std::hint::black_box(d.render_tile(*k));
                    });
                }
            });
            eprintln!("слоёв {n}, процессор: {:?}", t.elapsed());
        }
    }
}
