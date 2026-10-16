use crate::weather::{Conditions, WeatherKind};
use eframe::egui::{self, Color32, Pos2, Rect, Vec2};

use super::weather_transition::{Motion, WeatherMix};

struct Particle {
    x: f32,
    y: f32,
    depth: f32,
    phase: f32,
}

pub(super) struct Atmosphere {
    flake: egui::TextureHandle,
    mist: egui::TextureHandle,
    particles: [Particle; 320],
    pub sky: super::sky::Sky,
}

impl Atmosphere {
    pub fn new(ctx: &egui::Context) -> Self {
        let flake = texture(ctx, "soft-snow", [64, 64], |x, y| {
            let r = (x * x + y * y).sqrt();
            ((-r * r * 5.5).exp() * (1.0 - r).max(0.0) * 255.0) as u8
        });
        let mist = texture(ctx, "volumetric-mist", [192, 96], |x, y| {
            let noise = ((x * 13.0 + y * 7.0).sin() * (y * 19.0 - x * 3.0).cos() * 0.16
                + ((x * 31.0).sin() + (y * 37.0).cos()) * 0.04
                + 0.65)
                .max(0.0);
            (noise * (1.0 - x * x).max(0.0) * (1.0 - y * y).max(0.0) * 180.0) as u8
        });
        Self {
            flake,
            mist,
            particles: std::array::from_fn(|i| Particle {
                x: hash(i as u32, 31),
                y: hash(i as u32, 47),
                depth: hash(i as u32, 17),
                phase: hash(i as u32, 73) * std::f32::consts::TAU,
            }),
            sky: super::sky::Sky::new(ctx),
        }
    }

    pub fn paint(
        &self,
        p: &egui::Painter,
        rect: Rect,
        time: f32,
        scene: Conditions,
        strength: f32,
    ) {
        let mix = WeatherMix::new(scene);
        let mut motion = Motion::default();
        motion.advance(time, mix);
        self.paint_weather(p, rect, motion, mix, strength, 1.0);
    }

    pub fn paint_weather(
        &self,
        p: &egui::Painter,
        rect: Rect,
        motion: Motion,
        mix: WeatherMix,
        strength: f32,
        detail: f32,
    ) {
        if !rect.is_positive() || !p.clip_rect().intersects(rect) {
            return;
        }
        if mix.snow() > 0.001 {
            self.snow(p, rect, motion, mix, strength, detail);
        }
        if mix.rain() > 0.001 {
            self.rain(p, rect, motion, mix, strength, detail);
        }
        if mix.weight(WeatherKind::Hail) > 0.001 {
            self.hail(
                p,
                rect,
                motion,
                mix.weight(WeatherKind::Hail) * strength,
                detail,
            );
        }
        let fog = mix.weight(WeatherKind::Fog)
            + mix.weight(WeatherKind::Blizzard) * 0.55
            + mix.weight(WeatherKind::Downpour) * 0.18;
        if fog > 0.001 {
            self.fog(p, rect, motion.time, strength * fog);
        }
        let thunder = mix.weight(WeatherKind::Storm);
        if thunder > 0.001 {
            let pulse = (motion.time * 0.35).sin().max(0.0).powi(8) * strength * thunder * 0.13;
            p.image(
                self.mist.id(),
                rect,
                uv(),
                Color32::from_rgb(184, 200, 236).gamma_multiply(pulse),
            );
        }
    }

    fn snow(
        &self,
        p: &egui::Painter,
        rect: Rect,
        motion: Motion,
        mix: WeatherMix,
        strength: f32,
        detail: f32,
    ) {
        let mut mesh = egui::Mesh::with_texture(self.flake.id());
        mesh.vertices.reserve(280 * 4);
        mesh.indices.reserve(280 * 6);
        let blizzard = mix.weight(WeatherKind::Blizzard);
        for (i, seed) in self.particles[..280].iter().enumerate() {
            let visibility = density(i, 280, detail) * mix.snow();
            if visibility < 0.002 {
                continue;
            }
            let depth = seed.depth;
            let x = (seed.x * rect.width()
                + motion.wind * (1.0 + depth)
                + (motion.time * 0.3).sin() * 35.0
                + (motion.time * 0.6 + seed.phase).sin() * (8.0 + depth * 12.0))
                .rem_euclid(rect.width() + 30.0)
                - 15.0;
            let y = (seed.y * rect.height() + motion.snow * (0.45 + depth * depth * 1.6))
                .rem_euclid(rect.height() + 30.0)
                - 15.0;
            let radius = 1.4 + depth.powi(3) * 10.0;
            let size = Vec2::new(radius * (1.0 + blizzard), radius * 1.15);
            mesh.add_rect_with_uv(
                Rect::from_center_size(rect.min + Vec2::new(x, y), size * 2.0),
                uv(),
                Color32::WHITE.gamma_multiply(strength * visibility * (0.35 + depth * 0.65)),
            );
        }
        p.add(mesh);
    }

