//! Позы предмета: файл позы (повороты суставов по именам — подходит и к другой модели с теми же
//! именами), отражение «левая ↔ правая» и «тянуть за кисть или стопу» (обратная кинематика).

use crate::model::Model;
use crate::scene::Object;
use glam::{Quat, Vec3};

/// Поворот меньше этого (по модулю `1 − |w|`) — как без поворота.
const IDENTITY_EPS: f32 = 1e-6;

/// Текст позы: строка `gspose 1`, затем `rot <x> <y> <z> <w> <имя узла>` и
/// `move <x> <y> <z> <имя узла>`. Имя — в конце строки: в нём бывают пробелы.
pub fn to_text(o: &Object, m: &Model) -> String {
    let mut s = String::from("gspose 1\n");
    for (&n, q) in &o.pose {
        if let Some(node) = m.nodes.get(n) {
            s += &format!("rot {} {} {} {} {}\n", q.x, q.y, q.z, q.w, node.name);
        }
    }
    for (&n, v) in &o.offsets {
        if let Some(node) = m.nodes.get(n) {
            s += &format!("move {} {} {} {}\n", v.x, v.y, v.z, node.name);
        }
    }
    s
}

/// Применить позу из текста: прежняя поза заменяется; узлы находятся по имени (без учёта
/// регистра, если точного нет). Возвращает, сколько строк позы нашли свой узел и сколько нет.
pub fn apply_text(o: &mut Object, m: &Model, text: &str) -> Result<(usize, usize), String> {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("gspose 1") {
        return Err("не файл позы GrazySAI".into());
    }
    let find = |name: &str| m.nodes.iter().position(|n| n.name == name).or_else(|| m.nodes.iter().position(|n| n.name.eq_ignore_ascii_case(name)));
    let (mut found, mut missed) = (0, 0);
    o.pose.clear();
    o.offsets.clear();
    for line in lines {
        let mut w = line.splitn(2, ' ');
        let (Some(kind), Some(rest)) = (w.next(), w.next()) else { continue };
        let n = match kind {
            "rot" => 4,
            "move" => 3,
            _ => continue,
        };
        let parts: Vec<&str> = rest.splitn(n + 1, ' ').collect();
        if parts.len() != n + 1 {
            continue;
        }
        let Some(v) = parts[..n].iter().map(|x| x.parse::<f32>().ok().filter(|v| v.is_finite())).collect::<Option<Vec<f32>>>() else { continue };
        let Some(i) = find(parts[n]) else {
            missed += 1;
            continue;
        };
        found += 1;
        if n == 4 {
            let q = Quat::from_xyzw(v[0], v[1], v[2], v[3]);
            if q.length() > 1e-6 {
                o.pose.insert(i, q.normalize());
            }
        } else {
            o.offsets.insert(i, Vec3::new(v[0], v[1], v[2]));
        }
    }
    Ok((found, missed))
}

/// Пары слов «левый — правый» в именах суставов (Mixamo, Blender, VRM, MMD и т. п.).
const SIDES: [(&str, &str); 12] = [
    ("Left", "Right"),
    ("left", "right"),
    ("LEFT", "RIGHT"),
    (".L", ".R"),
    (".l", ".r"),
    ("_L", "_R"),
    ("_l", "_r"),
    ("L_", "R_"),
    ("l_", "r_"),
    (" L", " R"),
    ("-L", "-R"),
    ("左", "右"),
];

/// Узел той же модели с именем другой стороны (сам узел — если он посередине).
pub fn counterpart(m: &Model, i: usize) -> usize {
    let name = &m.nodes[i].name;
    for (a, b) in SIDES {
        for (from, to) in [(a, b), (b, a)] {
            // каждое вхождение по очереди: у «L_UpperArm_L» сначала первое, потом второе
            for (at, _) in name.match_indices(from) {
                let other = format!("{}{}{}", &name[..at], to, &name[at + from.len()..]);
                if let Some(k) = m.nodes.iter().position(|n| n.name == other) {
                    return k;
                }
            }
        }
    }
    i
}

