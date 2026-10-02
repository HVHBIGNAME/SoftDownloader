use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Margin, Pos2, Rect, RichText, Sense, Stroke, Vec2,
};

pub const BG: Color32 = Color32::from_rgb(18, 21, 24);
pub const SIDEBAR: Color32 = Color32::from_rgb(22, 26, 29);
pub const SURFACE: Color32 = Color32::from_rgb(29, 33, 37);
pub const RAISED: Color32 = Color32::from_rgb(36, 41, 46);
pub const BORDER: Color32 = Color32::from_rgb(46, 52, 57);
pub const TEXT: Color32 = Color32::from_rgb(236, 240, 237);
pub const MUTED: Color32 = Color32::from_rgb(151, 160, 166);
pub const DIM: Color32 = Color32::from_rgb(107, 119, 125);
pub const ACCENT: Color32 = Color32::from_rgb(188, 239, 119);
pub const VIOLET: Color32 = Color32::from_rgb(186, 169, 242);
pub const ORANGE: Color32 = Color32::from_rgb(238, 178, 117);
pub const RED: Color32 = Color32::from_rgb(242, 142, 142);

pub fn apply(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = SURFACE;
    style.visuals.extreme_bg_color = BG;
    style.visuals.faint_bg_color = SURFACE;
    style.visuals.window_stroke = Stroke::new(1.0_f32, BORDER);
    style.visuals.selection.bg_fill = ACCENT.gamma_multiply(0.25);
    style.visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    style.visuals.hyperlink_color = ACCENT;
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    style.visuals.widgets.inactive.bg_fill = SURFACE;
    style.visuals.widgets.inactive.weak_bg_fill = SURFACE;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    style.visuals.widgets.hovered.bg_fill = RAISED;
    style.visuals.widgets.hovered.weak_bg_fill = RAISED;
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, MUTED.gamma_multiply(0.5));
    style.visuals.widgets.active.bg_fill = ACCENT.gamma_multiply(0.25);
    for widgets in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.noninteractive,
    ] {
        widgets.corner_radius = CornerRadius::same(8);
    }
    style.spacing.item_spacing = Vec2::new(10.0, 10.0);
    style.spacing.button_padding = Vec2::new(12.0, 9.0);
    style.spacing.interact_size.y = 34.0;
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

pub fn card_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(12)
        .inner_margin(16)
}

pub fn primary(text: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.into()).color(BG).strong())
        .fill(ACCENT)
        .min_size(Vec2::new(0.0, 40.0))
        .corner_radius(8)
}

pub fn pill(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.09))
        .corner_radius(5)
        .inner_margin(Margin::symmetric(7, 3))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(10.5).color(color));
        });
}

pub fn heading(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.label(RichText::new(title).size(29.0).strong());
    ui.add_space(1.0);
    ui.label(RichText::new(subtitle).color(MUTED).size(14.0));
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
    let fallback: String = if let Some(version) = id.strip_prefix("java-") {
        format!("J{version}")
    } else {
        id.split('-')
            .filter_map(|part| part.chars().next())
            .take(2)
            .flat_map(char::to_uppercase)
            .collect()
    };
    let palette = [ACCENT, VIOLET, ORANGE, Color32::from_rgb(126, 195, 240)];
    let index = id.bytes().fold(0_u8, u8::wrapping_add) as usize % palette.len();
    let (letters, color) = match id {
        "amnezia-vpn" => ("A", ORANGE),
        "flclash" => ("Fl", VIOLET),
        "happ" => ("H", Color32::from_rgb(126, 195, 240)),
        "httpdebugger" => ("{ }", ACCENT),
        "blender" | "blender-addon" => ("B", ORANGE),
        "krita" => ("K", VIOLET),
        "vscode" => ("VS", Color32::from_rgb(126, 195, 240)),
        "obs" | "obs-addon" => ("OBS", Color32::from_rgb(208, 207, 229)),
        "7zip" => ("7z", TEXT),
        "git" => ("git", Color32::from_rgb(239, 157, 132)),
        "vlc" => ("vlc", ORANGE),
        "gimp" => ("G", Color32::from_rgb(202, 186, 155)),
        "python" => ("Py", Color32::from_rgb(240, 211, 119)),
        "notepad-plus-plus" => ("n+", ACCENT),
        "discord" => ("D", VIOLET),
        "telegram" => ("TG", Color32::from_rgb(126, 195, 240)),
        "ayugram" => ("Ay", VIOLET),
        "brave" => ("Br", ORANGE),
        "chrome" => ("Ch", Color32::from_rgb(126, 195, 240)),
        "firefox" => ("Fx", ORANGE),
        "spotify" => ("Sp", ACCENT),
        "claude" | "claude-code" => ("Cl", ORANGE),
        "cursor" => ("Cu", TEXT),
        "trae" => ("Tr", ACCENT),
        "prism-launcher" | "prism-cracked" => ("Pr", VIOLET),
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
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    ui.painter().rect_filled(rect, 9.0, ACCENT);
    draw_icon(ui, rect.shrink(size * 0.20), Icon::Download, BG);
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
    }
}

pub fn nav(
    ui: &mut egui::Ui,
    text: &str,
    icon_type: Icon,
    active: bool,
    count: Option<usize>,
) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 39.0), Sense::click());
    if active || response.hovered() {
        ui.painter().rect_filled(
            rect,
            8,
            if active {
                ACCENT.gamma_multiply(0.10)
            } else {
                SURFACE
            },
        );
    }
    let color = if active { ACCENT } else { MUTED };
    draw_icon(
        ui,
        Rect::from_min_size(rect.min + Vec2::new(12.0, 10.0), Vec2::splat(18.0)),
        icon_type,
        color,
    );
    let mut label = egui::text::LayoutJob::simple(
        text.to_owned(),
        FontId::proportional(13.0),
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

pub fn window_icon() -> egui::IconData {
    let mut rgba = vec![0_u8; 64 * 64 * 4];
    for y in 0_usize..64 {
        for x in 0_usize..64 {
            let arrow = (29..=34).contains(&x) && (14..=37).contains(&y)
                || ((26..=41).contains(&y) && x.abs_diff(31) == 41 - y)
                || ((44..=48).contains(&y) && (17..=46).contains(&x))
                || ((39..=47).contains(&y) && ((17..=20).contains(&x) || (43..=46).contains(&x)));
            let color = if arrow { BG } else { ACCENT };
            let start = (y * 64 + x) * 4;
            rgba[start..start + 4].copy_from_slice(&color.to_array());
        }
    }
    egui::IconData {
        rgba,
        width: 64,
        height: 64,
    }
}
