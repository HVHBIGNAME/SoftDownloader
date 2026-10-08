use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke, Vec2};

use crate::preferences::{Appearance, Season, Theme};

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub bg: Color32,
    pub sidebar: Color32,
    pub surface: Color32,
    pub raised: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub dim: Color32,
    pub accent: Color32,
    pub accent_fill: Color32,
    pub on_accent: Color32,
    pub violet: Color32,
    pub orange: Color32,
    pub red: Color32,
    pub dark: bool,
}

impl Palette {
    pub fn new(appearance: Appearance, season: Option<Season>) -> Self {
        let rgb = Color32::from_rgb;
        let (bg, sidebar, surface, raised, border) = match appearance.theme {
            Theme::Dark => (
                rgb(21, 25, 30),
                rgb(25, 30, 36),
                rgb(33, 39, 46),
                rgb(43, 50, 59),
                rgb(52, 61, 71),
            ),
            Theme::Light => (
                rgb(242, 245, 248),
                rgb(250, 252, 254),
                rgb(255, 255, 255),
                rgb(231, 236, 241),
                rgb(205, 214, 223),
            ),
            Theme::Graphite => (
                rgb(37, 39, 42),
                rgb(42, 44, 47),
                rgb(49, 52, 56),
                rgb(60, 63, 68),
                rgb(77, 82, 89),
            ),
            Theme::Midnight => (
                rgb(13, 19, 32),
                rgb(17, 25, 42),
                rgb(24, 34, 52),
                rgb(32, 44, 64),
                rgb(46, 62, 84),
            ),
        };
        let tint = match season {
            Some(Season::Winter) => rgb(81, 174, 219),
            Some(Season::Halloween) => rgb(186, 115, 203),
            None => bg,
        };
        let dark = appearance.theme != Theme::Light;
        let [r, g, b] = appearance.accent;
        let accent_fill = rgb(r, g, b);
        let mut palette = Self {
            bg: bg.lerp_to_gamma(tint, if season.is_some() { 0.025 } else { 0.0 }),
            sidebar: sidebar.lerp_to_gamma(tint, if season.is_some() { 0.05 } else { 0.0 }),
            surface,
            raised,
            border,
            text: if dark {
                rgb(238, 242, 245)
            } else {
                rgb(26, 37, 49)
            },
            muted: if dark {
                rgb(170, 180, 188)
            } else {
                rgb(82, 96, 110)
            },
            dim: if dark {
                rgb(147, 159, 169)
            } else {
                rgb(97, 110, 122)
            },
            accent: accent_fill,
            accent_fill,
            on_accent: on_color(accent_fill),
            violet: rgb(186, 169, 242),
            orange: rgb(238, 178, 117),
            red: rgb(242, 142, 142),
            dark,
        };
        palette.ensure_contrast();
        palette
    }

    fn ensure_contrast(&mut self) {
        let backgrounds = [self.bg, self.sidebar, self.surface, self.raised];
        for color in [
            &mut self.text,
            &mut self.muted,
            &mut self.dim,
            &mut self.accent,
            &mut self.violet,
            &mut self.orange,
            &mut self.red,
        ] {
            *color = readable(*color, backgrounds);
        }
        self.on_accent = on_color(self.accent_fill);
    }

    fn mix(self, target: Self, t: f32) -> Self {
        let mix = |a: Color32, b| a.lerp_to_gamma(b, t);
        let mut value = Self {
            bg: mix(self.bg, target.bg),
            sidebar: mix(self.sidebar, target.sidebar),
            surface: mix(self.surface, target.surface),
            raised: mix(self.raised, target.raised),
            border: mix(self.border, target.border),
            text: mix(self.text, target.text),
            muted: mix(self.muted, target.muted),
            dim: mix(self.dim, target.dim),
            accent: mix(self.accent, target.accent),
            accent_fill: mix(self.accent_fill, target.accent_fill),
            on_accent: target.on_accent,
            violet: mix(self.violet, target.violet),
            orange: mix(self.orange, target.orange),
            red: mix(self.red, target.red),
            dark: target.dark,
        };
        value.balance_backgrounds();
        value.ensure_contrast();
        value
    }

