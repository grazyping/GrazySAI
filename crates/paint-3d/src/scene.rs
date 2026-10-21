//! Сцена слоя 3D: предметы (модель, положение, поза, морфы, скрытые части), камера, свет и вид.
//! Хранится текстом (строка на свойство), чтобы файл читался и через годы, а неизвестное
//! пропускалось.

use crate::model::Model;
use glam::{Mat4, Quat, Vec3};
use std::collections::{BTreeMap, BTreeSet};

/// Как показывать модели.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Display {
    /// С освещением и цветом (текстурой) материала.
    #[default]
    Lit,
    /// Серая фигура со светом — без цвета материалов (удобно срисовывать форму).
    Clay,
    /// Силуэт одним цветом.
    Silhouette,
    /// Только контур (очертания и сгибы).
    Outline,
}

impl Display {
    pub const ALL: [Display; 4] = [Display::Lit, Display::Clay, Display::Silhouette, Display::Outline];

    pub fn key(self) -> &'static str {
        match self {
            Display::Lit => "lit",
            Display::Clay => "clay",
            Display::Silhouette => "silhouette",
            Display::Outline => "outline",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// Точка, вокруг которой вращается камера.
    pub target: Vec3,
    /// Поворот вокруг вертикали и наклон, градусы.
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    /// Угол обзора по вертикали, градусы (меньше — «телеобъектив», больше — «широкий угол»).
    pub fov: f32,
    /// Наклон горизонта, градусы.
    pub roll: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self { target: Vec3::new(0.0, 1.0, 0.0), yaw: 20.0, pitch: 10.0, distance: 4.0, fov: 35.0, roll: 0.0 }
    }
}

impl Camera {
    pub fn eye(&self) -> Vec3 {
        let (y, p) = (self.yaw.to_radians(), self.pitch.to_radians());
        self.target + self.distance * Vec3::new(p.cos() * y.sin(), p.sin(), p.cos() * y.cos())
    }

    pub fn view(&self) -> Mat4 {
        let fwd = (self.target - self.eye()).normalize_or(Vec3::NEG_Z);
        let up = Quat::from_axis_angle(fwd, -self.roll.to_radians()) * Vec3::Y;
        glam::camera::rh::view::look_at_mat4(self.eye(), self.target, up)
    }

    /// Проекция; ближняя и дальняя плоскости — по размеру сцены `radius`.
    pub fn proj(&self, aspect: f32, radius: f32) -> Mat4 {
        let near = (self.distance - radius).max(self.distance * 0.002).max(1e-4);
        let far = self.distance + radius * 2.0 + 1.0;
        glam::camera::rh::proj::directx::perspective(self.fov.clamp(1.0, 150.0).to_radians(), aspect.max(1e-3), near, far)
    }
}

/// Предмет сцены — одна загруженная модель.
#[derive(Clone, Debug, PartialEq)]
pub struct Object {
    /// Номер модели (`Asset3d::id`).
    pub asset: u64,
    pub pos: Vec3,
    pub rot: Quat,
    pub scale: f32,
    /// Поворот сустава (узла) сверх положения в покое.
    pub pose: BTreeMap<usize, Quat>,
    /// Сдвиг узла (части сцены) сверх положения в покое.
    pub offsets: BTreeMap<usize, Vec3>,
    /// Веса морфов: (сетка, номер морфа) → вес.
    pub morphs: BTreeMap<(usize, usize), f32>,
    /// Скрытые узлы (с вложенными).
    pub hidden: BTreeSet<usize>,
    pub visible: bool,
}

impl Object {
    pub fn new(asset: u64) -> Self {
        Self { asset, pos: Vec3::ZERO, rot: Quat::IDENTITY, scale: 1.0, pose: BTreeMap::new(), offsets: BTreeMap::new(), morphs: BTreeMap::new(), hidden: BTreeSet::new(), visible: true }
    }

