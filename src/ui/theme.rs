use eframe::egui::{
    self, Align2, Color32, FontId, Margin, Pos2, Rect, RichText, Sense, Stroke, Vec2,
};

mod palette;
pub use palette::{Palette, ThemeTransition, colors, on_color};
mod status;
pub use status::{PackageStatus, package_status};

pub fn initialize(ctx: &egui::Context) {
    system_fonts(ctx);
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(12.0, 12.0);
    style.spacing.button_padding = Vec2::new(12.0, 9.0);
    style.spacing.interact_size.y = 34.0;
    style.animation_time = 0.16;
    style
        .text_styles
        .insert(egui::TextStyle::Body, FontId::proportional(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Button, FontId::proportional(14.0));
    style
        .text_styles
        .insert(egui::TextStyle::Small, FontId::proportional(12.0));
    style
        .text_styles
        .insert(egui::TextStyle::Heading, FontId::proportional(26.0));
    ctx.set_style(style);
}

fn system_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    #[cfg(windows)]
    if let Ok(system) = crate::installer::windows::system_directory()
        && let Some(windows) = system.parent()
        && let Ok(bytes) = std::fs::read(windows.join("Fonts/segoeui.ttf"))
    {
        fonts
            .font_data
            .insert("Segoe UI".into(), egui::FontData::from_owned(bytes).into());
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "Segoe UI".into());
    }
    ctx.set_fonts(fonts);
}

pub fn pill(ui: &mut egui::Ui, text: &str, color: Color32) {
    let colors = colors(ui.ctx());
    egui::Frame::new()
        .fill(colors.surface.lerp_to_gamma(color, 0.08))
        .corner_radius(5)
        .inner_margin(Margin::symmetric(7, 3))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(10.5).color(color));
        });
}

pub fn heading(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    let colors = colors(ui.ctx());
    ui.label(RichText::new(title).size(28.0).strong());
    ui.add_space(1.0);
    ui.label(RichText::new(subtitle).color(colors.muted).size(14.0));
    ui.add_space(16.0);
}

pub fn bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1} ГБ", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} МБ", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.0} КБ", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} Б")
    }
}

pub fn app_icon(ui: &mut egui::Ui, id: &str, size: f32) {
    let colors = colors(ui.ctx());
    let fallback: String = if let Some(version) = id.strip_prefix("java-") {
        format!("J{version}")
    } else {
        id.split('-')
            .filter_map(|part| part.chars().next())
            .take(2)
            .flat_map(char::to_uppercase)
            .collect()
    };
    let blue = if colors.dark {
        Color32::from_rgb(126, 195, 240)
    } else {
        Color32::from_rgb(16, 103, 164)
    };
    let palette = [colors.accent, colors.violet, colors.orange, blue];
    let index = id.bytes().fold(0_u8, u8::wrapping_add) as usize % palette.len();
    let (letters, color) = match id {
        "amnezia-vpn" => ("A", colors.orange),
        "flclash" => ("Fl", colors.violet),
        "happ" => ("H", blue),
        "httpdebugger" => ("{ }", colors.accent),
        "blender" | "blender-addon" => ("B", colors.orange),
        "krita" => ("K", colors.violet),
        "vscode" => ("VS", blue),
        "obs" | "obs-addon" => ("OBS", colors.text),
        "7zip" => ("7z", colors.text),
        "git" => ("git", colors.orange),
        "vlc" => ("vlc", colors.orange),
        "gimp" => ("G", colors.dim),
        "python" => ("Py", colors.orange),
        "notepad-plus-plus" => ("n+", colors.accent),
        "discord" => ("D", colors.violet),
        "telegram" => ("TG", blue),
        "ayugram" => ("Ay", colors.violet),
        "brave" => ("Br", colors.orange),
        "chrome" => ("Ch", blue),
        "firefox" => ("Fx", colors.orange),
        "spotify" => ("Sp", colors.accent),
        "claude" | "claude-code" => ("Cl", colors.orange),
        "cursor" => ("Cu", colors.text),
        "trae" => ("Tr", colors.accent),
        "prism-launcher" | "prism-cracked" => ("Pr", colors.violet),
        _ => (fallback.as_str(), palette[index]),
    };
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    ui.painter()
        .rect_filled(rect, 10.0, color.gamma_multiply(0.13));
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        letters,
        FontId::proportional(size * if letters.len() > 2 { 0.29 } else { 0.4 }),
        color,
    );
}