    fn balance_backgrounds(&mut self) {
        let backgrounds = [self.bg, self.sidebar, self.surface, self.raised];
        if [Color32::BLACK, Color32::WHITE].into_iter().any(|text| {
            backgrounds
                .iter()
                .all(|&background| contrast(text, background) >= 4.5)
        }) {
            return;
        }
        // At the light/dark crossover, bring surfaces closer together so one text color works on all of them.
        let base = self.bg;
        let text = on_color(base);
        for surface in [&mut self.sidebar, &mut self.surface, &mut self.raised] {
            if contrast(text, *surface) < 4.5 {
                *surface = (1..=32)
                    .map(|step| surface.lerp_to_gamma(base, step as f32 / 32.0))
                    .find(|&color| contrast(text, color) >= 4.5)
                    .unwrap_or(base);
            }
        }
    }

    pub fn card_frame(self) -> egui::Frame {
        egui::Frame::new()
            .fill(self.surface)
            .stroke(Stroke::new(1.0_f32, self.border))
            .corner_radius(10)
            .inner_margin(16)
    }

    pub fn primary(self, text: impl Into<String>) -> egui::Button<'static> {
        egui::Button::new(RichText::new(text.into()).color(self.on_accent).strong())
            .fill(self.accent_fill)
            .min_size(Vec2::new(0.0, 40.0))
            .corner_radius(6)
    }
}

pub fn colors(ctx: &egui::Context) -> Palette {
    ctx.data(|data| data.get_temp::<Palette>(egui::Id::new("app-palette")))
        .unwrap_or_else(|| Palette::new(Appearance::default(), None))
}