/// Повороты узлов в осях модели (без положения и поворота предмета): покой и с позой.
fn model_rotations(o: &Object, m: &Model, posed: bool) -> Vec<Quat> {
    let mut g = vec![Quat::IDENTITY; m.nodes.len()];
    let mut stack: Vec<(usize, Quat)> = m.roots.iter().map(|&r| (r, Quat::IDENTITY)).collect();
    while let Some((i, parent)) = stack.pop() {
        let extra = if posed { o.pose.get(&i).copied().unwrap_or(Quat::IDENTITY) } else { Quat::IDENTITY };
        g[i] = (parent * m.nodes[i].r * extra).normalize();
        stack.extend(m.nodes[i].children.iter().map(|&c| (c, g[i])));
    }
    g
}

/// Отражение поворота в плоскости x = 0.
fn reflect(q: Quat) -> Quat {
    Quat::from_xyzw(q.x, -q.y, -q.z, q.w)
}

/// Отразить позу «левая ↔ правая» относительно плоскости x = 0 модели: сустав левой руки
/// получает зеркальный поворот правой и наоборот, средние (позвоночник, голова) отражаются
/// сами в себя. Разные оси суставов в покое учитываются: в покое отражение ничего не меняет.
pub fn mirror(o: &mut Object, m: &Model) {
    let rest = model_rotations(o, m, false);
    let posed = model_rotations(o, m, true);
    let pair: Vec<usize> = (0..m.nodes.len()).map(|i| counterpart(m, i)).collect();
    // новые повороты в осях модели — сверху вниз, чтобы у родителя они уже были
    let mut new_g = vec![Quat::IDENTITY; m.nodes.len()];
    let mut pose = std::collections::BTreeMap::new();
    let mut stack: Vec<(usize, Quat)> = m.roots.iter().map(|&r| (r, Quat::IDENTITY)).collect();
    while let Some((k, parent)) = stack.pop() {
        let j = pair[k];
        let fix = reflect(rest[j]).inverse() * rest[k];
        let g = (reflect(posed[j]) * fix).normalize();
        new_g[k] = g;
        let local = parent.inverse() * g;
        let extra = (m.nodes[k].r.inverse() * local).normalize();
        if 1.0 - extra.w.abs() > IDENTITY_EPS {
            pose.insert(k, if extra.w < 0.0 { -extra } else { extra });
        }
        stack.extend(m.nodes[k].children.iter().map(|&c| (c, g)));
    }
    // сдвиги частей: к узлу другой стороны, по x — с обратным знаком
    let offsets = o.offsets.iter().map(|(&i, v)| (pair.get(i).copied().unwrap_or(i), Vec3::new(-v.x, v.y, v.z))).collect();
    o.pose = pose;
    o.offsets = offsets;
}

/// Цепочка для «тянуть за конец»: `len` суставов над `end` (у кисти — предплечье и плечо).
pub fn chain(m: &Model, end: usize, len: usize) -> Vec<usize> {
    let joints = m.nodes.iter().any(|n| n.joint);
    let mut out = Vec::new();
    let mut p = m.nodes[end].parent;
    while let Some(i) = p {
        if out.len() >= len {
            break;
        }
        if !joints || m.nodes[i].joint {
            out.push(i);
        }
        p = m.nodes[i].parent;
    }
    out
}

