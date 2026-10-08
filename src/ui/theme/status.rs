use eframe::egui::{self, Align2, FontId, Sense, Stroke, Vec2};

use super::colors;

#[derive(Clone, Copy, PartialEq)]
pub enum PackageStatus {
    Available,
    Selected,
    Installed,
    Manual,
    Unavailable,
    Loading,
}

pub fn package_status(ui: &mut egui::Ui, status: PackageStatus, enabled: bool) -> egui::Response {
    let colors = colors(ui.ctx());
    let selectable =
        enabled && matches!(status, PackageStatus::Available | PackageStatus::Selected);
    let sense = if selectable {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (slot, response) = ui.allocate_exact_size(Vec2::splat(34.0), sense);
    let label = match status {
        PackageStatus::Available => "Добавить к установке",
        PackageStatus::Selected => "Выбрано для установки · нажмите, чтобы убрать",
        PackageStatus::Installed => "Уже установлено на этом компьютере",
        PackageStatus::Manual => "Ручная установка · инструкция в подробностях",
        PackageStatus::Unavailable => "Пока недоступно · причина в подробностях",
        PackageStatus::Loading => "Проверяем доступность установщика…",
    };
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::Checkbox,
            selectable && ui.is_enabled(),
            matches!(status, PackageStatus::Selected | PackageStatus::Installed),
            label,
        )
    });
    let checked = matches!(status, PackageStatus::Selected | PackageStatus::Installed);
    let selected = status == PackageStatus::Selected;
    let border = if checked || selectable && (response.hovered() || response.has_focus()) {
        colors.accent
    } else {
        colors.dim
    };
    let rect = egui::Rect::from_center_size(slot.center(), Vec2::splat(18.0));
    let fill = if selected {
        colors.accent_fill
    } else {
        colors.surface
    };
    ui.painter().rect_filled(rect, 5, fill);
    ui.painter().rect_stroke(
        rect,
        5,
        Stroke::new(1.4_f32, border),
        egui::StrokeKind::Inside,
    );
    if checked {
        let center = rect.center();
        ui.painter().add(egui::Shape::line(
            vec![
                center + Vec2::new(-4.0, 0.0),
                center + Vec2::new(-1.0, 3.0),
                center + Vec2::new(4.5, -3.0),
            ],
            Stroke::new(
                1.7_f32,
                if selected {
                    colors.on_accent
                } else {
                    colors.accent
                },
            ),
        ));
    } else if status != PackageStatus::Available {
        let symbol = match status {
            PackageStatus::Manual => "↗",
            PackageStatus::Loading => "·",
            _ => "−",
        };
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            symbol,
            FontId::proportional(14.0),
            colors.muted,
        );
    }
    response.on_hover_text(label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_state_keeps_the_same_control_slot() {
        let mut bounds = Vec::new();
        for status in [
            PackageStatus::Available,
            PackageStatus::Selected,
            PackageStatus::Installed,
            PackageStatus::Manual,
            PackageStatus::Unavailable,
            PackageStatus::Loading,
        ] {
            let ctx = egui::Context::default();
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    bounds.push(package_status(ui, status, true).rect);
                });
            });
        }
        assert!(bounds.iter().all(|rect| *rect == bounds[0]));
        assert_eq!(bounds[0].size(), Vec2::splat(34.0));
    }
}
