use eframe::egui::{self, Color32, Pos2, Rect, Vec2};

use super::atmosphere::{hash, texture, uv};
use super::weather_transition::{Motion, WeatherMix};
use crate::weather::{Sunlight, WeatherKind};

pub(super) struct Sky {
    cloud: egui::TextureHandle,
    glow: egui::TextureHandle,
    moon: egui::TextureHandle,
}

impl Sky {
    pub fn new(ctx: &egui::Context) -> Self {
        let cloud = texture(ctx, "weather-cloud-volume", [256, 128], |x, y| {
            let mut density = 0.0;
            for octave in 0..5 {
                let scale = 2.0_f32.powi(octave);
                density += noise((x + 2.0) * 2.8 * scale, (y + 2.0) * 2.0 * scale) / scale;
            }
            let edge = ((1.0 - x * x) * (1.0 - y * y)).max(0.0);
            ((density - 0.45).max(0.0) * edge.powf(0.7) * 245.0).min(255.0) as u8
        });
        let glow = texture(ctx, "weather-light-halo", [128, 128], |x, y| {
            let radius = x * x + y * y;
            ((-radius * 4.0).exp() * (1.0 - radius).max(0.0) * 255.0) as u8
        });
        let mut moon = egui::ColorImage::filled([96, 96], Color32::TRANSPARENT);
        for y in 0..96 {
            for x in 0..96 {
                let nx = (x as f32 - 47.5) / 46.0;
                let ny = (y as f32 - 47.5) / 46.0;
                let radius = nx * nx + ny * ny;
                if radius < 1.0 {
                    let z = (1.0 - radius).sqrt();
                    let light = (nx * -0.6 + z * 0.75 + 0.2).clamp(0.08, 1.0);
                    let rock = 0.72 + 0.28 * noise(x as f32 * 0.13, y as f32 * 0.13);
                    let value = (255.0 * light * rock) as u8;
                    moon[(x, y)] = Color32::from_rgba_unmultiplied(
                        value,
                        value,
                        255_u8.saturating_sub((255 - value) / 2),
                        ((1.0 - radius) * 18.0).min(1.0).mul_add(255.0, 0.0) as u8,
                    );
                }
            }
        }
        Self {
            cloud,
            glow,
            moon: ctx.load_texture("weather-moon", moon, egui::TextureOptions::LINEAR),
        }
    }

    pub fn paint(
        &self,
        p: &egui::Painter,
        rect: Rect,
        motion: Motion,
        sun: Sunlight,
        mix: WeatherMix,
    ) {
        let cover = mix.cover();
        let time = motion.time;
        let rgb = Color32::from_rgb;
        let color = |night: Color32, day: Color32, sunset: Color32| {
            night
                .lerp_to_gamma(day, sun.daylight)
                .lerp_to_gamma(sunset, sun.warmth * (1.0 - cover * 0.55))
        };
        let top = color(rgb(5, 10, 28), rgb(37, 114, 192), rgb(48, 40, 93));
        let middle = color(rgb(16, 29, 58), rgb(91, 177, 229), rgb(191, 99, 122));
        let horizon = color(rgb(30, 48, 74), rgb(188, 218, 234), rgb(255, 179, 112));
        let overcast_day =
            rgb(130, 147, 160).lerp_to_gamma(rgb(72, 87, 108), mix.weight(WeatherKind::Storm));
        let overcast = rgb(18, 27, 44).lerp_to_gamma(overcast_day, sun.daylight);
        gradient(
            p,
            rect,
            &[
                (0.0, top.lerp_to_gamma(overcast, cover * 0.85)),
                (0.56, middle.lerp_to_gamma(overcast, cover * 0.9)),
                (0.88, horizon.lerp_to_gamma(overcast, cover * 0.95)),
                (
                    1.0,
                    color(rgb(9, 18, 35), rgb(84, 136, 161), rgb(104, 84, 113)),
                ),
            ],
        );
        self.stars(
            p,
            rect,
            time,
            (1.0 - sun.daylight).powi(4) * (1.0 - cover * 0.92),
        );
        self.celestial(p, rect, time, sun, cover);
        self.clouds(p, rect, motion, sun, cover);
    }