/// Довести узел `end` до точки `target` (в мире), поворачивая суставы цепочки `chain` (снизу
/// вверх) — метод CCD: каждый сустав по очереди поворачивается так, чтобы конец смотрел на цель.
/// Возвращает оставшееся расстояние.
pub fn reach(o: &mut Object, m: &Model, end: usize, chain: &[usize], target: Vec3) -> f32 {
    let pos = |o: &Object| -> Vec<Vec3> { o.globals(m).iter().map(|g| g.w_axis.truncate()).collect() };
    let mut p = pos(o);
    for _ in 0..24 {
        if (p[end] - target).length() < 1e-4 {
            break;
        }
        for &j in chain {
            let (to_end, to_target) = (p[end] - p[j], target - p[j]);
            if to_end.length() < 1e-6 || to_target.length() < 1e-6 {
                continue;
            }
            let world = Quat::from_rotation_arc(to_end.normalize(), to_target.normalize());
            let g = o.globals(m);
            let parent_rot = m.nodes[j].parent.map_or(o.rot, |pi| g[pi].to_scale_rotation_translation().1);
            let rest = m.nodes[j].r;
            let local = rest * o.pose.get(&j).copied().unwrap_or(Quat::IDENTITY);
            let new_local = parent_rot.inverse() * world * parent_rot * local;
            o.pose.insert(j, (rest.inverse() * new_local).normalize());
            p = pos(o);
        }
    }
    (p[end] - target).length()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Node;

    fn node(name: &str, parent: Option<usize>, children: Vec<usize>, t: Vec3, r: Quat) -> Node {
        Node { name: name.into(), parent, children, t, r, s: Vec3::ONE, mesh: None, skin: None, joint: true }
    }

    /// Туловище и две руки: плечо и предплечье с каждой стороны; оси левой и правой — зеркальны.
    fn body() -> Model {
        let tilt = Quat::from_rotation_z(0.3);
        let nodes = vec![
            node("Hips", None, vec![1, 3], Vec3::ZERO, Quat::IDENTITY),
            node("LeftArm", Some(0), vec![2], Vec3::new(1.0, 0.0, 0.0), tilt),
            node("LeftForeArm", Some(1), vec![], Vec3::new(1.0, 0.0, 0.0), Quat::IDENTITY),
            node("RightArm", Some(0), vec![4], Vec3::new(-1.0, 0.0, 0.0), reflect(tilt)),
            node("RightForeArm", Some(3), vec![], Vec3::new(-1.0, 0.0, 0.0), Quat::IDENTITY),
        ];
        Model { nodes, roots: vec![0], ..Default::default() }
    }

    #[test]
    fn finds_the_other_side() {
        let m = body();
        assert_eq!(counterpart(&m, 1), 3);
        assert_eq!(counterpart(&m, 4), 2);
        assert_eq!(counterpart(&m, 0), 0);
    }

    #[test]
    fn mirror_swaps_sides() {
        let m = body();
        let mut o = Object::new(1);
        o.pose.insert(1, Quat::from_rotation_y(0.7) * Quat::from_rotation_x(0.2));
        o.pose.insert(0, Quat::from_rotation_y(0.4));
        let before: Vec<Vec3> = o.globals(&m).iter().map(|g| g.w_axis.truncate()).collect();
        mirror(&mut o, &m);
        let after: Vec<Vec3> = o.globals(&m).iter().map(|g| g.w_axis.truncate()).collect();
        // каждая точка — зеркало точки другой стороны
        for (i, j) in [(0, 0), (1, 3), (2, 4), (3, 1), (4, 2)] {
            let want = before[j] * Vec3::new(-1.0, 1.0, 1.0);
            assert!((after[i] - want).length() < 1e-4, "{i}: {:?} ≠ {want:?}", after[i]);
        }
        // дважды — как было
        mirror(&mut o, &m);
        let back: Vec<Vec3> = o.globals(&m).iter().map(|g| g.w_axis.truncate()).collect();
        for i in 0..5 {
            assert!((back[i] - before[i]).length() < 1e-4);
        }
        // в покое отражение ничего не меняет
        let mut rest = Object::new(1);
        mirror(&mut rest, &m);
        assert!(rest.pose.is_empty(), "{:?}", rest.pose);
    }

    #[test]
    fn pose_file_round_trip() {
        let m = body();
        let mut o = Object::new(1);
        o.pose.insert(2, Quat::from_rotation_x(0.5));
        o.offsets.insert(1, Vec3::new(0.1, 0.0, 0.0));
        let t = to_text(&o, &m);
        let mut p = Object::new(2);
        assert_eq!(apply_text(&mut p, &m, &t), Ok((2, 0)));
        assert_eq!(p.pose.keys().copied().collect::<Vec<_>>(), vec![2]);
        assert!(p.pose[&2].angle_between(o.pose[&2]) < 1e-5);
        assert!(apply_text(&mut p, &m, "что-то").is_err());
        // строки с чужими именами не мешают
        assert_eq!(apply_text(&mut p, &m, "gspose 1\nrot 0 0 0 1 Tail\nrot 0 0 0 1 hips\nmove x 1 2 Hips"), Ok((1, 1)));
    }

    #[test]
    fn reach_moves_the_hand() {
        let m = body();
        let mut o = Object::new(1);
        let c = chain(&m, 2, 2);
        assert_eq!(c, vec![1, 0]);
        let target = Vec3::new(1.2, 1.1, 0.3);
        let left = reach(&mut o, &m, 2, &c[..1], target);
        // одной костью длиной 1 от плеча (1, 0, 0) до цели не дотянуться — ближе, чем было
        assert!(left < 0.5, "{left}");
        let reachable = Vec3::new(1.5, 1.0, 0.5);
        let d = reach(&mut o, &m, 2, &c, reachable);
        assert!(d < 1e-2, "{d}");
    }
}
