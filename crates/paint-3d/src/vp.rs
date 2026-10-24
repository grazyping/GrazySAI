//! Камера по точкам схода перспективной линейки: горизонт даёт наклон горизонта и наклон
//! камеры, две точки схода (направления под прямым углом) — угол обзора и поворот, одна — поворот
//! при прежнем угле обзора. Середина кадра — середина холста.

use crate::scene::Camera;
use glam::Vec2;

/// Камера, у которой оси X и Z мира сходятся в точках `vps` холста `w`×`h` (первая — ось X; у
/// одной точки — ось Z). Цель и расстояние — прежние. `None` — точки не годятся (две точки по
/// одну сторону от середины, совпадают и т. п.).
pub fn camera_from_vps(cam: &Camera, w: u32, h: u32, vps: &[[f32; 2]]) -> Option<Camera> {
    let c = Vec2::new(w as f32 / 2.0, h as f32 / 2.0);
    let v: Vec<Vec2> = vps.iter().map(|p| Vec2::new(p[0], p[1]) - c).collect();
    let mut out = *cam;
    // горизонт и его наклон; у одной точки — по горизонтали
    let dir = if v.len() >= 2 { (v[1] - v[0]).normalize_or_zero() } else { Vec2::X };
    if dir == Vec2::ZERO {
        return None;
    }
    let dir = if dir.x < 0.0 { -dir } else { dir };
    // экран: y вниз; «вверх» по кадру после поворота горизонта
    let up = Vec2::new(dir.y, -dir.x);
    let roll = dir.y.atan2(dir.x);
    // расстояние от середины до горизонта вверх и места точек вдоль горизонта
    let d = v[0].dot(up);
    let u: Vec<f32> = v.iter().map(|p| p.dot(dir)).collect();
    let f = if v.len() >= 2 {
        let ff = -(u[0] * u[1] + d * d);
        if ff <= 1.0 {
            return None;
        }
        ff.sqrt()
    } else {
        (h as f32 / 2.0) / (cam.fov.to_radians() / 2.0).tan()
    };
    out.fov = (2.0 * ((h as f32 / 2.0) / f).atan()).to_degrees().clamp(1.0, 150.0);
    // горизонт выше середины — камера смотрит вниз
    out.pitch = d.atan2(f).to_degrees().clamp(-89.0, 89.0);
    let alpha = u[0].atan2(f.hypot(d));
    let yaw = if v.len() >= 2 { alpha - std::f32::consts::FRAC_PI_2 } else { alpha };
    out.yaw = (yaw.to_degrees() + 540.0).rem_euclid(360.0) - 180.0;
    out.roll = roll.to_degrees();
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::Scene;
    use glam::Vec3;

    fn vp(s: &Scene, d: Vec3) -> [f32; 2] {
        let p = crate::render::project(s, 1200, 800, s.camera.eye() + d * 1e5).unwrap();
        [p.x, p.y]
    }

    #[test]
    fn two_points_give_back_the_camera() {
        for (yaw, pitch, fov, roll) in [(30.0, 12.0, 40.0, 0.0), (-50.0, -8.0, 60.0, 5.0), (70.0, 25.0, 30.0, -7.0)] {
            let mut s = Scene::default();
            s.camera.yaw = yaw;
            s.camera.pitch = pitch;
            s.camera.fov = fov;
            s.camera.roll = roll;
            // ось X мира — та из двух сторон, что видна впереди
            let fwd = s.camera.target - s.camera.eye();
            let x = if fwd.dot(Vec3::X) > 0.0 { Vec3::X } else { Vec3::NEG_X };
            let z = if fwd.dot(Vec3::Z) > 0.0 { Vec3::Z } else { Vec3::NEG_Z };
            let got = camera_from_vps(&Camera::default(), 1200, 800, &[vp(&s, x), vp(&s, z)]).unwrap();
            // поворот — с точностью до четверти оборота (какую ось считать X, линейка не знает)
            let dy = ((got.yaw - yaw) / 90.0).round() * 90.0 - (got.yaw - yaw);
            assert!(dy.abs() < 0.05, "{yaw} {pitch}: поворот {}", got.yaw);
            assert!((got.pitch - pitch).abs() < 0.05, "наклон {} ≠ {pitch}", got.pitch);
            assert!((got.fov - fov).abs() < 0.05, "обзор {} ≠ {fov}", got.fov);
            assert!((got.roll - roll).abs() < 0.05, "горизонт {} ≠ {roll}", got.roll);
        }
    }

    #[test]
    fn one_point_keeps_the_fov() {
        let mut s = Scene::default();
        s.camera.yaw = 15.0;
        s.camera.pitch = 10.0;
        let fwd = s.camera.target - s.camera.eye();
        let z = if fwd.dot(Vec3::Z) > 0.0 { Vec3::Z } else { Vec3::NEG_Z };
        let got = camera_from_vps(&s.camera, 1200, 800, &[vp(&s, z)]).unwrap();
        assert!((got.pitch - 10.0).abs() < 0.05, "{}", got.pitch);
        let dy = ((got.yaw - 15.0) / 180.0).round() * 180.0 - (got.yaw - 15.0);
        assert!(dy.abs() < 0.05, "{}", got.yaw);
        assert_eq!(got.fov, s.camera.fov);
        // точки по одну сторону от середины — не годятся
        assert!(camera_from_vps(&s.camera, 1200, 800, &[[700.0, 300.0], [900.0, 300.0]]).is_none());
    }
}