    fn celestial(&self, p: &egui::Painter, rect: Rect, time: f32, sun: Sunlight, cover: f32) {
        let scale = rect.height();
        let center = rect.min
            + rect.size()
                * Vec2::new(
                    0.18 + 0.64 * ((sun.azimuth - 60.0) / 240.0).clamp(0.0, 1.0),
                    (0.86 - sun.elevation.to_radians().sin() * 0.78).clamp(0.12, 1.1),
                );
        let visibility = sun.daylight * (1.0 - cover).powi(2);
        let gold = Color32::from_rgb(255, 238, 194)
            .lerp_to_gamma(Color32::from_rgb(255, 149, 89), sun.warmth);
        let mut mesh = egui::Mesh::default();
        for i in 0..if visibility > 0.002 { 7 } else { 0 } {
            let a = i as f32 * 0.9 + time * 0.018;
            let direction = Vec2::angled(a);
            let side = direction.rot90() * scale * 0.04;
            let end = center + direction * scale * 0.8;
            let index = mesh.vertices.len() as u32;
            mesh.colored_vertex(
                center,
                gold.gamma_multiply(visibility * (0.09 + 0.025 * (time * 0.8 + a).sin())),
            );
            mesh.colored_vertex(end - side, Color32::TRANSPARENT);
            mesh.colored_vertex(end + side, Color32::TRANSPARENT);
            mesh.add_triangle(index, index + 1, index + 2);
        }
        if !mesh.vertices.is_empty() {
            p.add(mesh);
        }
        p.image(
            self.glow.id(),
            Rect::from_center_size(center, Vec2::splat(scale * 0.95)),
            uv(),
            gold.gamma_multiply(visibility * 0.65),
        );
        let horizon_clip = Rect::from_min_max(
            rect.min,
            Pos2::new(rect.right(), rect.top() + rect.height() * 0.89),
        );
        p.with_clip_rect(p.clip_rect().intersect(horizon_clip))
            .circle_filled(center, scale * 0.034, gold.gamma_multiply(visibility));
        let moon = rect.min + rect.size() * Vec2::new(0.78, 0.23);
        let moonlight = (1.0 - sun.daylight).powi(2) * (1.0 - cover).powi(2);
        p.image(
            self.glow.id(),
            Rect::from_center_size(moon, Vec2::splat(scale * 0.65)),
            uv(),
            Color32::from_rgb(146, 183, 242).gamma_multiply(moonlight * 0.24),
        );
        p.image(
            self.moon.id(),
            Rect::from_center_size(moon, Vec2::splat(scale * 0.085)),
            uv(),
            Color32::WHITE.gamma_multiply(moonlight),
        );
    }

    fn clouds(&self, p: &egui::Painter, rect: Rect, motion: Motion, sun: Sunlight, cover: f32) {
        let time = motion.time;
        let tint = Color32::from_rgb(83, 107, 154)
            .lerp_to_gamma(Color32::from_rgb(240, 245, 247), sun.daylight)
            .lerp_to_gamma(Color32::from_rgb(249, 173, 145), sun.warmth * 0.6);
        let mut mesh = egui::Mesh::with_texture(self.cloud.id());
        for i in 0..12 {
            let depth = (i / 4) as f32;
            let width = rect.width() * (0.70 - cover * 0.25 + depth * 0.10);
            let period = rect.width() + width;
            let drift = (motion.cloud + time * depth * 10.0) * (rect.width() / 1200.0);
            let x = (hash(i, 43) * period + drift).rem_euclid(period) - width * 0.5;
            let y = rect.height() * (0.08 + hash(i, 59) * 0.56)
                + (time * 0.15 + i as f32).sin() * rect.height() * 0.015;
            let height = rect.height() * (0.20 + cover * 0.20 + depth * 0.07);
            let cloud =
                Rect::from_center_size(rect.min + Vec2::new(x, y), Vec2::new(width, height));
            mesh.add_rect_with_uv(
                cloud.translate(Vec2::new(0.0, height * 0.16)),
                uv(),
                Color32::from_rgb(31, 47, 73).gamma_multiply(cover * 0.23),
            );
            mesh.add_rect_with_uv(cloud, uv(), tint.gamma_multiply(0.16 + cover * 0.58));
        }
        p.add(mesh);
    }

    fn stars(&self, p: &egui::Painter, rect: Rect, time: f32, opacity: f32) {
        if opacity < 0.002 {
            return;
        }
        let mut mesh = egui::Mesh::with_texture(self.glow.id());
        for i in 0..95 {
            let center = rect.min + rect.size() * Vec2::new(hash(i, 83), hash(i, 97) * 0.82);
            let radius = 0.6 + hash(i, 103) * 1.2;
            let alpha = opacity * (0.5 + 0.25 * (time * (0.8 + hash(i, 13)) + i as f32).sin());
            mesh.add_rect_with_uv(
                Rect::from_center_size(center, Vec2::splat(radius * 3.0)),
                uv(),
                Color32::from_rgb(216, 230, 255).gamma_multiply(alpha),
            );
            if i % 5 == 0 {
                mesh.add_rect_with_uv(
                    Rect::from_center_size(center, Vec2::splat(radius * 10.0)),
                    uv(),
                    Color32::WHITE.gamma_multiply(alpha * 0.35),
                );
            }
        }
        p.add(mesh);
    }
}

pub(super) fn gradient(p: &egui::Painter, rect: Rect, stops: &[(f32, Color32)]) {
    let mut mesh = egui::Mesh::default();
    for &(position, color) in stops {
        let y = rect.top() + rect.height() * position;
        mesh.colored_vertex(Pos2::new(rect.left(), y), color);
        mesh.colored_vertex(Pos2::new(rect.right(), y), color);
    }
    for i in 0..stops.len().saturating_sub(1) as u32 {
        mesh.add_triangle(i * 2, i * 2 + 1, i * 2 + 2);
        mesh.add_triangle(i * 2 + 1, i * 2 + 3, i * 2 + 2);
    }
    p.add(mesh);
}

fn noise(x: f32, y: f32) -> f32 {
    let ix = x.floor() as u32;
    let iy = y.floor() as u32;
    let smooth = |v: f32| v * v * (3.0 - 2.0 * v);
    let (fx, fy) = (smooth(x.fract()), smooth(y.fract()));
    let row = |dy| {
        let a = hash(ix, iy.wrapping_add(dy));
        a + (hash(ix.wrapping_add(1), iy.wrapping_add(dy)) - a) * fx
    };
    row(0) + (row(1) - row(0)) * fy
}