    pub fn matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(Vec3::splat(self.scale), self.rot, self.pos)
    }

    /// Мировые матрицы всех узлов модели в этой позе.
    pub fn globals(&self, m: &Model) -> Vec<Mat4> {
        let mut g = vec![Mat4::IDENTITY; m.nodes.len()];
        let root = self.matrix();
        let mut stack: Vec<(usize, Mat4)> = m.roots.iter().map(|&r| (r, root)).collect();
        while let Some((i, parent)) = stack.pop() {
            let n = &m.nodes[i];
            let r = n.r * self.pose.get(&i).copied().unwrap_or(Quat::IDENTITY);
            let t = n.t + self.offsets.get(&i).copied().unwrap_or(Vec3::ZERO);
            g[i] = parent * Mat4::from_scale_rotation_translation(n.s, r, t);
            stack.extend(n.children.iter().map(|&c| (c, g[i])));
        }
        g
    }

    /// Узлы, скрытые сами или через родителя.
    pub fn hidden_nodes(&self, m: &Model) -> Vec<bool> {
        let mut h = vec![false; m.nodes.len()];
        let mut stack: Vec<(usize, bool)> = m.roots.iter().map(|&r| (r, false)).collect();
        while let Some((i, up)) = stack.pop() {
            h[i] = up || self.hidden.contains(&i);
            stack.extend(m.nodes[i].children.iter().map(|&c| (c, h[i])));
        }
        h
    }

    pub fn morph(&self, m: &Model, mesh: usize, k: usize) -> f32 {
        self.morphs.get(&(mesh, k)).copied().unwrap_or_else(|| m.meshes[mesh].weights.get(k).copied().unwrap_or(0.0))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub objects: Vec<Object>,
    pub camera: Camera,
    /// Направление света относительно камеры: поворот и наклон, градусы.
    pub light: [f32; 2],
    pub display: Display,
    /// Цвет силуэта и контура.
    pub ink: [u8; 3],
    /// Толщина контура, px.
    pub line: f32,
}

impl Default for Scene {
    fn default() -> Self {
        Self { objects: Vec::new(), camera: Camera::default(), light: [-35.0, 40.0], display: Display::Lit, ink: [60, 60, 70], line: 1.5 }
    }
}

fn q_text(q: Quat) -> String {
    format!("{} {} {} {}", q.x, q.y, q.z, q.w)
}

fn v_text(v: Vec3) -> String {
    format!("{} {} {}", v.x, v.y, v.z)
}

impl Scene {
    pub fn to_text(&self) -> String {
        let c = &self.camera;
        let mut s = String::from("gs3d 1\n");
        s += &format!("camera {} {} {} {} {} {}\n", v_text(c.target), c.yaw, c.pitch, c.distance, c.fov, c.roll);
        s += &format!("light {} {}\n", self.light[0], self.light[1]);
        s += &format!("display {}\n", self.display.key());
        s += &format!("ink {} {} {} {}\n", self.ink[0], self.ink[1], self.ink[2], self.line);
        for o in &self.objects {
            s += &format!("object {} {} {} {} {}\n", o.asset, v_text(o.pos), q_text(o.rot), o.scale, o.visible as u8);
            for (n, q) in &o.pose {
                s += &format!("pose {n} {}\n", q_text(*q));
            }
            for (n, v) in &o.offsets {
                s += &format!("move {n} {}\n", v_text(*v));
            }
            for ((m, k), w) in &o.morphs {
                s += &format!("morph {m} {k} {w}\n");
            }
            for n in &o.hidden {
                s += &format!("hide {n}\n");
            }
        }
        s
    }