pub fn logo(ui: &mut egui::Ui, size: f32) {
    let colors = colors(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    ui.painter().rect_filled(rect, 9.0, colors.accent_fill);
    draw_icon(
        ui,
        rect.shrink(size * 0.20),
        Icon::Download,
        colors.on_accent,
    );
}

#[derive(Clone, Copy)]
pub enum Icon {
    Grid,
    Layers,
    Download,
    Check,
    Settings,
    Search,
    Folder,
    Star,
}

pub fn icon(ui: &mut egui::Ui, icon: Icon, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(19.0), Sense::hover());
    draw_icon(ui, rect, icon, color);
}

fn draw_icon(ui: &egui::Ui, rect: Rect, icon: Icon, color: Color32) {
    let point = |x: f32, y: f32| {
        Pos2::new(
            rect.left() + rect.width() * x,
            rect.top() + rect.height() * y,
        )
    };
    let line = |points: &[(f32, f32)]| {
        ui.painter().add(egui::Shape::line(
            points.iter().map(|&(x, y)| point(x, y)).collect(),
            Stroke::new(1.6_f32, color),
        ))
    };
    match icon {
        Icon::Grid => {
            for (x, y) in [(0.1, 0.1), (0.58, 0.1), (0.1, 0.58), (0.58, 0.58)] {
                ui.painter().rect_stroke(
                    Rect::from_min_max(point(x, y), point(x + 0.30, y + 0.30)),
                    2,
                    Stroke::new(1.5_f32, color),
                    egui::StrokeKind::Inside,
                );
            }
        }
        Icon::Download => {
            line(&[(0.5, 0.08), (0.5, 0.60)]);
            line(&[(0.26, 0.38), (0.5, 0.63), (0.74, 0.38)]);
            line(&[(0.13, 0.66), (0.13, 0.87), (0.87, 0.87), (0.87, 0.66)]);
        }
        Icon::Layers => {
            line(&[
                (0.08, 0.32),
                (0.5, 0.1),
                (0.92, 0.32),
                (0.5, 0.56),
                (0.08, 0.32),
            ]);
            line(&[(0.08, 0.53), (0.5, 0.77), (0.92, 0.53)]);
            line(&[(0.08, 0.72), (0.5, 0.96), (0.92, 0.72)]);
        }
        Icon::Check => {
            ui.painter().circle_stroke(
                rect.center(),
                rect.width() * 0.43,
                Stroke::new(1.5_f32, color),
            );
            line(&[(0.27, 0.5), (0.44, 0.67), (0.74, 0.33)]);
        }
        Icon::Search => {
            ui.painter().circle_stroke(
                point(0.42, 0.42),
                rect.width() * 0.29,
                Stroke::new(1.5_f32, color),
            );
            line(&[(0.64, 0.64), (0.92, 0.92)]);
        }
        Icon::Settings => {
            ui.painter().circle_stroke(
                rect.center(),
                rect.width() * 0.35,
                Stroke::new(1.5_f32, color),
            );
            ui.painter().circle_stroke(
                rect.center(),
                rect.width() * 0.13,
                Stroke::new(1.5_f32, color),
            );
            for (a, b) in [
                ((0.5, 0.0), (0.5, 0.15)),
                ((0.5, 0.85), (0.5, 1.0)),
                ((0.0, 0.5), (0.15, 0.5)),
                ((0.85, 0.5), (1.0, 0.5)),
            ] {
                line(&[a, b]);
            }
        }
        Icon::Folder => {
            line(&[
                (0.08, 0.3),
                (0.08, 0.16),
                (0.40, 0.16),
                (0.54, 0.3),
                (0.92, 0.3),
                (0.92, 0.85),
                (0.08, 0.85),
                (0.08, 0.3),
                (0.92, 0.3),
            ]);
        }
        Icon::Star => {
            line(&[
                (0.5, 0.04),
                (0.64, 0.34),
                (0.97, 0.38),
                (0.73, 0.62),
                (0.79, 0.96),
                (0.5, 0.79),
                (0.21, 0.96),
                (0.27, 0.62),
                (0.03, 0.38),
                (0.36, 0.34),
                (0.5, 0.04),
            ]);
        }
    }
}