pub fn contrast(a: Color32, b: Color32) -> f32 {
    let a = luminance(a);
    let b = luminance(b);
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn luminance(color: Color32) -> f32 {
    let linear = |v: u8| {
        let v = f32::from(v) / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(color.r()) + 0.7152 * linear(color.g()) + 0.0722 * linear(color.b())
}

pub fn on_color(color: Color32) -> Color32 {
    if contrast(Color32::BLACK, color) >= contrast(Color32::WHITE, color) {
        Color32::BLACK
    } else {
        Color32::WHITE
    }
}

fn readable(color: Color32, backgrounds: [Color32; 4]) -> Color32 {
    let minimum = |candidate| {
        backgrounds
            .iter()
            .map(|&bg| contrast(candidate, bg))
            .fold(f32::INFINITY, f32::min)
    };
    if minimum(color) >= 4.5 {
        return color;
    }
    let target = if minimum(Color32::BLACK) >= minimum(Color32::WHITE) {
        Color32::BLACK
    } else {
        Color32::WHITE
    };
    (1..=32)
        .map(|step| color.lerp_to_gamma(target, step as f32 / 32.0))
        .find(|&color| minimum(color) >= 4.5)
        .unwrap_or(target)
}

pub struct ThemeTransition {
    key: (Appearance, Option<Season>, bool),
    current: Palette,
    from: Palette,
    target: Palette,
    started: Option<Instant>,
}

impl ThemeTransition {
    pub fn new(
        ctx: &egui::Context,
        appearance: Appearance,
        season: Option<Season>,
        reduced: bool,
    ) -> Self {
        let current = Palette::new(appearance, season);
        apply(ctx, current, reduced);
        Self {
            key: (appearance, season, reduced),
            current,
            from: current,
            target: current,
            started: None,
        }
    }

    pub fn update(
        &mut self,
        ctx: &egui::Context,
        appearance: Appearance,
        season: Option<Season>,
        reduced: bool,
    ) {
        let key = (appearance, season, reduced);
        let changed = self.key != key;
        if changed {
            self.key = key;
            self.from = self.current;
            self.target = Palette::new(appearance, season);
            self.started = Some(Instant::now());
        }
        let Some(started) = self.started else {
            return;
        };
        let t = if reduced || !ctx.input(|i| i.focused) {
            1.0
        } else {
            (started.elapsed().as_secs_f32() / 0.24).min(1.0)
        };
        self.current = self.from.mix(self.target, 1.0 - (1.0 - t).powi(3));
        apply(ctx, self.current, reduced);
        if t >= 1.0 {
            self.started = None;
        } else {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }
}

fn apply(ctx: &egui::Context, colors: Palette, reduced: bool) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::new("app-palette"), colors));
    ctx.style_mut(|style| {
        let v = &mut style.visuals;
        v.dark_mode = colors.dark;
        v.override_text_color = Some(colors.text);
        v.weak_text_color = Some(colors.muted);
        v.panel_fill = colors.bg;
        v.window_fill = colors.surface;
        v.extreme_bg_color = colors.bg;
        v.faint_bg_color = colors.surface;
        v.window_stroke = Stroke::new(1.0_f32, colors.border);
        v.selection.bg_fill = colors.surface.lerp_to_gamma(colors.accent, 0.18);
        v.selection.stroke = Stroke::new(1.4_f32, colors.accent);
        v.hyperlink_color = colors.accent;
        v.warn_fg_color = colors.orange;
        v.error_fg_color = colors.red;
        v.text_cursor.stroke = Stroke::new(1.5_f32, colors.accent);
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, colors.border);
        for widget in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            widget.fg_stroke = Stroke::new(1.2_f32, colors.text);
            widget.corner_radius = CornerRadius::same(6);
        }
        for widget in [&mut v.widgets.inactive, &mut v.widgets.open] {
            widget.bg_fill = colors.surface;
            widget.weak_bg_fill = colors.surface;
            widget.bg_stroke = Stroke::new(1.0_f32, colors.border);
        }
        v.widgets.hovered.bg_fill = colors.raised;
        v.widgets.hovered.weak_bg_fill = colors.raised;
        v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, colors.accent);
        v.widgets.active.bg_fill = colors.raised;
        v.widgets.active.weak_bg_fill = colors.raised;
        v.widgets.active.bg_stroke = Stroke::new(1.5_f32, colors.accent);
        style.animation_time = if reduced { 0.0 } else { 0.18 };
        style.scroll_animation = if reduced {
            egui::style::ScrollAnimation::none()
        } else {
            egui::style::ScrollAnimation::duration(0.18)
        };
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changing_between_light_and_dark_keeps_intermediate_frames_readable() {
        let light = Palette::new(
            Appearance {
                theme: Theme::Light,
                accent: [255, 255, 255],
                ..Default::default()
            },
            None,
        );
        for theme in [Theme::Dark, Theme::Graphite, Theme::Midnight] {
            let dark = Palette::new(
                Appearance {
                    theme,
                    accent: [0, 0, 0],
                    ..Default::default()
                },
                Some(Season::Halloween),
            );
            for step in 0..=32 {
                for (from, to) in [(light, dark), (dark, light)] {
                    let frame = from.mix(to, step as f32 / 32.0);
                    for foreground in [frame.text, frame.muted, frame.dim, frame.accent, frame.red]
                    {
                        for background in [frame.bg, frame.sidebar, frame.surface, frame.raised] {
                            assert!(contrast(foreground, background) >= 4.5);
                        }
                    }
                    assert!(contrast(frame.on_accent, frame.accent_fill) >= 4.5);
                }
            }
        }
    }

    #[test]
    fn reduced_motion_applies_a_theme_immediately_to_widgets_and_custom_drawing() {
        let ctx = egui::Context::default();
        let mut transition = ThemeTransition::new(&ctx, Appearance::default(), None, false);
        let appearance = Appearance {
            theme: Theme::Light,
            accent: [0, 0, 0],
            ..Default::default()
        };
        transition.update(&ctx, appearance, Some(Season::Winter), true);
        let expected = Palette::new(appearance, Some(Season::Winter));
        assert!(transition.started.is_none());
        assert_eq!(colors(&ctx).bg, expected.bg);
        assert_eq!(colors(&ctx).accent_fill, expected.accent_fill);
        assert_eq!(ctx.style().visuals.panel_fill, expected.bg);
        assert_eq!(ctx.style().visuals.override_text_color, Some(expected.text));
        assert_eq!(ctx.style().animation_time, 0.0);
        transition.update(&ctx, appearance, Some(Season::Winter), false);
        assert!(ctx.style().animation_time > 0.0);
    }

    #[test]
    fn all_themes_keep_text_and_custom_accents_readable() {
        for theme in Theme::ALL {
            for season in [None, Some(Season::Winter), Some(Season::Halloween)] {
                for accent in [
                    [0, 0, 0],
                    [255, 255, 255],
                    [188, 239, 119],
                    [128, 128, 128],
                    [255, 0, 0],
                    [0, 0, 255],
                ] {
                    let p = Palette::new(
                        Appearance {
                            theme,
                            accent,
                            ..Default::default()
                        },
                        season,
                    );
                    for fg in [p.text, p.muted, p.dim, p.accent, p.red, p.orange, p.violet] {
                        for bg in [p.bg, p.sidebar, p.surface, p.raised] {
                            assert!(contrast(fg, bg) >= 4.5, "{theme:?}: {fg:?} on {bg:?}");
                        }
                    }
                    assert!(contrast(p.on_accent, p.accent_fill) >= 4.5);
                }
            }
        }
    }
}
