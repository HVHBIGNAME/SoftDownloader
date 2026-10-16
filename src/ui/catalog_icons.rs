use std::collections::BTreeMap;

use anyhow::{Result, ensure};
use eframe::egui::{self, ColorImage, Pos2, Rect, TextureHandle, Vec2};
use serde::Deserialize;

#[derive(Deserialize)]
struct Entry {
    tile: usize,
}

#[derive(Deserialize)]
struct Index {
    columns: usize,
    cell: usize,
    size: usize,
    packages: BTreeMap<String, Entry>,
}

pub(super) struct CatalogIcons {
    texture: TextureHandle,
    regions: BTreeMap<String, Rect>,
}

impl CatalogIcons {
    pub fn new(ctx: &egui::Context) -> Result<Self> {
        let index: Index = serde_json::from_str(include_str!("../../catalog/icons.json"))?;
        let png =
            eframe::icon_data::from_png_bytes(include_bytes!("../../assets/catalog-icons.png"))?;
        ensure!(
            index.columns > 0 && index.size > 0 && index.cell >= index.size && index.cell <= 256,
            "Invalid catalog icon geometry"
        );
        ensure!(
            png.width as usize == index.columns * index.cell
                && (png.height as usize).is_multiple_of(index.cell),
            "Icon atlas dimensions do not match index"
        );
        let image_size = Vec2::new(png.width as f32, png.height as f32);
        let capacity = index.columns * (png.height as usize / index.cell);
        let mut regions = BTreeMap::new();
        for (id, entry) in index.packages {
            ensure!(entry.tile < capacity, "Icon outside atlas: {id}");
            let padding = (index.cell - index.size) as f32 * 0.5;
            let min = Vec2::new(
                (entry.tile % index.columns * index.cell) as f32 + padding,
                (entry.tile / index.columns * index.cell) as f32 + padding,
            );
            let max = min + Vec2::splat(index.size as f32);
            regions.insert(
                id,
                Rect::from_min_max(Pos2::ZERO + min / image_size, Pos2::ZERO + max / image_size),
            );
        }
        let image = ColorImage::from_rgba_unmultiplied(
            [png.width as usize, png.height as usize],
            &png.rgba,
        );
        Ok(Self {
            texture: ctx.load_texture("catalog-product-icons", image, egui::TextureOptions::LINEAR),
            regions,
        })
    }

    pub fn show(&self, ui: &mut egui::Ui, id: &str, size: f32) -> bool {
        let Some(uv) = self.regions.get(id) else {
            return false;
        };
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
        ui.painter()
            .image(self.texture.id(), rect, *uv, egui::Color32::WHITE);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_package_has_an_offline_icon_inside_the_atlas() {
        let ctx = egui::Context::default();
        let icons = CatalogIcons::new(&ctx).unwrap();
        let catalog = crate::catalog::CatalogDocument::builtin().unwrap().catalog;
        assert_eq!(icons.regions.len(), catalog.packages.len());
        for package in catalog.packages {
            let uv = icons
                .regions
                .get(&package.id)
                .unwrap_or_else(|| panic!("Missing icon: {}", package.id));
            assert!(
                uv.is_positive()
                    && uv.min.x >= 0.0
                    && uv.min.y >= 0.0
                    && uv.max.x <= 1.0
                    && uv.max.y <= 1.0
            );
        }
    }
}