pub fn nav(
    ui: &mut egui::Ui,
    text: &str,
    icon_type: Icon,
    active: bool,
    count: Option<usize>,
) -> bool {
    let colors = colors(ui.ctx());
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 39.0), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            active,
            text,
        )
    });
    let selected = ui
        .ctx()
        .animate_bool_responsive(response.id.with("active"), active);
    let hovered = ui
        .ctx()
        .animate_bool_responsive(response.id.with("hover"), response.hovered());
    if selected > 0.0 || hovered > 0.0 {
        ui.painter().rect_filled(
            rect,
            8,
            colors
                .sidebar
                .lerp_to_gamma(colors.raised, hovered * 0.55 + selected * 0.30),
        );
    }
    if selected > 0.0 {
        let indicator = Rect::from_center_size(
            Pos2::new(rect.left() + 3.0, rect.center().y),
            Vec2::new(3.0, 18.0),
        );
        ui.painter()
            .rect_filled(indicator, 2, colors.accent.gamma_multiply(selected));
    }
    let color = colors.muted.lerp_to_gamma(colors.text, selected);
    draw_icon(
        ui,
        Rect::from_min_size(rect.min + Vec2::new(12.0, 10.0), Vec2::splat(18.0)),
        icon_type,
        color,
    );
    let mut label = egui::text::LayoutJob::simple(
        text.to_owned(),
        FontId::proportional(14.0),
        color,
        (rect.width() - if count.is_some() { 74.0 } else { 52.0 }).max(10.0),
    );
    label.wrap.max_rows = 1;
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(label));
    ui.painter().galley(
        Pos2::new(rect.left() + 41.0, rect.center().y - galley.size().y / 2.0),
        galley,
        color,
    );
    if let Some(count) = count {
        ui.painter().text(
            Pos2::new(rect.right() - 12.0, rect.center().y),
            Align2::RIGHT_CENTER,
            count,
            FontId::proportional(12.0),
            color,
        );
    }
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

pub fn favorite_button(ui: &mut egui::Ui, selected: bool, name: &str) -> egui::Response {
    let colors = colors(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::click());
    let label = format!(
        "{} «{name}»",
        if selected {
            "Убрать из избранного"
        } else {
            "В избранное"
        }
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::Checkbox,
            ui.is_enabled(),
            selected,
            &label,
        )
    });
    let hover = ui
        .ctx()
        .animate_bool_responsive(response.id.with("hover"), response.hovered());
    ui.painter()
        .rect_filled(rect, 7, colors.surface.lerp_to_gamma(colors.raised, hover));
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            7,
            Stroke::new(1.0_f32, colors.accent),
            egui::StrokeKind::Inside,
        );
    }
    draw_icon(
        ui,
        rect.shrink(8.0),
        Icon::Star,
        if selected {
            colors.accent
        } else {
            colors.muted
        },
    );
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(label)
}

pub fn filter_chip(ui: &mut egui::Ui, label: impl Into<String>, selected: bool) -> egui::Response {
    let colors = colors(ui.ctx());
    ui.add(
        egui::Button::new(RichText::new(label.into()).size(12.0).color(if selected {
            colors.text
        } else {
            colors.muted
        }))
        .fill(if selected {
            colors.raised
        } else {
            Color32::TRANSPARENT
        })
        .stroke(Stroke::new(
            1.0_f32,
            if selected {
                colors.border
            } else {
                Color32::TRANSPARENT
            },
        ))
        .corner_radius(6),
    )
}

pub fn window_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../../assets/SoftDownloader.png"))
        .expect("the bundled application icon is a valid PNG")
}

pub fn percent_slider(
    ui: &mut egui::Ui,
    value: &mut u8,
    range: std::ops::RangeInclusive<u8>,
    label: &str,
) -> egui::Response {
    let colors = colors(ui.ctx());
    ui.scope(|ui| {
        let style = ui.style_mut();
        style.spacing.slider_width = 160.0;
        style.spacing.interact_size.y = 24.0;
        style.visuals.widgets.inactive.bg_fill = colors.border;
        style.visuals.widgets.hovered.bg_fill = colors.accent_fill;
        style.visuals.widgets.active.bg_fill = colors.accent_fill;
        style.visuals.selection.bg_fill = colors.accent_fill;
        style.visuals.handle_shape = egui::style::HandleShape::Circle;
        ui.add(
            egui::Slider::new(value, range)
                .suffix(" %")
                .text(label)
                .trailing_fill(true),
        )
    })
    .inner
}
