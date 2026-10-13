use crate::weather::{Conditions, WeatherKind};
use eframe::egui::{self, Color32, Pos2, Rect, Vec2};

pub(super) struct Atmosphere {
    flake: egui::TextureHandle,
    mist: egui::TextureHandle,
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
        Self { flake, mist }
    }

    pub fn paint(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        clock: f32,
        scene: Conditions,
        strength: f32,
    ) {
        if !rect.is_positive() {
            return;
        }
        match scene.kind {
            WeatherKind::Snow | WeatherKind::Blizzard => {
                self.snow(painter, rect, clock, scene, strength)
            }
            WeatherKind::Rain | WeatherKind::Storm => {
                self.rain(painter, rect, clock, scene, strength)
            }
            WeatherKind::Fog | WeatherKind::Clouds => self.fog(painter, rect, clock, strength),
            WeatherKind::Clear => self.sky(painter, rect, clock, scene.day, strength),
        }
        if scene.kind == WeatherKind::Blizzard {
            self.fog(painter, rect, clock * 2.0, strength * 0.55);
        }
    }

    fn snow(&self, p: &egui::Painter, rect: Rect, t: f32, scene: Conditions, strength: f32) {
        let storm = scene.kind == WeatherKind::Blizzard;
        for i in 0..if storm { 210 } else { 115 } {
            let depth = hash(i, 7);
            let speed = 18.0 + depth * depth * 140.0;
            let wind = scene.wind_kmh * (1.0 + depth) + (t * 0.3).sin() * 12.0;
            let x = (hash(i, 13) * rect.width()
                + t * wind
                + (t * 0.6 + i as f32).sin() * (8.0 + depth * 12.0))
                .rem_euclid(rect.width() + 30.0)
                - 15.0;
            let y = (hash(i, 29) * rect.height() + t * speed * if storm { 1.8 } else { 1.0 })
                .rem_euclid(rect.height() + 30.0)
                - 15.0;
            let radius = 1.4 + depth.powi(3) * 10.0;
            let size = Vec2::new(radius * if storm { 2.0 } else { 1.0 }, radius * 1.15);
            p.image(
                self.flake.id(),
                Rect::from_center_size(rect.min + Vec2::new(x, y), size * 2.0),
                uv(),
                Color32::WHITE.gamma_multiply(strength * (0.35 + depth * 0.65)),
            );
        }
    }

    fn rain(&self, p: &egui::Painter, rect: Rect, t: f32, scene: Conditions, strength: f32) {
        let mut mesh = egui::Mesh::default();
        for i in 0..130 {
            let depth = hash(i, 17);
            let speed = 350.0 + depth * 600.0;
            let x = (hash(i, 31) * rect.width() + t * scene.wind_kmh * 3.0)
                .rem_euclid(rect.width() + 50.0)
                - 25.0;
            let y =
                (hash(i, 47) * rect.height() + t * speed).rem_euclid(rect.height() + 80.0) - 40.0;
            let head = rect.min + Vec2::new(x, y);
            let tail = head - Vec2::new(2.0 + scene.wind_kmh * 0.22, 12.0 + depth * 25.0);
            let width = Vec2::new(0.35 + depth * 0.5, 0.0);
            let index = mesh.vertices.len() as u32;
            for (pos, alpha) in [
                (tail - width, 0.0),
                (tail + width, 0.0),
                (head + width, 0.7),
                (head - width, 0.7),
            ] {
                mesh.colored_vertex(
                    pos,
                    Color32::from_rgb(191, 216, 237).gamma_multiply(strength * alpha),
                );
            }
            mesh.add_triangle(index, index + 1, index + 2);
            mesh.add_triangle(index, index + 2, index + 3);
        }
        p.add(mesh);
        // A slow diffuse glow, never a flashing full-screen lightning effect.
        if scene.kind == WeatherKind::Storm {
            let pulse = ((t * 0.35).sin().max(0.0)).powi(8) * strength * 0.13;
            p.image(
                self.mist.id(),
                rect,
                uv(),
                Color32::from_rgb(184, 200, 236).gamma_multiply(pulse),
            );
        }
    }

    pub fn fog(&self, p: &egui::Painter, rect: Rect, t: f32, strength: f32) {
        if !rect.is_positive() {
            return;
        }
        for layer in 0..6 {
            let phase = layer as f32;
            let width = rect.width() * 1.3;
            let x = (phase * rect.width() * 0.39 + t * (5.0 + phase))
                .rem_euclid(rect.width() * 2.4)
                - width * 0.5;
            let center = rect.min + Vec2::new(x, rect.height() * (0.25 + phase * 0.13));
            p.image(
                self.mist.id(),
                Rect::from_center_size(center, Vec2::new(width, rect.height() * 0.65)),
                uv(),
                Color32::from_rgb(207, 221, 230).gamma_multiply(strength * 0.24),
            );
        }
    }

    fn sky(&self, p: &egui::Painter, rect: Rect, t: f32, day: bool, strength: f32) {
        if day {
            let center = rect.min + rect.size() * Vec2::new(0.78, 0.12);
            p.image(
                self.flake.id(),
                Rect::from_center_size(center, Vec2::splat(rect.height() * 0.9)),
                uv(),
                Color32::from_rgb(255, 216, 150).gamma_multiply(strength * 0.55),
            );
        } else {
            for i in 0..65 {
                let center = rect.min
                    + Vec2::new(
                        hash(i, 83) * rect.width(),
                        hash(i, 97) * rect.height() * 0.8,
                    );
                let alpha = strength * (0.35 + 0.15 * (t * 0.4 + i as f32).sin());
                p.image(
                    self.flake.id(),
                    Rect::from_center_size(center, Vec2::splat(4.0)),
                    uv(),
                    Color32::WHITE.gamma_multiply(alpha),
                );
            }
        }
    }
}

fn texture(
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
fn uv() -> Rect {
    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0))
}
fn hash(i: u32, salt: u32) -> f32 {
    let mut value = i
        .wrapping_mul(747796405)
        .wrapping_add(salt.wrapping_mul(2891336453));
    value = ((value >> ((value >> 28) + 4)) ^ value).wrapping_mul(277803737);
    ((value >> 22) ^ value) as f32 / u32::MAX as f32
}