    /// Прочитать текст сцены; неизвестные и испорченные строки пропускаются.
    pub fn parse(text: &str) -> Scene {
        let mut s = Scene::default();
        for line in text.lines() {
            let w: Vec<&str> = line.split_whitespace().collect();
            let f = |i: usize| w.get(i).and_then(|x| x.parse::<f32>().ok()).filter(|v| v.is_finite());
            let u = |i: usize| w.get(i).and_then(|x| x.parse::<usize>().ok()).filter(|&v| v < 1 << 24);
            let v3 = |i: usize| Some(Vec3::new(f(i)?, f(i + 1)?, f(i + 2)?));
            let q4 = |i: usize| Some(Quat::from_xyzw(f(i)?, f(i + 1)?, f(i + 2)?, f(i + 3)?).normalize()).filter(|q| q.is_finite());
            match w.first().copied() {
                Some("camera") => {
                    if let (Some(t), Some(yaw), Some(pitch), Some(d), Some(fov), Some(roll)) = (v3(1), f(4), f(5), f(6), f(7), f(8)) {
                        s.camera = Camera { target: t, yaw, pitch: pitch.clamp(-89.9, 89.9), distance: d.max(1e-4), fov: fov.clamp(1.0, 150.0), roll };
                    }
                }
                Some("light") => {
                    if let (Some(a), Some(b)) = (f(1), f(2)) {
                        s.light = [a, b];
                    }
                }
                Some("display") => {
                    if let Some(d) = Display::ALL.into_iter().find(|d| Some(d.key()) == w.get(1).copied()) {
                        s.display = d;
                    }
                }
                Some("ink") => {
                    if let (Some(r), Some(g), Some(b)) = (u(1), u(2), u(3)) {
                        s.ink = [r.min(255) as u8, g.min(255) as u8, b.min(255) as u8];
                    }
                    if let Some(l) = f(4) {
                        s.line = l.clamp(0.5, 20.0);
                    }
                }
                Some("object") => {
                    if let Some(id) = w.get(1).and_then(|x| x.parse::<u64>().ok()) {
                        let mut o = Object::new(id);
                        o.pos = v3(2).unwrap_or(Vec3::ZERO);
                        o.rot = q4(5).unwrap_or(Quat::IDENTITY);
                        o.scale = f(9).filter(|&x| x > 0.0).unwrap_or(1.0);
                        o.visible = w.get(10) != Some(&"0");
                        s.objects.push(o);
                    }
                }
                Some(k) => {
                    let Some(o) = s.objects.last_mut() else { continue };
                    match k {
                        "pose" => {
                            if let (Some(n), Some(q)) = (u(1), q4(2)) {
                                o.pose.insert(n, q);
                            }
                        }
                        "move" => {
                            if let (Some(n), Some(v)) = (u(1), v3(2)) {
                                o.offsets.insert(n, v);
                            }
                        }
                        "morph" => {
                            if let (Some(m), Some(i), Some(x)) = (u(1), u(2), f(3)) {
                                o.morphs.insert((m, i), x.clamp(-10.0, 10.0));
                            }
                        }
                        "hide" => {
                            if let Some(n) = u(1) {
                                o.hidden.insert(n);
                            }
                        }
                        _ => {}
                    }
                }
                None => {}
            }
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_round_trip_and_junk() {
        let mut s = Scene::default();
        let mut o = Object::new(77);
        o.pos = Vec3::new(1.0, 2.0, 3.0);
        o.pose.insert(5, Quat::from_rotation_z(0.5));
        o.offsets.insert(2, Vec3::X);
        o.morphs.insert((0, 1), 0.75);
        o.hidden.insert(9);
        s.objects.push(o);
        s.display = Display::Outline;
        s.camera.yaw = 123.0;
        let back = Scene::parse(&s.to_text());
        assert_eq!(back.objects.len(), 1);
        assert_eq!(back.display, Display::Outline);
        assert!((back.camera.yaw - 123.0).abs() < 1e-4);
        assert!(back.objects[0].pose[&5].abs_diff_eq(Quat::from_rotation_z(0.5), 1e-5));
        assert_eq!(back.objects[0].morphs[&(0, 1)], 0.75);
        assert!(back.objects[0].hidden.contains(&9));
        // мусор не роняет
        let junk = Scene::parse("camera nan 1 2\npose 1 2 3\nobject x\nobject 5 1 1 1 0 0 0 0 -3\nmorph 0 0 inf\nhide 99999999999\n\u{0}\u{1}");
        assert_eq!(junk.objects.len(), 1);
        assert_eq!(junk.objects[0].scale, 1.0);
        assert!(junk.objects[0].rot.is_finite());
    }
}
