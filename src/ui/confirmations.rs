use eframe::egui::{self, RichText};

use super::theme;
use crate::uninstall::InstalledProgram;

pub(super) fn removal(
    ctx: &egui::Context,
    pending: &mut Option<Vec<InstalledProgram>>,
) -> Option<Vec<InstalledProgram>> {
    let colors = theme::colors(ctx);
    let programs = pending.as_ref()?;
    let mut approved = false;
    let mut cancelled = false;
    let response = egui::Modal::new(egui::Id::new("confirm-removal"))
        .frame(colors.card_frame().inner_margin(24))
        .show(ctx, |ui| {
            ui.set_width(510.0);
            theme::heading(ui, "Удалить программы?", "Проверьте список. Зависимые пакеты удаляются первыми.");
            egui::ScrollArea::vertical().max_height(270.0).show(ui, |ui| {
                for program in programs {
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new(RichText::new(&program.name).strong()).truncate());
                        ui.label(RichText::new(&program.version).color(colors.dim).size(12.0));
                    });
                }
            });
            ui.add_space(14.0);
            ui.label(RichText::new("Будут запущены штатные деинсталляторы. Некоторые откроют свой мастер или запрос UAC.").size(12.0).color(colors.muted));
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                cancelled = ui.button("Оставить программы").clicked();
                approved = ui.add(egui::Button::new(RichText::new(format!("Удалить · {}", programs.len())).strong().color(theme::on_color(colors.red)))
                    .fill(colors.red)).clicked();
            });
        });
    if cancelled || response.should_close() {
        *pending = None;
        return None;
    }
    if approved { pending.take() } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::uninstall::UninstallTarget;

    #[test]
    fn removal_preview_waits_for_confirmation_and_escape_never_approves() {
        let ctx = egui::Context::default();
        let mut pending = Some(vec![InstalledProgram {
            id: "fixture".into(),
            name: "Preview fixture".into(),
            version: "1".into(),
            publisher: String::new(),
            quiet: false,
            target: UninstallTarget::Detected {
                path: "fixture.exe".into(),
            },
            managed_ids: vec![],
            package_ids: vec![],
            icon_path: None,
        }]);
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            assert!(removal(ctx, &mut pending).is_none());
        });
        assert!(pending.is_some());
        let input = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            assert!(removal(ctx, &mut pending).is_none());
        });
        assert!(pending.is_none());
    }
}