    fn rain(
        &self,
        p: &egui::Painter,
        rect: Rect,
        motion: Motion,
        mix: WeatherMix,
        strength: f32,
        detail: f32,
    ) {
        let mut mesh = egui::Mesh::default();
        mesh.vertices.reserve(320 * 4);
        mesh.indices.reserve(320 * 6);
        for (i, seed) in self.particles.iter().enumerate() {
            let visibility = density(i, 320, detail * (0.5 + mix.rain() * 0.5)) * mix.rain();
            if visibility < 0.002 {
                continue;
            }
            let depth = seed.depth;
            let x =
                (seed.x * rect.width() + motion.wind * 3.0).rem_euclid(rect.width() + 50.0) - 25.0;
            let y = (seed.y * rect.height() + motion.rain * (0.5 + depth * 0.8))
                .rem_euclid(rect.height() + 100.0)
                - 50.0;
            let head = rect.min + Vec2::new(x, y);
            let tail = head
                - Vec2::new(
                    3.0 + mix.wind.min(120.0) * 0.28,
                    20.0 + depth * 42.0 + mix.weight(WeatherKind::Downpour) * 22.0,
                );
            let width = Vec2::new(0.45 + depth * 0.6, 0.0);
            let index = mesh.vertices.len() as u32;
            for (pos, alpha) in [
                (tail - width, 0.0),
                (tail + width, 0.0),
                (head + width, 0.55 + depth * 0.4),
                (head - width, 0.55 + depth * 0.4),
            ] {
                mesh.colored_vertex(
                    pos,
                    Color32::from_rgb(191, 216, 237).gamma_multiply(strength * visibility * alpha),
                );
            }
            mesh.add_triangle(index, index + 1, index + 2);
            mesh.add_triangle(index, index + 2, index + 3);
        }
        p.add(mesh);
    }

    fn hail(&self, p: &egui::Painter, rect: Rect, motion: Motion, strength: f32, detail: f32) {
        let mut mesh = egui::Mesh::default();
        mesh.vertices.reserve(150 * 4);
        mesh.indices.reserve(150 * 6);
        for (i, seed) in self.particles[..150].iter().enumerate() {
            let opacity = density(i, 150, detail) * strength;
            if opacity < 0.002 {
                continue;
            }
            let cycle =
                (seed.y + motion.hail * (0.65 + seed.depth * 0.6) / rect.height()).rem_euclid(1.4);
            let bounce = ((cycle - 1.0) / 0.4).max(0.0);
            let y = if cycle < 1.0 {
                cycle * (rect.height() + 8.0) - 8.0
            } else {
                rect.height()
                    - 4.0
                    - (bounce * std::f32::consts::PI).sin() * (18.0 + 30.0 * seed.depth)
            };
            let x = (seed.x * rect.width() + motion.wind * 1.8 + bounce * 30.0)
                .rem_euclid(rect.width() + 12.0)
                - 6.0;
            let center = rect.min + Vec2::new(x, y);
            let radius = 1.6 + seed.depth * 2.8;
            let index = mesh.vertices.len() as u32;
            let color =
                Color32::from_rgb(218, 237, 255).gamma_multiply(opacity * (1.0 - bounce).max(0.0));
            for offset in [
                Vec2::new(-radius, 0.0),
                Vec2::new(0.0, -radius * 1.3),
                Vec2::new(radius, 0.0),
                Vec2::new(0.0, radius),
            ] {
                mesh.colored_vertex(center + offset, color);
            }
            mesh.add_triangle(index, index + 1, index + 2);
            mesh.add_triangle(index, index + 2, index + 3);
        }
        p.add(mesh);
    }

    pub fn fog(&self, p: &egui::Painter, rect: Rect, time: f32, strength: f32) {
        if !rect.is_positive() {
            return;
        }
        let mut mesh = egui::Mesh::with_texture(self.mist.id());
        for layer in 0..6 {
            let phase = layer as f32;
            let width = rect.width() * 1.3;
            let x = (phase * rect.width() * 0.39 + time * (18.0 + phase * 3.0))
                .rem_euclid(rect.width() * 2.4)
                - width * 0.5;
            let center = rect.min + Vec2::new(x, rect.height() * (0.25 + phase * 0.13));
            mesh.add_rect_with_uv(
                Rect::from_center_size(center, Vec2::new(width, rect.height() * 0.65)),
                uv(),
                Color32::from_rgb(207, 221, 230).gamma_multiply(strength * 0.40),
            );
        }
        p.add(mesh);
    }
}

fn density(index: usize, count: usize, amount: f32) -> f32 {
    ((count as f32 * amount - index as f32) / 12.0).clamp(0.0, 1.0)
}

pub(super) fn texture(
    ctx: &egui::Context,
    name: &str,
    size: [usize; 2],
    alpha: impl Fn(f32, f32) -> u8,
) -> egui::TextureHandle {
    let mut image = egui::ColorImage::filled(size, Color32::TRANSPARENT);
    for y in 0..size[1] {
        for x in 0..size[0] {
            image[(x, y)] = Color32::from_white_alpha(alpha(
                x as f32 / (size[0] - 1) as f32 * 2.0 - 1.0,
                y as f32 / (size[1] - 1) as f32 * 2.0 - 1.0,
            ));
        }
    }
    ctx.load_texture(name, image, egui::TextureOptions::LINEAR)
}
pub(super) fn uv() -> Rect {
    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0))
}
pub(super) fn hash(i: u32, salt: u32) -> f32 {
    let mut value = i
        .wrapping_mul(747796405)
        .wrapping_add(salt.wrapping_mul(2891336453));
    value = ((value >> ((value >> 28) + 4)) ^ value).wrapping_mul(277803737);
    ((value >> 22) ^ value) as f32 / u32::MAX as f32
}
