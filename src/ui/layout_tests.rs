use eframe::egui::{self, Pos2, Rect, Vec2};

use super::{app::SoftDownloaderApp, installation_tests::fixture};

struct TextBounds {
    text: String,
    rect: Rect,
    clip: Rect,
    rows: usize,
}

fn render(app: &mut SoftDownloaderApp, ctx: &egui::Context, size: Vec2) -> Vec<TextBounds> {
    let mut texts = Vec::new();
    for _ in 0..3 {
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                focused: true,
                ..Default::default()
            },
            |ctx| app.render(ctx),
        );
        texts.clear();
        for shape in output.shapes {
            collect(&shape.shape, shape.clip_rect, &mut texts);
        }
    }
    texts
}

fn collect(shape: &egui::Shape, clip: Rect, texts: &mut Vec<TextBounds>) {
    match shape {
        egui::Shape::Text(text) => texts.push(TextBounds {
            text: text.galley.text().to_owned(),
            rect: Rect::from_min_size(text.pos, text.galley.size()),
            clip,
            rows: text.galley.rows.len(),
        }),
        egui::Shape::Vec(shapes) => {
            for shape in shapes {
                collect(shape, clip, texts);
            }
        }
        _ => {}
    }
}

fn visible<'a>(texts: &'a [TextBounds], label: &str, size: Vec2) -> &'a TextBounds {
    let screen = Rect::from_min_size(Pos2::ZERO, size);
    texts
        .iter()
        .find(|text| {
            text.text == label
                && text.clip.expand(1.0).contains_rect(text.rect)
                && screen.contains_rect(text.rect)
        })
        .unwrap_or_else(|| {
            let candidates: Vec<_> = texts
                .iter()
                .filter(|text| text.text == label)
                .map(|text| (text.rect, text.clip, text.rows))
                .collect();
            panic!("Control clipped or absent at {size:?}: {label}; bounds={candidates:?}")
        })
}

#[test]
fn selection_controls_and_sidebar_do_not_overlap_at_small_logical_sizes() {
    for size in [Vec2::new(1024.0, 700.0), Vec2::new(819.2, 560.0)] {
        let (_root, ctx, mut app) = fixture(true);
        let texts = render(&mut app, &ctx, size);
        for label in ["Установить · 1", "План", "Тихая установка"] {
            assert_eq!(
                visible(&texts, label, size).rows,
                1,
                "Wrapped button: {label}"
            );
        }
        let help = visible(&texts, "Помощь", size).rect;
        let settings = visible(&texts, "Настройки", size).rect;
        assert!(help.bottom() < settings.top());
        for text in texts
            .iter()
            .filter(|text| text.rect.left() < 224.0 && text.clip.contains_rect(text.rect))
        {
            if [
                "Каталог",
                "Избранное",
                "Установлено",
                "Очередь",
                "Наборы программ",
                "Погода",
            ]
            .contains(&text.text.as_str())
            {
                assert!(
                    text.rect.bottom() < help.top(),
                    "Navigation overlaps footer: {}",
                    text.text
                );
            }
        }
    }
}

#[test]
fn narrow_details_keep_install_action_and_catalog_heading_visible() {
    let size = Vec2::new(819.2, 560.0);
    let (_root, ctx, mut app) = fixture(true);
    app.selected.clear();
    app.details = Some("ui-fixture".into());
    let texts = render(&mut app, &ctx, size);
    for label in [
        "О программе",
        "Установить сейчас",
        "Импорт списка",
        "Обновить",
    ] {
        assert_eq!(visible(&texts, label, size).rows, 1);
    }
    assert!(
        texts.iter().any(|text| text.text == "Каталог"
            && text.rect.left() > 224.0
            && text.clip.contains_rect(text.rect)),
        "Details squeezed out the catalog heading"
    );
}
