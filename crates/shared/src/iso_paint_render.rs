use crate::iso_paint_brush::{self, IsoPaintBrushSample};
use crate::prelude::*;
use rayon::prelude::*;
use scenevm::{
    Atom, Camera3D, CameraKind, PaintSurfaceBuffer, Raster3DPaintGpuStroke,
    Raster3DPaintGpuSurface, SceneVM,
};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use theframework::prelude::*;
use vek::{Mat4, Vec3, Vec4};

const ISO_PAINT_PAR_COMPOSITE_PIXELS: usize = 32_768;

#[derive(Clone)]
struct IsoPaintStrokeRenderCache {
    order: u64,
    origin: [i32; 2],
    screen_anchor: Option<[i32; 2]>,
    world_anchor: Option<[f32; 3]>,
    camera_scale: Option<f32>,
    clip_geo_id: Option<scenevm::GeoId>,
    color_coverage_scale: f32,
    replace_material: bool,
    replace_opacity: u8,
    writes_material: bool,
    brush: String,
    clip: String,
    material_id: u8,
    color: [u8; 4],
    pattern_kind: String,
    pattern_scale: f32,
    pattern_mortar: f32,
    pattern_detail: f32,
    pattern_variation: f32,
    path_points: Vec<[f32; 2]>,
    path_lengths: Vec<f32>,
    erase: bool,
    buffer: TheRGBABuffer,
}

#[derive(Clone)]
struct IsoPaintCachedStrokeRender {
    key: u64,
    strokes: Vec<IsoPaintStrokeRenderCache>,
}

#[derive(Default)]
struct IsoPaintChunkRenderCache {
    revision: u64,
    strokes: Vec<IsoPaintStrokeRenderCache>,
    stroke_caches: HashMap<Uuid, IsoPaintCachedStrokeRender>,
}

#[derive(Clone, Copy)]
enum IsoPaintRenderItem<'a> {
    Stroke(&'a IsoPaintStrokeRenderCache),
    Stamp(&'a IsoPaintStamp),
}

#[derive(Default)]
pub struct IsoPaintRenderCache {
    region_id: Option<Uuid>,
    chunks: HashMap<String, IsoPaintChunkRenderCache>,
    prepared_key: Option<IsoPaintPreparedOverlayKey>,
    prepared_overlay: Option<IsoPaintPreparedOverlay>,
    uploaded_key: Option<IsoPaintPreparedOverlayKey>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct IsoPaintPreparedOverlayKey {
    region_id: Uuid,
    render_context: u8,
    width: i32,
    height: i32,
    layer_key: u64,
    surface_key: u64,
    camera_key: u64,
    camera_scale_bits: u32,
}

#[derive(Clone)]
#[allow(dead_code)]
struct IsoPaintPreparedOverlay {
    width: u32,
    height: u32,
    color_rgba: Vec<u8>,
    material_rgba: Vec<u8>,
    paint_alpha_geo_ids: Vec<scenevm::GeoId>,
}

pub struct IsoPaintRenderer;

#[allow(dead_code)]
impl IsoPaintRenderer {
    fn iso_paint_color_with_opacity(mut color: [u8; 4], opacity: f32) -> [u8; 4] {
        color[3] = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
        color
    }

    fn iso_paint_material_pixel(
        material_id: u8,
        replace_opacity: Option<u8>,
        coverage: u8,
    ) -> [u8; 4] {
        let mode = replace_opacity
            .map(|opacity| opacity.saturating_add(1).max(1))
            .unwrap_or(0);
        [254, material_id, mode, coverage]
    }

    fn iso_paint_set_material_pixel_at(
        material_pixels: &mut [u8],
        index: usize,
        material_id: u8,
        replace_material: bool,
        replace_opacity: u8,
        coverage: u8,
    ) {
        if coverage == 0 || index + 3 >= material_pixels.len() {
            return;
        }
        let existing = material_pixels[index + 3] as u16;
        let src = coverage as u16;
        let out_alpha = (src + (existing * (255 - src)) / 255).min(255) as u8;
        let material = Self::iso_paint_material_pixel(
            material_id,
            replace_material.then_some(replace_opacity),
            out_alpha,
        );
        material_pixels[index..index + 4].copy_from_slice(&material);
    }

    fn iso_paint_clear_material_pixel_at(material_pixels: &mut [u8], index: usize, coverage: u8) {
        if coverage == 0 || index + 3 >= material_pixels.len() {
            return;
        }
        let keep = 255_u16.saturating_sub(coverage as u16);
        let next_alpha = ((material_pixels[index + 3] as u16 * keep) / 255) as u8;
        if next_alpha == 0 {
            material_pixels[index..index + 4]
                .copy_from_slice(&Self::iso_paint_material_pixel(0, None, 0));
        } else {
            material_pixels[index + 3] = next_alpha;
        }
    }

    fn iso_paint_blend_pixel_at(pixels: &mut [u8], index: usize, color: [u8; 4]) {
        if color[3] == 0 || index + 3 >= pixels.len() {
            return;
        }

        let src_a = color[3] as u32;
        let dst_a = pixels[index + 3] as u32;
        let inv_a = 255 - src_a;
        let out_a = (src_a + (dst_a * inv_a) / 255).min(255);
        if out_a == 0 {
            pixels[index..index + 4].copy_from_slice(&[0, 0, 0, 0]);
            return;
        }

        let denom = out_a * 255;
        pixels[index] = ((color[0] as u32 * src_a * 255 + pixels[index] as u32 * dst_a * inv_a)
            / denom)
            .min(255) as u8;
        pixels[index + 1] =
            ((color[1] as u32 * src_a * 255 + pixels[index + 1] as u32 * dst_a * inv_a) / denom)
                .min(255) as u8;
        pixels[index + 2] =
            ((color[2] as u32 * src_a * 255 + pixels[index + 2] as u32 * dst_a * inv_a) / denom)
                .min(255) as u8;
        pixels[index + 3] = out_a as u8;
    }

    fn iso_paint_coat_pixel_at(pixels: &mut [u8], index: usize, color: [u8; 4]) {
        if color[3] == 0 || index + 3 >= pixels.len() {
            return;
        }

        let src_a = color[3] as u32;
        let dst_a = pixels[index + 3] as u32;
        if dst_a == 0 || src_a >= dst_a {
            pixels[index] = color[0];
            pixels[index + 1] = color[1];
            pixels[index + 2] = color[2];
            pixels[index + 3] = color[3];
            return;
        }

        let keep_a = dst_a.saturating_sub(src_a);
        pixels[index] =
            ((color[0] as u32 * src_a + pixels[index] as u32 * keep_a) / dst_a).min(255) as u8;
        pixels[index + 1] =
            ((color[1] as u32 * src_a + pixels[index + 1] as u32 * keep_a) / dst_a).min(255) as u8;
        pixels[index + 2] =
            ((color[2] as u32 * src_a + pixels[index + 2] as u32 * keep_a) / dst_a).min(255) as u8;
        pixels[index + 3] = dst_a as u8;
    }

    fn iso_paint_write_overlay_pixel_at(pixels: &mut [u8], index: usize, color: [u8; 4]) {
        if color[3] == 0 || index + 3 >= pixels.len() || color[3] <= pixels[index + 3] {
            return;
        }

        pixels[index] = color[0];
        pixels[index + 1] = color[1];
        pixels[index + 2] = color[2];
        pixels[index + 3] = color[3];
    }

    fn iso_paint_color_coverage_scale(brush: &str, material_id: u8) -> f32 {
        let family = material_id / 4;
        if brush == "puddle" {
            1.0
        } else if matches!(family, 5 | 6) {
            0.12
        } else {
            1.0
        }
    }

    fn iso_paint_material_is_translucent(material_id: u8) -> bool {
        matches!(material_id / 4, 5 | 6)
    }

    fn iso_paint_alpha_geo_ids(
        material_pixels: &[u8],
        width: usize,
        height: usize,
        paint_surface: Option<&scenevm::PaintSurfaceBuffer>,
    ) -> Vec<scenevm::GeoId> {
        let Some(paint_surface) = paint_surface else {
            return Vec::new();
        };
        let mut seen = HashSet::new();
        let mut geo_ids = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let index = (y * width + x) * 4;
                if index + 3 >= material_pixels.len()
                    || material_pixels[index] != 254
                    || material_pixels[index + 3] == 0
                {
                    continue;
                }
                let material_id = material_pixels[index + 1];
                let replace_mode = material_pixels[index + 2];
                let opaque_replace = replace_mode > 0
                    && replace_mode.saturating_sub(1) == 254
                    && !Self::iso_paint_material_is_translucent(material_id);
                if opaque_replace {
                    continue;
                }
                let Some(pixel) = paint_surface.pixel(x as i32, y as i32) else {
                    continue;
                };
                if pixel.valid && seen.insert(pixel.geo_id) {
                    geo_ids.push(pixel.geo_id);
                }
            }
        }
        geo_ids
    }

    fn iso_paint_set_material_pixel(
        material_pixels: &mut [u8],
        width: usize,
        height: usize,
        x: i32,
        y: i32,
        material_id: u8,
        replace_material: bool,
        replace_opacity: u8,
        coverage: u8,
    ) {
        if x < 0 || y < 0 || x as usize >= width || y as usize >= height || coverage == 0 {
            return;
        }
        let index = (y as usize * width + x as usize) * 4;
        if index + 3 >= material_pixels.len() {
            return;
        }
        Self::iso_paint_set_material_pixel_at(
            material_pixels,
            index,
            material_id,
            replace_material,
            replace_opacity,
            coverage,
        );
    }

    fn iso_paint_set_stamp_material_pixel(
        material_pixels: &mut [u8],
        width: usize,
        height: usize,
        surface_buffer: Option<&scenevm::PaintSurfaceBuffer>,
        owner_geo_id: Option<scenevm::GeoId>,
        x: i32,
        y: i32,
        material_id: u8,
        coverage: u8,
    ) {
        if !Self::iso_paint_stamp_pixel_visible(surface_buffer, None, owner_geo_id, x, y) {
            return;
        }
        Self::iso_paint_set_material_pixel(
            material_pixels,
            width,
            height,
            x,
            y,
            material_id,
            true,
            254,
            coverage,
        );
    }

    fn iso_paint_clear_material_pixel(
        material_pixels: &mut [u8],
        width: usize,
        height: usize,
        x: i32,
        y: i32,
        coverage: u8,
    ) {
        if x < 0 || y < 0 || x as usize >= width || y as usize >= height || coverage == 0 {
            return;
        }
        let index = (y as usize * width + x as usize) * 4;
        if index + 3 >= material_pixels.len() {
            return;
        }
        Self::iso_paint_clear_material_pixel_at(material_pixels, index, coverage);
    }

    fn iso_paint_blend_pixel(
        pixels: &mut [u8],
        width: usize,
        height: usize,
        x: i32,
        y: i32,
        color: [u8; 4],
    ) {
        if x < 0 || y < 0 || x as usize >= width || y as usize >= height || color[3] == 0 {
            return;
        }
        let index = (y as usize * width + x as usize) * 4;
        if index + 3 >= pixels.len() {
            return;
        }
        Self::iso_paint_blend_pixel_at(pixels, index, color);
    }

    fn iso_paint_coat_pixel(
        pixels: &mut [u8],
        width: usize,
        height: usize,
        x: i32,
        y: i32,
        color: [u8; 4],
    ) {
        if x < 0 || y < 0 || x as usize >= width || y as usize >= height || color[3] == 0 {
            return;
        }
        let index = (y as usize * width + x as usize) * 4;
        if index + 3 >= pixels.len() {
            return;
        }
        Self::iso_paint_coat_pixel_at(pixels, index, color);
    }

    fn iso_paint_write_coverage_pixel(
        pixels: &mut [u8],
        width: usize,
        height: usize,
        x: i32,
        y: i32,
        color: [u8; 4],
    ) {
        if x < 0 || y < 0 || x as usize >= width || y as usize >= height || color[3] == 0 {
            return;
        }
        let index = (y as usize * width + x as usize) * 4;
        if index + 3 >= pixels.len() || color[3] <= pixels[index + 3] {
            return;
        }
        Self::iso_paint_write_overlay_pixel_at(pixels, index, color);
    }

    fn iso_paint_write_overlay_pixel(
        pixels: &mut [u8],
        width: usize,
        height: usize,
        x: i32,
        y: i32,
        color: [u8; 4],
    ) {
        if x < 0 || y < 0 || x as usize >= width || y as usize >= height || color[3] == 0 {
            return;
        }
        let index = (y as usize * width + x as usize) * 4;
        if index + 3 >= pixels.len() || color[3] <= pixels[index + 3] {
            return;
        }

        pixels[index] = color[0];
        pixels[index + 1] = color[1];
        pixels[index + 2] = color[2];
        pixels[index + 3] = color[3];
    }

    fn iso_paint_stamp_coverage(
        pixels: &mut [u8],
        width: usize,
        height: usize,
        local_x: i32,
        local_y: i32,
        radius: i32,
        color: [u8; 4],
        palette: &[[u8; 4]],
        brush: &str,
        shape: &str,
        seed: u32,
    ) {
        let radius = radius.max(1);
        let sample = IsoPaintBrushSample {
            brush,
            shape,
            color,
            palette,
            opacity: 1.0,
            radius,
            seed,
        };
        for oy in -radius..=radius {
            for ox in -radius..=radius {
                let Some(shaped_color) = iso_paint_brush::sample_pixel(&sample, ox, oy) else {
                    continue;
                };
                Self::iso_paint_write_coverage_pixel(
                    pixels,
                    width,
                    height,
                    local_x + ox,
                    local_y + oy,
                    shaped_color,
                );
            }
        }
    }

    fn iso_paint_draw_segment_coverage(
        pixels: &mut [u8],
        width: usize,
        height: usize,
        a: [i32; 2],
        b: [i32; 2],
        origin: [i32; 2],
        radius: i32,
        color: [u8; 4],
        palette: &[[u8; 4]],
        brush: &str,
        shape: &str,
        seed: u32,
    ) {
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let distance = ((dx * dx + dy * dy) as f32).sqrt();
        let step_spacing = (radius as f32 * 0.35).clamp(1.0, 10.0);
        let steps = (distance / step_spacing).ceil().max(1.0) as i32;
        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            let x = (a[0] as f32 + dx as f32 * t).round() as i32;
            let y = (a[1] as f32 + dy as f32 * t).round() as i32;
            Self::iso_paint_stamp_coverage(
                pixels,
                width,
                height,
                x - origin[0],
                y - origin[1],
                radius,
                color,
                palette,
                brush,
                shape,
                seed ^ (step as u32).wrapping_mul(0x27d4_eb2d),
            );
        }
    }

    fn iso_paint_sample_brick_color(
        pattern_x: f32,
        pattern_y: f32,
        base: [u8; 4],
        pattern_kind: &str,
        pattern_scale: f32,
        pattern_mortar: f32,
        pattern_detail: f32,
        pattern_variation: f32,
    ) -> [u8; 4] {
        let pattern_scale = pattern_scale.clamp(0.25, 4.0);
        let pattern_mortar = pattern_mortar.clamp(0.0, 0.4);
        let pattern_detail = pattern_detail.clamp(0.0, 1.0);
        let pattern_variation = pattern_variation.clamp(0.0, 1.0);
        let staggered = !matches!(pattern_kind, "tile" | "tiles");
        let brick_w = if staggered { 34.0 } else { 24.0 } * pattern_scale;
        let brick_h = if staggered { 17.0 } else { 24.0 } * pattern_scale;
        let mortar =
            (brick_w.min(brick_h) * pattern_mortar).clamp(0.0, brick_w.min(brick_h) * 0.45);

        let row = (pattern_y / brick_h).floor();
        let offset_x = if staggered && row as i32 & 1 != 0 {
            brick_w * 0.5
        } else {
            0.0
        };
        let local_x = (pattern_x + offset_x).rem_euclid(brick_w);
        let local_y = pattern_y.rem_euclid(brick_h);
        let col = ((pattern_x + offset_x) / brick_w).floor() as i32;
        let row_i = row as i32;

        let hash = |x: i32, y: i32, salt: i32| -> f32 {
            let mut n = x
                .wrapping_mul(374_761_393)
                .wrapping_add(y.wrapping_mul(668_265_263))
                .wrapping_add(salt.wrapping_mul(2_147_483_647));
            n = (n ^ (n >> 13)).wrapping_mul(1_274_126_177);
            ((n ^ (n >> 16)) & 0xffff) as f32 / 65_535.0
        };

        if local_x < mortar || local_y < mortar {
            return [base[0], base[1], base[2], 0];
        }

        let edge_distance = local_x
            .min(local_y)
            .min(brick_w - local_x)
            .min(brick_h - local_y);
        let edge_wear = if edge_distance < mortar + 1.6 {
            1.0 - 0.12 * pattern_detail + hash(col, row_i, 31) * 0.06 * pattern_detail
        } else {
            1.0
        };
        let brick_variation = 1.0 + (hash(col, row_i, 11) - 0.5) * 0.44 * pattern_variation;
        let grain = 1.0
            + (hash(
                pattern_x.floor() as i32,
                pattern_y.floor() as i32,
                col.wrapping_mul(19) ^ row_i.wrapping_mul(23),
            ) - 0.5)
                * 0.20
                * pattern_detail;
        let hairline = if (local_y - mortar).abs() < 1.0 || (local_x - mortar).abs() < 0.8 {
            1.0 - 0.07 * pattern_detail
        } else {
            1.0
        };
        let shade = brick_variation * grain * edge_wear * hairline;
        [
            (base[0] as f32 * shade).clamp(0.0, 255.0) as u8,
            (base[1] as f32 * shade).clamp(0.0, 255.0) as u8,
            (base[2] as f32 * shade).clamp(0.0, 255.0) as u8,
            base[3],
        ]
    }

    fn iso_paint_sample_brick_surface_color(
        surface_uv: [f32; 2],
        base: [u8; 4],
        pattern_kind: &str,
        pattern_scale: f32,
        pattern_mortar: f32,
        pattern_detail: f32,
        pattern_variation: f32,
    ) -> [u8; 4] {
        let pixels_per_world = 42.0;
        Self::iso_paint_sample_brick_color(
            surface_uv[0] * pixels_per_world,
            surface_uv[1] * pixels_per_world,
            base,
            pattern_kind,
            pattern_scale,
            pattern_mortar,
            pattern_detail,
            pattern_variation,
        )
    }

    fn iso_paint_path_pattern_coord(
        screen: [i32; 2],
        path_points: &[[f32; 2]],
        path_lengths: &[f32],
        origin: [i32; 2],
        scale: f32,
    ) -> Option<[f32; 2]> {
        if path_points.len() < 2 || path_lengths.len() != path_points.len() {
            return None;
        }

        let px = screen[0] as f32;
        let py = screen[1] as f32;
        let scale = scale.clamp(0.05, 20.0);
        let mut best: Option<(f32, f32, f32)> = None;

        for index in 0..path_points.len().saturating_sub(1) {
            let a = path_points[index];
            let b = path_points[index + 1];
            let ax = origin[0] as f32 + a[0] * scale;
            let ay = origin[1] as f32 + a[1] * scale;
            let bx = origin[0] as f32 + b[0] * scale;
            let by = origin[1] as f32 + b[1] * scale;
            let vx = bx - ax;
            let vy = by - ay;
            let len2 = vx * vx + vy * vy;
            if len2 <= f32::EPSILON {
                continue;
            }
            let t = (((px - ax) * vx + (py - ay) * vy) / len2).clamp(0.0, 1.0);
            let qx = ax + vx * t;
            let qy = ay + vy * t;
            let dx = px - qx;
            let dy = py - qy;
            let dist2 = dx * dx + dy * dy;
            let segment_len = len2.sqrt();
            let along = path_lengths[index] * scale + segment_len * t;
            let signed_across = (vx * dy - vy * dx).signum() * dist2.sqrt();
            if best.map_or(true, |(best_dist2, _, _)| dist2 < best_dist2) {
                best = Some((dist2, along, signed_across));
            }
        }

        best.map(|(_, along, across)| [along, across])
    }

    fn iso_paint_arch_pattern_coord(
        screen: [i32; 2],
        path_points: &[[f32; 2]],
        path_lengths: &[f32],
        origin: [i32; 2],
        scale: f32,
    ) -> Option<[f32; 2]> {
        let coord =
            Self::iso_paint_path_pattern_coord(screen, path_points, path_lengths, origin, scale)?;
        Some([coord[0], coord[1] + 8192.0])
    }

    fn iso_paint_sample_arch_brick_color(
        screen: [i32; 2],
        path_points: &[[f32; 2]],
        path_lengths: &[f32],
        origin: [i32; 2],
        scale: f32,
        base: [u8; 4],
        pattern_scale: f32,
        pattern_mortar: f32,
        pattern_detail: f32,
        pattern_variation: f32,
    ) -> Option<[u8; 4]> {
        let coord =
            Self::iso_paint_arch_pattern_coord(screen, path_points, path_lengths, origin, scale)?;
        Some(Self::iso_paint_sample_brick_color(
            coord[0],
            coord[1],
            base,
            "tile",
            pattern_scale,
            pattern_mortar,
            pattern_detail,
            pattern_variation,
        ))
    }

    fn iso_paint_geo_object_matches(a: scenevm::GeoId, b: scenevm::GeoId) -> bool {
        match (a, b) {
            (scenevm::GeoId::GeometryObject(a), scenevm::GeoId::GeometryObject(b)) => a == b,
            (scenevm::GeoId::Sector(a), scenevm::GeoId::Sector(b)) => a == b,
            (scenevm::GeoId::Terrain(..), scenevm::GeoId::Terrain(..)) => true,
            (scenevm::GeoId::Character(a), scenevm::GeoId::Character(b)) => a == b,
            (scenevm::GeoId::Item(a), scenevm::GeoId::Item(b)) => a == b,
            (scenevm::GeoId::Triangle(a), scenevm::GeoId::Triangle(b)) => a == b,
            _ => a == b,
        }
    }

    fn iso_paint_owner_geo_id(owner: &IsoPaintOwner) -> scenevm::GeoId {
        match owner {
            IsoPaintOwner::Unknown(id) => scenevm::GeoId::Unknown(*id),
            IsoPaintOwner::Vertex(id) => scenevm::GeoId::Vertex(*id),
            IsoPaintOwner::Linedef(id) => scenevm::GeoId::Linedef(*id),
            IsoPaintOwner::Sector(id) => scenevm::GeoId::Sector(*id),
            IsoPaintOwner::Character(id) => scenevm::GeoId::Character(*id),
            IsoPaintOwner::Item(id) => scenevm::GeoId::Item(*id),
            IsoPaintOwner::Light(id) => scenevm::GeoId::Light(*id),
            IsoPaintOwner::ItemLight(id) => scenevm::GeoId::ItemLight(*id),
            IsoPaintOwner::Triangle(id) => scenevm::GeoId::Triangle(*id),
            IsoPaintOwner::Terrain { x, z } => scenevm::GeoId::Terrain(*x, *z),
            IsoPaintOwner::GeometryObject(id) => scenevm::GeoId::GeometryObject(*id),
            IsoPaintOwner::Hole { sector_id, hole_id } => {
                scenevm::GeoId::Hole(*sector_id, *hole_id)
            }
            IsoPaintOwner::Gizmo(id) => scenevm::GeoId::Gizmo(*id),
        }
    }

    fn iso_paint_start_clip_geo_id(
        surface_buffer: Option<&scenevm::PaintSurfaceBuffer>,
        clip: &str,
        clip_geo_id: Option<scenevm::GeoId>,
        start_screen: Option<[i32; 2]>,
    ) -> Option<scenevm::GeoId> {
        if clip == "none" {
            return None;
        }
        if clip_geo_id.is_some() {
            return clip_geo_id;
        }
        let start_screen = start_screen?;
        surface_buffer?
            .pixel(start_screen[0], start_screen[1])
            .copied()
            .filter(|pixel| pixel.valid)
            .map(|pixel| pixel.geo_id)
    }

    fn iso_paint_brush_clip_geo_id(
        surface_buffer: Option<&scenevm::PaintSurfaceBuffer>,
        clip: &str,
        clip_geo_id: Option<scenevm::GeoId>,
        start_screen: Option<[i32; 2]>,
        paint: &TheRGBABuffer,
        draw_origin: [i32; 2],
        scale: f32,
    ) -> Option<scenevm::GeoId> {
        let center_geo_id =
            Self::iso_paint_start_clip_geo_id(surface_buffer, clip, clip_geo_id, start_screen);
        if clip == "none" {
            return None;
        }
        if let Some(stored_geo_id) = clip_geo_id {
            return Some(stored_geo_id);
        }

        let Some(surface_buffer) = surface_buffer else {
            return center_geo_id;
        };
        let paint_dim = *paint.dim();
        if paint_dim.width <= 0 || paint_dim.height <= 0 {
            return center_geo_id;
        }

        let scale = scale.clamp(0.05, 20.0);
        let paint_w = paint_dim.width as usize;
        let paint_h = paint_dim.height as usize;
        let draw_w = ((paint_dim.width as f32) * scale).round().max(1.0) as usize;
        let draw_h = ((paint_dim.height as f32) * scale).round().max(1.0) as usize;
        let paint_pixels = paint.pixels();
        let mut weights: HashMap<scenevm::GeoId, usize> = HashMap::new();
        for gy in 0..draw_h {
            let sy = ((gy as f32) / scale).floor() as usize;
            if sy >= paint_h {
                continue;
            }
            let dst_y = draw_origin[1] + gy as i32;
            for gx in 0..draw_w {
                let sx = ((gx as f32) / scale).floor() as usize;
                if sx >= paint_w {
                    continue;
                }
                let src_index = (sy * paint_w + sx) * 4;
                let Some(alpha) = paint_pixels.get(src_index + 3).copied() else {
                    continue;
                };
                if alpha == 0 {
                    continue;
                }
                let dst_x = draw_origin[0] + gx as i32;
                if let Some(pixel) = surface_buffer.pixel(dst_x, dst_y)
                    && pixel.valid
                {
                    *weights.entry(pixel.geo_id).or_insert(0) += alpha as usize;
                }
            }
        }

        let dominant = weights
            .iter()
            .max_by_key(|(_, weight)| *weight)
            .map(|(geo_id, weight)| (*geo_id, *weight));
        let Some((dominant_geo_id, dominant_weight)) = dominant else {
            return center_geo_id;
        };
        let center_weight = center_geo_id
            .and_then(|geo_id| weights.get(&geo_id).copied())
            .unwrap_or(0);

        let chosen = if center_weight == 0 || dominant_weight > center_weight.saturating_mul(2) {
            Some(dominant_geo_id)
        } else {
            center_geo_id
        };
        chosen
    }

    fn iso_paint_clip_allows(
        surface_buffer: Option<&scenevm::PaintSurfaceBuffer>,
        clip: &str,
        start_geo_id: Option<scenevm::GeoId>,
        x: i32,
        y: i32,
    ) -> bool {
        match clip {
            "none" => true,
            _ => {
                let Some(start_geo_id) = start_geo_id else {
                    return false;
                };
                surface_buffer
                    .and_then(|surface| surface.pixel(x, y))
                    .is_some_and(|pixel| {
                        pixel.valid
                            && Self::iso_paint_geo_object_matches(start_geo_id, pixel.geo_id)
                    })
            }
        }
    }

    fn iso_paint_composite_overlay_scaled_at(
        target: &mut TheRGBABuffer,
        material_pixels: &mut [u8],
        paint: &TheRGBABuffer,
        surface_buffer: Option<&scenevm::PaintSurfaceBuffer>,
        clip: &str,
        material_id: u8,
        start_screen: Option<[i32; 2]>,
        clip_geo_id: Option<scenevm::GeoId>,
        color_coverage_scale: f32,
        replace_material: bool,
        replace_opacity: u8,
        writes_material: bool,
        x: i32,
        y: i32,
        scale: f32,
    ) {
        let target_dim = *target.dim();
        let paint_dim = *paint.dim();
        if target_dim.width <= 0
            || target_dim.height <= 0
            || paint_dim.width <= 0
            || paint_dim.height <= 0
        {
            return;
        }

        let scale = scale.clamp(0.05, 20.0);
        let target_w = target_dim.width as usize;
        let target_h = target_dim.height as usize;
        let paint_w = paint_dim.width as usize;
        let paint_h = paint_dim.height as usize;
        let draw_w = ((paint_dim.width as f32) * scale).round().max(1.0) as usize;
        let draw_h = ((paint_dim.height as f32) * scale).round().max(1.0) as usize;
        let target_pixels = target.pixels_mut();
        let paint_pixels = paint.pixels();
        let start_geo_id = Self::iso_paint_brush_clip_geo_id(
            surface_buffer,
            clip,
            clip_geo_id,
            start_screen,
            paint,
            [x, y],
            scale,
        );
        let draw_area = draw_w.saturating_mul(draw_h);

        if draw_area >= ISO_PAINT_PAR_COMPOSITE_PIXELS {
            let row_stride = target_w * 4;
            let color_coverage_scale = color_coverage_scale.clamp(0.0, 1.0);
            target_pixels
                .par_chunks_exact_mut(row_stride)
                .zip(material_pixels.par_chunks_exact_mut(row_stride))
                .enumerate()
                .for_each(|(dy, (target_row, material_row))| {
                    let dy = dy as i32;
                    let dy_local = dy - y;
                    if dy_local < 0 || dy_local >= draw_h as i32 {
                        return;
                    }
                    let sy = ((dy_local as f32) / scale).floor() as usize;
                    if sy >= paint_h {
                        return;
                    }

                    let dx_start = x.max(0);
                    let dx_end = (x + draw_w as i32).min(target_dim.width);
                    for dx in dx_start..dx_end {
                        let dx_local = dx - x;
                        let sx = ((dx_local as f32) / scale).floor() as usize;
                        if sx >= paint_w
                            || !Self::iso_paint_clip_allows(
                                surface_buffer,
                                clip,
                                start_geo_id,
                                dx,
                                dy,
                            )
                        {
                            continue;
                        }

                        let src_index = (sy * paint_w + sx) * 4;
                        if src_index + 3 >= paint_pixels.len() {
                            continue;
                        }
                        let src = [
                            paint_pixels[src_index],
                            paint_pixels[src_index + 1],
                            paint_pixels[src_index + 2],
                            paint_pixels[src_index + 3],
                        ];
                        if src[3] == 0 {
                            continue;
                        }

                        let row_index = dx as usize * 4;
                        let mut color_src = src;
                        color_src[3] = ((color_src[3] as f32 * color_coverage_scale)
                            .round()
                            .clamp(0.0, 255.0)) as u8;
                        if color_src[3] > 0 && replace_material {
                            Self::iso_paint_write_overlay_pixel_at(
                                target_row, row_index, color_src,
                            );
                        } else if color_src[3] > 0 {
                            Self::iso_paint_coat_pixel_at(target_row, row_index, color_src);
                        }
                        if writes_material {
                            Self::iso_paint_set_material_pixel_at(
                                material_row,
                                row_index,
                                material_id,
                                replace_material,
                                replace_opacity,
                                src[3],
                            );
                        }
                    }
                });
            return;
        }

        for dy_local in 0..draw_h {
            let dy = y + dy_local as i32;
            if dy < 0 || dy >= target_dim.height {
                continue;
            }
            let sy = ((dy_local as f32) / scale).floor() as usize;
            if sy >= paint_h {
                continue;
            }
            for dx_local in 0..draw_w {
                let dx = x + dx_local as i32;
                if dx < 0 || dx >= target_dim.width {
                    continue;
                }
                let sx = ((dx_local as f32) / scale).floor() as usize;
                if sx >= paint_w {
                    continue;
                }
                if !Self::iso_paint_clip_allows(surface_buffer, clip, start_geo_id, dx, dy) {
                    continue;
                }

                let src_index = (sy * paint_w + sx) * 4;
                if src_index + 3 >= paint_pixels.len() {
                    continue;
                }
                let src = [
                    paint_pixels[src_index],
                    paint_pixels[src_index + 1],
                    paint_pixels[src_index + 2],
                    paint_pixels[src_index + 3],
                ];
                if src[3] == 0 {
                    continue;
                }
                let mut color_src = src;
                color_src[3] = ((color_src[3] as f32 * color_coverage_scale.clamp(0.0, 1.0))
                    .round()
                    .clamp(0.0, 255.0)) as u8;
                if color_src[3] > 0 && replace_material {
                    Self::iso_paint_write_overlay_pixel(
                        target_pixels,
                        target_w,
                        target_h,
                        dx,
                        dy,
                        color_src,
                    );
                } else if color_src[3] > 0 {
                    Self::iso_paint_coat_pixel(
                        target_pixels,
                        target_w,
                        target_h,
                        dx,
                        dy,
                        color_src,
                    );
                }
                if writes_material {
                    Self::iso_paint_set_material_pixel(
                        material_pixels,
                        target_w,
                        target_h,
                        dx,
                        dy,
                        material_id,
                        replace_material,
                        replace_opacity,
                        src[3],
                    );
                }
            }
        }
    }

    fn iso_paint_composite_brick_overlay_scaled_at(
        target: &mut TheRGBABuffer,
        material_pixels: &mut [u8],
        mask: &TheRGBABuffer,
        surface_buffer: Option<&scenevm::PaintSurfaceBuffer>,
        clip: &str,
        material_id: u8,
        start_screen: Option<[i32; 2]>,
        clip_geo_id: Option<scenevm::GeoId>,
        replace_material: bool,
        replace_opacity: u8,
        x: i32,
        y: i32,
        scale: f32,
        base: [u8; 4],
        pattern_kind: &str,
        pattern_scale: f32,
        pattern_mortar: f32,
        pattern_detail: f32,
        pattern_variation: f32,
        path_points: &[[f32; 2]],
        path_lengths: &[f32],
    ) {
        let target_dim = *target.dim();
        let mask_dim = *mask.dim();
        if target_dim.width <= 0
            || target_dim.height <= 0
            || mask_dim.width <= 0
            || mask_dim.height <= 0
        {
            return;
        }

        let Some(surface_buffer) = surface_buffer else {
            return;
        };

        let scale = scale.clamp(0.05, 20.0);
        let target_w = target_dim.width as usize;
        let target_h = target_dim.height as usize;
        let mask_w = mask_dim.width as usize;
        let mask_h = mask_dim.height as usize;
        let draw_w = ((mask_dim.width as f32) * scale).round().max(1.0) as usize;
        let draw_h = ((mask_dim.height as f32) * scale).round().max(1.0) as usize;
        let target_pixels = target.pixels_mut();
        let mask_pixels = mask.pixels();
        let start_geo_id = Self::iso_paint_brush_clip_geo_id(
            Some(surface_buffer),
            clip,
            clip_geo_id,
            start_screen,
            mask,
            [x, y],
            scale,
        );
        let draw_area = draw_w.saturating_mul(draw_h);

        if draw_area >= ISO_PAINT_PAR_COMPOSITE_PIXELS {
            let row_stride = target_w * 4;
            target_pixels
                .par_chunks_exact_mut(row_stride)
                .zip(material_pixels.par_chunks_exact_mut(row_stride))
                .enumerate()
                .for_each(|(dy, (target_row, material_row))| {
                    let dy = dy as i32;
                    let dy_local = dy - y;
                    if dy_local < 0 || dy_local >= draw_h as i32 {
                        return;
                    }
                    let sy = ((dy_local as f32) / scale).floor() as usize;
                    if sy >= mask_h {
                        return;
                    }

                    let dx_start = x.max(0);
                    let dx_end = (x + draw_w as i32).min(target_dim.width);
                    for dx in dx_start..dx_end {
                        let dx_local = dx - x;
                        let sx = ((dx_local as f32) / scale).floor() as usize;
                        if sx >= mask_w
                            || !Self::iso_paint_clip_allows(
                                Some(surface_buffer),
                                clip,
                                start_geo_id,
                                dx,
                                dy,
                            )
                        {
                            continue;
                        }

                        let src_index = (sy * mask_w + sx) * 4;
                        if src_index + 3 >= mask_pixels.len() {
                            continue;
                        }
                        let mask_alpha = mask_pixels[src_index + 3];
                        if mask_alpha == 0 {
                            continue;
                        }
                        let Some(surface_pixel) =
                            surface_buffer.pixel(dx, dy).filter(|pixel| pixel.valid)
                        else {
                            continue;
                        };
                        let mut color = if matches!(pattern_kind, "arch" | "trim") {
                            Self::iso_paint_sample_arch_brick_color(
                                [dx, dy],
                                path_points,
                                path_lengths,
                                [x, y],
                                scale,
                                base,
                                pattern_scale,
                                pattern_mortar,
                                pattern_detail,
                                pattern_variation,
                            )
                            .unwrap_or_else(|| {
                                Self::iso_paint_sample_brick_surface_color(
                                    surface_pixel.uv,
                                    base,
                                    "brick",
                                    pattern_scale,
                                    pattern_mortar,
                                    pattern_detail,
                                    pattern_variation,
                                )
                            })
                        } else {
                            Self::iso_paint_sample_brick_surface_color(
                                surface_pixel.uv,
                                base,
                                pattern_kind,
                                pattern_scale,
                                pattern_mortar,
                                pattern_detail,
                                pattern_variation,
                            )
                        };
                        let color_alpha = ((color[3] as u16 * mask_alpha as u16) / 255) as u8;
                        color[3] = if replace_material {
                            ((color_alpha as u16 * replace_opacity as u16) / 254) as u8
                        } else {
                            color_alpha
                        };

                        let row_index = dx as usize * 4;
                        if color[3] > 0 {
                            if replace_material {
                                Self::iso_paint_write_overlay_pixel_at(
                                    target_row, row_index, color,
                                );
                            } else {
                                Self::iso_paint_coat_pixel_at(target_row, row_index, color);
                            }
                        }
                        Self::iso_paint_set_material_pixel_at(
                            material_row,
                            row_index,
                            material_id,
                            replace_material,
                            replace_opacity,
                            mask_alpha,
                        );
                    }
                });
            return;
        }

        for dy_local in 0..draw_h {
            let dy = y + dy_local as i32;
            if dy < 0 || dy >= target_dim.height {
                continue;
            }
            let sy = ((dy_local as f32) / scale).floor() as usize;
            if sy >= mask_h {
                continue;
            }
            for dx_local in 0..draw_w {
                let dx = x + dx_local as i32;
                if dx < 0 || dx >= target_dim.width {
                    continue;
                }
                let sx = ((dx_local as f32) / scale).floor() as usize;
                if sx >= mask_w {
                    continue;
                }
                if !Self::iso_paint_clip_allows(Some(surface_buffer), clip, start_geo_id, dx, dy) {
                    continue;
                }

                let src_index = (sy * mask_w + sx) * 4;
                if src_index + 3 >= mask_pixels.len() {
                    continue;
                }
                let mask_alpha = mask_pixels[src_index + 3];
                if mask_alpha == 0 {
                    continue;
                }
                let Some(surface_pixel) = surface_buffer.pixel(dx, dy).filter(|pixel| pixel.valid)
                else {
                    continue;
                };
                let mut color = if matches!(pattern_kind, "arch" | "trim") {
                    Self::iso_paint_sample_arch_brick_color(
                        [dx, dy],
                        path_points,
                        path_lengths,
                        [x, y],
                        scale,
                        base,
                        pattern_scale,
                        pattern_mortar,
                        pattern_detail,
                        pattern_variation,
                    )
                    .unwrap_or_else(|| {
                        Self::iso_paint_sample_brick_surface_color(
                            surface_pixel.uv,
                            base,
                            "brick",
                            pattern_scale,
                            pattern_mortar,
                            pattern_detail,
                            pattern_variation,
                        )
                    })
                } else {
                    Self::iso_paint_sample_brick_surface_color(
                        surface_pixel.uv,
                        base,
                        pattern_kind,
                        pattern_scale,
                        pattern_mortar,
                        pattern_detail,
                        pattern_variation,
                    )
                };
                let color_alpha = ((color[3] as u16 * mask_alpha as u16) / 255) as u8;
                color[3] = if replace_material {
                    ((color_alpha as u16 * replace_opacity as u16) / 254) as u8
                } else {
                    color_alpha
                };
                if color[3] > 0 {
                    if replace_material {
                        Self::iso_paint_write_overlay_pixel(
                            target_pixels,
                            target_w,
                            target_h,
                            dx,
                            dy,
                            color,
                        );
                    } else {
                        Self::iso_paint_coat_pixel(
                            target_pixels,
                            target_w,
                            target_h,
                            dx,
                            dy,
                            color,
                        );
                    }
                }
                Self::iso_paint_set_material_pixel(
                    material_pixels,
                    target_w,
                    target_h,
                    dx,
                    dy,
                    material_id,
                    replace_material,
                    replace_opacity,
                    mask_alpha,
                );
            }
        }
    }

    fn iso_paint_clear_overlay_scaled_at(
        target: &mut TheRGBABuffer,
        material_pixels: &mut [u8],
        mask: &TheRGBABuffer,
        surface_buffer: Option<&scenevm::PaintSurfaceBuffer>,
        clip: &str,
        start_screen: Option<[i32; 2]>,
        clip_geo_id: Option<scenevm::GeoId>,
        clears_material: bool,
        x: i32,
        y: i32,
        scale: f32,
    ) {
        let target_dim = *target.dim();
        let mask_dim = *mask.dim();
        if target_dim.width <= 0
            || target_dim.height <= 0
            || mask_dim.width <= 0
            || mask_dim.height <= 0
        {
            return;
        }

        let scale = scale.clamp(0.05, 20.0);
        let target_w = target_dim.width as usize;
        let target_h = target_dim.height as usize;
        let mask_w = mask_dim.width as usize;
        let mask_h = mask_dim.height as usize;
        let draw_w = ((mask_dim.width as f32) * scale).round().max(1.0) as usize;
        let draw_h = ((mask_dim.height as f32) * scale).round().max(1.0) as usize;
        let target_pixels = target.pixels_mut();
        let mask_pixels = mask.pixels();
        let start_geo_id = Self::iso_paint_brush_clip_geo_id(
            surface_buffer,
            clip,
            clip_geo_id,
            start_screen,
            mask,
            [x, y],
            scale,
        );
        let draw_area = draw_w.saturating_mul(draw_h);

        if draw_area >= ISO_PAINT_PAR_COMPOSITE_PIXELS {
            let row_stride = target_w * 4;
            target_pixels
                .par_chunks_exact_mut(row_stride)
                .zip(material_pixels.par_chunks_exact_mut(row_stride))
                .enumerate()
                .for_each(|(dy, (target_row, material_row))| {
                    let dy = dy as i32;
                    let dy_local = dy - y;
                    if dy_local < 0 || dy_local >= draw_h as i32 {
                        return;
                    }
                    let sy = ((dy_local as f32) / scale).floor() as usize;
                    if sy >= mask_h {
                        return;
                    }

                    let dx_start = x.max(0);
                    let dx_end = (x + draw_w as i32).min(target_dim.width);
                    for dx in dx_start..dx_end {
                        let dx_local = dx - x;
                        let sx = ((dx_local as f32) / scale).floor() as usize;
                        if sx >= mask_w
                            || !Self::iso_paint_clip_allows(
                                surface_buffer,
                                clip,
                                start_geo_id,
                                dx,
                                dy,
                            )
                        {
                            continue;
                        }

                        let src_index = (sy * mask_w + sx) * 4;
                        if src_index + 3 >= mask_pixels.len() {
                            continue;
                        }
                        let mask_a = mask_pixels[src_index + 3] as u16;
                        if mask_a == 0 {
                            continue;
                        }
                        let row_index = dx as usize * 4;
                        if row_index + 3 >= target_row.len() {
                            continue;
                        }
                        let keep = 255 - mask_a;
                        target_row[row_index + 3] =
                            ((target_row[row_index + 3] as u16 * keep) / 255) as u8;
                        if clears_material {
                            Self::iso_paint_clear_material_pixel_at(
                                material_row,
                                row_index,
                                mask_pixels[src_index + 3],
                            );
                        }
                    }
                });
            return;
        }

        for dy_local in 0..draw_h {
            let dy = y + dy_local as i32;
            if dy < 0 || dy >= target_dim.height {
                continue;
            }
            let sy = ((dy_local as f32) / scale).floor() as usize;
            if sy >= mask_h {
                continue;
            }
            for dx_local in 0..draw_w {
                let dx = x + dx_local as i32;
                if dx < 0 || dx >= target_dim.width {
                    continue;
                }
                let sx = ((dx_local as f32) / scale).floor() as usize;
                if sx >= mask_w {
                    continue;
                }
                if !Self::iso_paint_clip_allows(surface_buffer, clip, start_geo_id, dx, dy) {
                    continue;
                }

                let src_index = (sy * mask_w + sx) * 4;
                let dst_index = (dy as usize * target_w + dx as usize) * 4;
                if src_index + 3 >= mask_pixels.len() || dst_index + 3 >= target_pixels.len() {
                    continue;
                }
                let mask_a = mask_pixels[src_index + 3] as u16;
                if mask_a == 0 {
                    continue;
                }
                let keep = 255 - mask_a;
                target_pixels[dst_index + 3] =
                    ((target_pixels[dst_index + 3] as u16 * keep) / 255) as u8;
                if clears_material {
                    Self::iso_paint_clear_material_pixel(
                        material_pixels,
                        target_w,
                        target_h,
                        dx,
                        dy,
                        mask_pixels[src_index + 3],
                    );
                }
            }
        }
    }

    fn iso_paint_preview_color(layer: &IsoPaintLayer) -> [u8; 4] {
        match layer.active_operation.as_str() {
            "erase" => [242, 92, 84, 230],
            "pick" => [87, 186, 255, 230],
            _ => {
                let mut color = layer.active_color;
                color[3] = 230;
                color
            }
        }
    }

    fn draw_iso_paint_preview(
        buffer: &mut TheRGBABuffer,
        layer: &IsoPaintLayer,
        hover: Option<Vec2<i32>>,
    ) {
        if !layer.visible || layer.active_operation == "pick" && hover.is_none() {
            return;
        }

        let Some(hover) = hover else {
            return;
        };
        let dim = *buffer.dim();
        if dim.width <= 0 || dim.height <= 0 {
            return;
        }

        let radius = (layer.active_size * 2.0).round().clamp(3.0, 96.0) as i32;
        let outer = radius + 2;
        let radius_sq = radius * radius;
        let inner_sq = (radius - 2).max(1).pow(2);
        let shadow_sq = outer * outer;
        let color = Self::iso_paint_preview_color(layer);
        let fill = [color[0], color[1], color[2], 38];
        let shadow = [8, 10, 12, 145];
        let pixels = buffer.pixels_mut();
        let width = dim.width as usize;
        let height = dim.height as usize;

        for oy in -outer..=outer {
            for ox in -outer..=outer {
                let d = ox * ox + oy * oy;
                let x = hover.x + ox;
                let y = hover.y + oy;
                if d <= shadow_sq && d > radius_sq {
                    Self::iso_paint_blend_pixel(pixels, width, height, x, y, shadow);
                } else if d <= radius_sq && d >= inner_sq {
                    Self::iso_paint_blend_pixel(pixels, width, height, x, y, color);
                } else if d < inner_sq && layer.active_operation != "pick" {
                    Self::iso_paint_blend_pixel(pixels, width, height, x, y, fill);
                }
            }
        }
    }

    fn iso_paint_project_world(
        point: [f32; 3],
        view: Mat4<f32>,
        proj: Mat4<f32>,
        width: i32,
        height: i32,
    ) -> Option<[i32; 2]> {
        if width <= 0 || height <= 0 {
            return None;
        }
        let clip = (proj * view) * Vec4::new(point[0], point[1], point[2], 1.0);
        if clip.w.abs() <= f32::EPSILON {
            return None;
        }
        let ndc = Vec3::new(clip.x / clip.w, clip.y / clip.w, clip.z / clip.w);
        Some([
            ((ndc.x * 0.5 + 0.5) * width as f32).round() as i32,
            ((1.0 - (ndc.y * 0.5 + 0.5)) * height as f32).round() as i32,
        ])
    }

    fn iso_paint_blend_line(
        buffer: &mut TheRGBABuffer,
        surface_buffer: Option<&scenevm::PaintSurfaceBuffer>,
        stamp_depth: Option<f32>,
        owner_geo_id: Option<scenevm::GeoId>,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        color: [u8; 4],
    ) {
        let dim = *buffer.dim();
        if dim.width <= 0 || dim.height <= 0 {
            return;
        }
        let width = dim.width as usize;
        let height = dim.height as usize;
        let pixels = buffer.pixels_mut();
        let mut x = x0;
        let mut y = y0;
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            if Self::iso_paint_stamp_pixel_visible(surface_buffer, stamp_depth, owner_geo_id, x, y)
            {
                Self::iso_paint_blend_lit_stamp_pixel(pixels, width, height, x, y, color);
            }
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    fn iso_paint_world_depth(point: [f32; 3], camera: scenevm::Camera3D) -> Option<f32> {
        let point = Vec3::new(point[0], point[1], point[2]);
        let depth = (point - camera.pos).dot(camera.forward);
        (depth.is_finite() && depth > camera.near && depth < camera.far).then_some(depth)
    }

    fn iso_paint_stamp_lit_color(
        pixels: &[u8],
        width: usize,
        height: usize,
        x: i32,
        y: i32,
        color: [u8; 4],
    ) -> [u8; 4] {
        if x < 0 || y < 0 || x as usize >= width || y as usize >= height || color[3] == 0 {
            return color;
        }
        let sample_luma = |sx: i32, sy: i32| -> f32 {
            if sx < 0 || sy < 0 || sx as usize >= width || sy as usize >= height {
                return 0.0;
            }
            let index = (sy as usize * width + sx as usize) * 4;
            if index + 2 >= pixels.len() {
                return 0.0;
            }
            (pixels[index] as f32 * 0.2126
                + pixels[index + 1] as f32 * 0.7152
                + pixels[index + 2] as f32 * 0.0722)
                / 255.0
        };

        let local_offsets = [
            (0, 0),
            (-24, 0),
            (24, 0),
            (0, -18),
            (0, 18),
            (-36, -24),
            (36, -24),
            (-36, 24),
            (36, 24),
        ];
        let local_luma = local_offsets
            .iter()
            .map(|(ox, oy)| sample_luma(x + ox, y + oy))
            .fold(0.0_f32, f32::max);

        let width_i = width as i32;
        let height_i = height as i32;
        let broad_points = [
            (width_i / 2, height_i / 2),
            (width_i / 4, height_i / 4),
            (width_i * 3 / 4, height_i / 4),
            (width_i / 4, height_i * 3 / 4),
            (width_i * 3 / 4, height_i * 3 / 4),
            (width_i / 2, height_i / 4),
            (width_i / 2, height_i * 3 / 4),
            (width_i / 4, height_i / 2),
            (width_i * 3 / 4, height_i / 2),
        ];
        let broad_luma = broad_points
            .iter()
            .map(|(sx, sy)| sample_luma(*sx, *sy))
            .sum::<f32>()
            / broad_points.len() as f32;

        let global_light = (0.30 + broad_luma * 1.35).clamp(0.34, 1.08);
        let local_light = (0.30 + local_luma * 1.35).clamp(0.34, 1.08);
        let mut light = if local_light < global_light {
            let ratio = (local_light / global_light.max(0.001)).clamp(0.0, 1.0);
            global_light * (0.86 + ratio * 0.14)
        } else {
            (global_light * 0.75 + local_light * 0.25).min(1.08)
        };

        if color[0] > 220 && color[1] > 120 && color[2] < 130 {
            light = light.max(0.72);
        } else if color[0] > 220 && color[1] > 210 && color[2] > 90 {
            light = light.max(0.82);
        }

        [
            (color[0] as f32 * light).round().clamp(0.0, 255.0) as u8,
            (color[1] as f32 * light).round().clamp(0.0, 255.0) as u8,
            (color[2] as f32 * light).round().clamp(0.0, 255.0) as u8,
            color[3],
        ]
    }

    fn iso_paint_blend_lit_stamp_pixel(
        pixels: &mut [u8],
        width: usize,
        height: usize,
        x: i32,
        y: i32,
        color: [u8; 4],
    ) {
        let color = Self::iso_paint_stamp_lit_color(pixels, width, height, x, y, color);
        Self::iso_paint_blend_pixel(pixels, width, height, x, y, color);
    }

    fn iso_paint_stamp_pixel_visible(
        surface_buffer: Option<&scenevm::PaintSurfaceBuffer>,
        stamp_depth: Option<f32>,
        owner_geo_id: Option<scenevm::GeoId>,
        x: i32,
        y: i32,
    ) -> bool {
        let Some(surface_pixel) = surface_buffer.and_then(|surface| surface.pixel(x, y)) else {
            return true;
        };
        if !surface_pixel.valid {
            return true;
        }
        if let Some(stamp_depth) = stamp_depth {
            return surface_pixel.depth + 0.12 >= stamp_depth;
        };
        owner_geo_id.is_none_or(|owner_geo_id| {
            Self::iso_paint_geo_object_matches(owner_geo_id, surface_pixel.geo_id)
        })
    }

    fn iso_paint_adjust_rgb(color: [u8; 4], amount: f32) -> [u8; 4] {
        [
            (color[0] as f32 * amount).round().clamp(0.0, 255.0) as u8,
            (color[1] as f32 * amount).round().clamp(0.0, 255.0) as u8,
            (color[2] as f32 * amount).round().clamp(0.0, 255.0) as u8,
            color[3],
        ]
    }

    fn iso_paint_blend_stamp_pixel(
        buffer: &mut TheRGBABuffer,
        surface_buffer: Option<&scenevm::PaintSurfaceBuffer>,
        stamp_depth: Option<f32>,
        owner_geo_id: Option<scenevm::GeoId>,
        x: i32,
        y: i32,
        color: [u8; 4],
    ) {
        if !Self::iso_paint_stamp_pixel_visible(surface_buffer, stamp_depth, owner_geo_id, x, y) {
            return;
        }
        let dim = *buffer.dim();
        let width = dim.width.max(0) as usize;
        let height = dim.height.max(0) as usize;
        Self::iso_paint_blend_lit_stamp_pixel(buffer.pixels_mut(), width, height, x, y, color);
    }

    fn draw_iso_paint_rotated_ellipse(
        buffer: &mut TheRGBABuffer,
        surface_buffer: Option<&scenevm::PaintSurfaceBuffer>,
        stamp_depth: Option<f32>,
        owner_geo_id: Option<scenevm::GeoId>,
        center: [i32; 2],
        radius_major: f32,
        radius_minor: f32,
        angle: f32,
        color: [u8; 4],
        variation: u32,
    ) {
        let radius_major = radius_major.max(1.0);
        let radius_minor = radius_minor.max(1.0);
        let cos = angle.cos();
        let sin = angle.sin();
        let bound = (radius_major.max(radius_minor) + 1.0).ceil() as i32;
        for y in -bound..=bound {
            for x in -bound..=bound {
                let lx = x as f32 * cos + y as f32 * sin;
                let ly = -x as f32 * sin + y as f32 * cos;
                let edge = lx * lx / (radius_major * radius_major)
                    + ly * ly / (radius_minor * radius_minor);
                if edge > 1.0 {
                    continue;
                }
                let hash = iso_paint_brush::hash_u32(center[0] + x, center[1] + y, variation);
                let noise = (hash & 0xff) as f32 / 255.0;
                let shade = if ly < -radius_minor * 0.35 {
                    1.08 + noise * 0.14
                } else if edge > 0.78 || ly > radius_minor * 0.45 {
                    0.62 + noise * 0.18
                } else {
                    0.82 + noise * 0.20
                };
                let mut pixel = Self::iso_paint_adjust_rgb(color, shade);
                if edge > 0.9 {
                    pixel[3] = ((pixel[3] as f32) * 0.65).round() as u8;
                }
                Self::iso_paint_blend_stamp_pixel(
                    buffer,
                    surface_buffer,
                    stamp_depth,
                    owner_geo_id,
                    center[0] + x,
                    center[1] + y,
                    pixel,
                );
            }
        }
    }

    fn draw_iso_paint_leaves_stamp(
        buffer: &mut TheRGBABuffer,
        surface_buffer: Option<&scenevm::PaintSurfaceBuffer>,
        screen: [i32; 2],
        stamp_depth: Option<f32>,
        owner_geo_id: Option<scenevm::GeoId>,
        size: f32,
        color: [u8; 4],
        opacity: f32,
        variation: u32,
        rotation: f32,
    ) {
        let opacity = opacity.clamp(0.0, 1.0);
        let leaf_count = 5 + (variation % 6) as i32;
        let spread = (size * 10.0).round().clamp(5.0, 38.0) as i32;
        let shadow = [12, 10, 7, (opacity * 42.0).round() as u8];
        for i in 0..leaf_count {
            let seed = variation
                .wrapping_add((i as u32).wrapping_mul(0x27d4_eb2d))
                .rotate_left(((i * 5) as u32) & 15);
            let ox = ((seed & 0xff) as i32 - 128) * spread / 190;
            let oy = (((seed >> 8) & 0xff) as i32 - 128) * spread / 300;
            let center = [screen[0] + ox, screen[1] + oy];
            let angle = rotation + (((seed >> 16) & 0xff) as f32 / 255.0 - 0.5) * 0.85;
            let major = size * (2.2 + ((seed >> 24) & 0x7f) as f32 / 75.0);
            let minor = major * (0.34 + ((seed >> 11) & 0x3f) as f32 / 260.0);
            let shade = 0.68 + ((seed >> 5) & 0xff) as f32 / 255.0 * 0.78;
            let mut leaf = Self::iso_paint_adjust_rgb(color, shade);
            leaf[3] = (opacity * 215.0).round() as u8;
            Self::draw_iso_paint_rotated_ellipse(
                buffer,
                surface_buffer,
                stamp_depth,
                owner_geo_id,
                [center[0] + 1, center[1] + 1],
                major,
                minor,
                angle,
                shadow,
                seed ^ 0x51ad_0001,
            );
            Self::draw_iso_paint_rotated_ellipse(
                buffer,
                surface_buffer,
                stamp_depth,
                owner_geo_id,
                center,
                major,
                minor,
                angle,
                leaf,
                seed,
            );
            let vein_alpha = (opacity * 92.0).round() as u8;
            let vein = Self::iso_paint_adjust_rgb(leaf, 0.42);
            let vein = [vein[0], vein[1], vein[2], vein_alpha];
            let vx = (angle.cos() * major * 0.65).round() as i32;
            let vy = (angle.sin() * major * 0.65).round() as i32;
            Self::iso_paint_blend_line(
                buffer,
                surface_buffer,
                stamp_depth,
                owner_geo_id,
                center[0] - vx,
                center[1] - vy,
                center[0] + vx,
                center[1] + vy,
                vein,
            );
        }
    }

    fn iso_paint_stamp_palette_color(
        palette: &[[u8; 4]],
        index: usize,
        fallback: [u8; 4],
        opacity: f32,
        alpha: f32,
    ) -> [u8; 4] {
        let mut color = palette.get(index).copied().unwrap_or(fallback);
        color[3] = (opacity.clamp(0.0, 1.0) * alpha).round().clamp(0.0, 255.0) as u8;
        color
    }

    fn iso_paint_stamp_wood_color(
        palette: &[[u8; 4]],
        index: usize,
        fallback: [u8; 4],
        opacity: f32,
        alpha: f32,
    ) -> [u8; 4] {
        let palette_color = palette.get(index).copied().filter(|color| {
            color[0] >= color[1].saturating_add(10)
                && color[1] >= color[2].saturating_add(4)
                && color[0] >= 54
        });
        let mut color = palette_color.unwrap_or(fallback);
        color[3] = (opacity.clamp(0.0, 1.0) * alpha).round().clamp(0.0, 255.0) as u8;
        color
    }

    fn draw_iso_paint_flowers_stamp(
        buffer: &mut TheRGBABuffer,
        surface_buffer: Option<&scenevm::PaintSurfaceBuffer>,
        screen: [i32; 2],
        stamp_depth: Option<f32>,
        owner_geo_id: Option<scenevm::GeoId>,
        size: f32,
        color: [u8; 4],
        palette: &[[u8; 4]],
        opacity: f32,
        variant: &str,
        variation: u32,
        rotation: f32,
    ) {
        let opacity = opacity.clamp(0.0, 1.0);
        let flower_count = match variant {
            "bluebells" => 3 + (variation % 3) as i32,
            "poppies" => 3 + (variation % 4) as i32,
            _ => 4 + (variation % 5) as i32,
        };
        let spread = (size * 8.0).round().clamp(5.0, 32.0) as i32;
        let stem_source = palette.first().copied().unwrap_or(color);
        let mut stem = Self::iso_paint_adjust_rgb(stem_source, 0.82);
        stem[3] = (opacity * 220.0).round() as u8;
        let mut leaf = Self::iso_paint_adjust_rgb(stem_source, 1.12);
        leaf[3] = (opacity * 165.0).round() as u8;
        let mut shadow = Self::iso_paint_adjust_rgb(stem_source, 0.18);
        shadow[3] = (opacity * 48.0).round() as u8;

        for i in 0..flower_count {
            let seed = variation
                .wrapping_add((i as u32).wrapping_mul(0x7feb_352d))
                .rotate_left(((i * 6) as u32) & 15);
            let ox = ((seed & 0xff) as i32 - 128) * spread / 190;
            let oy = (((seed >> 8) & 0xff) as i32 - 128) * spread / 420;
            let base = [screen[0] + ox, screen[1] + oy];
            let height = (size * (5.6 + ((seed >> 16) & 0x7f) as f32 / 32.0))
                .round()
                .clamp(5.0, 24.0) as i32;
            let lean = ((seed >> 24) as i32 & 0xff) - 128;
            let lean = lean * spread / 520 + (rotation.sin() * spread as f32 * 0.2).round() as i32;
            let tip = [base[0] + lean, base[1] - height];

            Self::iso_paint_blend_line(
                buffer,
                surface_buffer,
                stamp_depth,
                owner_geo_id,
                base[0] + 1,
                base[1] + 1,
                tip[0] + 1,
                tip[1] + 1,
                shadow,
            );
            Self::iso_paint_blend_line(
                buffer,
                surface_buffer,
                stamp_depth,
                owner_geo_id,
                base[0],
                base[1],
                tip[0],
                tip[1],
                stem,
            );

            if i % 2 == 0 {
                let leaf_center = [
                    base[0] + lean / 2 + if seed & 1 == 0 { -1 } else { 1 },
                    base[1] - height / 2,
                ];
                Self::draw_iso_paint_rotated_ellipse(
                    buffer,
                    surface_buffer,
                    stamp_depth,
                    owner_geo_id,
                    leaf_center,
                    (size * 1.5).clamp(1.2, 5.0),
                    (size * 0.55).clamp(0.9, 2.4),
                    rotation + if seed & 1 == 0 { -0.45 } else { 0.45 },
                    leaf,
                    seed ^ 0x3311_aa01,
                );
            }

            let petal_slot = if variant == "wildflowers" {
                1 + (((seed >> 13) as usize) % 3)
            } else {
                1
            };
            let petal_fallback = Self::iso_paint_adjust_rgb(stem_source, 1.25);
            let petal = Self::iso_paint_stamp_palette_color(
                palette,
                petal_slot,
                petal_fallback,
                opacity,
                225.0,
            );
            let radius = (size * (0.9 + ((seed >> 5) & 0x3f) as f32 / 120.0)).clamp(1.1, 4.2);
            let petal_count = if variant == "poppies" { 5 } else { 4 };
            for petal_index in 0..petal_count {
                let angle = rotation
                    + petal_index as f32 * std::f32::consts::TAU / petal_count as f32
                    + ((seed >> 9) & 0x1f) as f32 / 255.0;
                let center = if variant == "bluebells" {
                    [
                        tip[0] + ((petal_index as f32 - 1.5) * radius * 0.45).round() as i32,
                        tip[1] + (radius * (petal_index as f32 * 0.9 + 0.6)).round() as i32,
                    ]
                } else {
                    [
                        tip[0] + (angle.cos() * radius * 0.75).round() as i32,
                        tip[1] + (angle.sin() * radius * 0.55).round() as i32,
                    ]
                };
                Self::draw_iso_paint_rotated_ellipse(
                    buffer,
                    surface_buffer,
                    stamp_depth,
                    owner_geo_id,
                    center,
                    radius,
                    if variant == "bluebells" {
                        radius * 0.8
                    } else {
                        radius * 0.62
                    },
                    angle,
                    petal,
                    seed ^ petal_index as u32,
                );
            }
            let center_slot = if variant == "wildflowers" { 2 } else { 3 };
            let center_fallback = Self::iso_paint_adjust_rgb(stem_source, 0.55);
            let center = Self::iso_paint_stamp_palette_color(
                palette,
                center_slot,
                center_fallback,
                opacity,
                if variant == "bluebells" { 150.0 } else { 230.0 },
            );
            Self::draw_iso_paint_rotated_ellipse(
                buffer,
                surface_buffer,
                stamp_depth,
                owner_geo_id,
                tip,
                (radius * 0.55).max(1.0),
                (radius * 0.45).max(1.0),
                rotation,
                center,
                seed ^ 0x7777_0013,
            );
        }
    }

    fn draw_iso_paint_vines_stamp(
        buffer: &mut TheRGBABuffer,
        surface_buffer: Option<&scenevm::PaintSurfaceBuffer>,
        screen: [i32; 2],
        stamp_depth: Option<f32>,
        owner_geo_id: Option<scenevm::GeoId>,
        size: f32,
        color: [u8; 4],
        palette: &[[u8; 4]],
        opacity: f32,
        variation: u32,
        rotation: f32,
    ) {
        let opacity = opacity.clamp(0.0, 1.0);
        let mut stem = Self::iso_paint_stamp_palette_color(
            palette,
            0,
            Self::iso_paint_adjust_rgb(color, 0.74),
            opacity,
            225.0,
        );
        stem = Self::iso_paint_adjust_rgb(stem, 0.86);
        let leaf_a = Self::iso_paint_stamp_palette_color(
            palette,
            1,
            Self::iso_paint_adjust_rgb(color, 1.12),
            opacity,
            205.0,
        );
        let leaf_b = Self::iso_paint_stamp_palette_color(
            palette,
            2,
            Self::iso_paint_adjust_rgb(color, 0.92),
            opacity,
            190.0,
        );
        let mut shadow = Self::iso_paint_adjust_rgb(stem, 0.22);
        shadow[3] = (opacity * 54.0).round() as u8;

        let vine_count = 2 + (variation % 3) as i32;
        let spread = (size * 7.0).round().clamp(3.0, 28.0) as i32;
        for i in 0..vine_count {
            let seed = variation
                .wrapping_add((i as u32).wrapping_mul(0x632b_e5ab))
                .rotate_left(((i * 5) as u32) & 15);
            let base = [
                screen[0] + ((seed & 0xff) as i32 - 128) * spread / 220,
                screen[1] + (((seed >> 8) & 0xff) as i32 - 128) * spread / 300,
            ];
            let length = (size * (14.0 + ((seed >> 16) & 0x7f) as f32 / 4.6))
                .round()
                .clamp(10.0, 58.0);
            let angle = rotation - std::f32::consts::FRAC_PI_2
                + ((seed >> 24) & 0xff) as f32 / 255.0 * 1.35
                - 0.68;
            let dir = [angle.cos(), angle.sin()];
            let normal = [-dir[1], dir[0]];
            let sway = (size * (3.0 + ((seed >> 10) & 0x3f) as f32 / 22.0)).clamp(2.0, 13.0);
            let phase = ((seed >> 4) & 0xff) as f32 / 255.0 * std::f32::consts::TAU;
            let segments = 5 + ((seed >> 7) & 3) as i32;
            let mut prev = base;
            for step in 1..=segments {
                let t = step as f32 / segments as f32;
                let wave = (phase + t * std::f32::consts::TAU * 0.72).sin() * sway;
                let point = [
                    base[0] + (dir[0] * length * t + normal[0] * wave).round() as i32,
                    base[1] + (dir[1] * length * t + normal[1] * wave).round() as i32,
                ];
                Self::iso_paint_blend_line(
                    buffer,
                    surface_buffer,
                    stamp_depth,
                    owner_geo_id,
                    prev[0] + 1,
                    prev[1] + 1,
                    point[0] + 1,
                    point[1] + 1,
                    shadow,
                );
                Self::iso_paint_blend_line(
                    buffer,
                    surface_buffer,
                    stamp_depth,
                    owner_geo_id,
                    prev[0],
                    prev[1],
                    point[0],
                    point[1],
                    stem,
                );

                if step % 2 == 0 || step == segments {
                    let side = if (seed >> (step as u32)) & 1 == 0 {
                        -1.0
                    } else {
                        1.0
                    };
                    let leaf_center = [
                        point[0] + (normal[0] * side * size * 2.2).round() as i32,
                        point[1] + (normal[1] * side * size * 2.2).round() as i32,
                    ];
                    let leaf_seed = seed ^ (step as u32).wrapping_mul(0x45d9_f3b);
                    Self::draw_iso_paint_rotated_ellipse(
                        buffer,
                        surface_buffer,
                        stamp_depth,
                        owner_geo_id,
                        leaf_center,
                        (size * 2.4).clamp(1.5, 8.0),
                        (size * 0.85).clamp(0.8, 3.2),
                        angle + side * 0.78,
                        if step % 3 == 0 { leaf_b } else { leaf_a },
                        leaf_seed,
                    );
                }
                prev = point;
            }
        }
    }

    fn draw_iso_paint_roots_stamp(
        buffer: &mut TheRGBABuffer,
        surface_buffer: Option<&scenevm::PaintSurfaceBuffer>,
        screen: [i32; 2],
        stamp_depth: Option<f32>,
        owner_geo_id: Option<scenevm::GeoId>,
        size: f32,
        color: [u8; 4],
        palette: &[[u8; 4]],
        opacity: f32,
        variation: u32,
        rotation: f32,
    ) {
        let opacity = opacity.clamp(0.0, 1.0);
        let mut root = Self::iso_paint_stamp_palette_color(
            palette,
            0,
            Self::iso_paint_adjust_rgb(color, 0.82),
            opacity,
            230.0,
        );
        root = Self::iso_paint_adjust_rgb(root, 0.92);
        let dark = Self::iso_paint_stamp_palette_color(
            palette,
            1,
            Self::iso_paint_adjust_rgb(color, 0.48),
            opacity,
            180.0,
        );
        let highlight = Self::iso_paint_stamp_palette_color(
            palette,
            2,
            Self::iso_paint_adjust_rgb(color, 1.18),
            opacity,
            145.0,
        );
        let branch_count = 3 + (variation % 4) as i32;
        let spread = (size * 9.0).round().clamp(5.0, 34.0) as i32;
        let base_angle = rotation + std::f32::consts::FRAC_PI_2;

        for i in 0..branch_count {
            let seed = variation
                .wrapping_add((i as u32).wrapping_mul(0x94d0_49bb))
                .rotate_left(((i * 4) as u32) & 15);
            let side = if i % 2 == 0 { -1.0 } else { 1.0 };
            let angle = base_angle + side * (0.35 + ((seed >> 8) & 0xff) as f32 / 255.0 * 0.7);
            let dir = [angle.cos(), angle.sin()];
            let normal = [-dir[1], dir[0]];
            let start = [
                screen[0] + (((seed >> 16) & 0xff) as i32 - 128) * spread / 300,
                screen[1] + (((seed >> 24) & 0xff) as i32 - 128) * spread / 420,
            ];
            let length = (size * (11.0 + ((seed >> 4) & 0x7f) as f32 / 5.0))
                .round()
                .clamp(9.0, 42.0);
            let bend = (size * (2.0 + ((seed >> 11) & 0x3f) as f32 / 24.0)).clamp(1.5, 10.0);
            let segments = 4 + ((seed >> 19) & 3) as i32;
            let mut prev = start;
            for step in 1..=segments {
                let t = step as f32 / segments as f32;
                let wave = (t * std::f32::consts::PI).sin() * bend * side;
                let point = [
                    start[0] + (dir[0] * length * t + normal[0] * wave).round() as i32,
                    start[1] + (dir[1] * length * t + normal[1] * wave).round() as i32,
                ];
                Self::iso_paint_blend_line(
                    buffer,
                    surface_buffer,
                    stamp_depth,
                    owner_geo_id,
                    prev[0] + 1,
                    prev[1] + 1,
                    point[0] + 1,
                    point[1] + 1,
                    dark,
                );
                Self::iso_paint_blend_line(
                    buffer,
                    surface_buffer,
                    stamp_depth,
                    owner_geo_id,
                    prev[0],
                    prev[1],
                    point[0],
                    point[1],
                    root,
                );
                if size > 1.35 && step < segments {
                    Self::iso_paint_blend_line(
                        buffer,
                        surface_buffer,
                        stamp_depth,
                        owner_geo_id,
                        prev[0],
                        prev[1] - 1,
                        point[0],
                        point[1] - 1,
                        root,
                    );
                }

                if step == 2 || (step == segments - 1 && seed & 1 == 0) {
                    let twig_angle = angle - side * 0.7;
                    let twig_len = (size * (4.5 + ((seed >> 13) & 0x3f) as f32 / 16.0))
                        .round()
                        .clamp(3.0, 16.0);
                    let twig_end = [
                        point[0] + (twig_angle.cos() * twig_len).round() as i32,
                        point[1] + (twig_angle.sin() * twig_len).round() as i32,
                    ];
                    Self::iso_paint_blend_line(
                        buffer,
                        surface_buffer,
                        stamp_depth,
                        owner_geo_id,
                        point[0],
                        point[1],
                        twig_end[0],
                        twig_end[1],
                        dark,
                    );
                }
                prev = point;
            }

            if i % 2 == 0 {
                Self::draw_iso_paint_rotated_ellipse(
                    buffer,
                    surface_buffer,
                    stamp_depth,
                    owner_geo_id,
                    start,
                    (size * 1.35).clamp(1.0, 5.0),
                    (size * 0.8).clamp(0.8, 3.0),
                    angle,
                    highlight,
                    seed ^ 0x7015_0001,
                );
            }
        }
    }

    fn draw_iso_paint_leaf_mass(
        buffer: &mut TheRGBABuffer,
        surface_buffer: Option<&scenevm::PaintSurfaceBuffer>,
        stamp_depth: Option<f32>,
        owner_geo_id: Option<scenevm::GeoId>,
        center: [i32; 2],
        radius_x: f32,
        radius_y: f32,
        seed: u32,
        dark: [u8; 4],
        mid: [u8; 4],
        light: [u8; 4],
    ) {
        let radius_x = radius_x.max(2.0);
        let radius_y = radius_y.max(2.0);
        let bound_x = (radius_x + 2.0).ceil() as i32;
        let bound_y = (radius_y + 2.0).ceil() as i32;
        for y in -bound_y..=bound_y {
            for x in -bound_x..=bound_x {
                let nx = x as f32 / radius_x;
                let ny = y as f32 / radius_y;
                let edge = nx * nx + ny * ny;
                let hash = iso_paint_brush::hash_u32(center[0] + x, center[1] + y, seed);
                let noise = (hash & 0xff) as f32 / 255.0;
                let wobble = (((hash >> 8) & 0xff) as f32 / 255.0 - 0.5) * 0.34;
                if edge > 0.94 + wobble {
                    continue;
                }
                if edge > 0.62 && ((hash >> 17) & 7) == 0 {
                    continue;
                }
                if edge < 0.36 && ((hash >> 21) & 31) == 0 {
                    continue;
                }

                let mut pixel = if ny < -0.34 && noise > 0.34 {
                    light
                } else if ny > 0.28 || edge > 0.72 {
                    dark
                } else {
                    mid
                };
                let shade = 0.76 + noise * 0.42 + (-ny).max(0.0) * 0.16;
                pixel = Self::iso_paint_adjust_rgb(pixel, shade);
                if edge > 0.76 {
                    pixel[3] = ((pixel[3] as f32) * (0.58 + noise * 0.32)).round() as u8;
                }
                Self::iso_paint_blend_stamp_pixel(
                    buffer,
                    surface_buffer,
                    stamp_depth,
                    owner_geo_id,
                    center[0] + x,
                    center[1] + y,
                    pixel,
                );

                if radius_x > 10.0 && noise > 0.91 && edge < 0.68 {
                    let mut fleck = light;
                    fleck[3] = ((fleck[3] as f32) * 0.72).round() as u8;
                    Self::iso_paint_blend_stamp_pixel(
                        buffer,
                        surface_buffer,
                        stamp_depth,
                        owner_geo_id,
                        center[0] + x + 1,
                        center[1] + y,
                        fleck,
                    );
                }
            }
        }
    }

    fn draw_iso_paint_bushes_stamp(
        buffer: &mut TheRGBABuffer,
        surface_buffer: Option<&scenevm::PaintSurfaceBuffer>,
        screen: [i32; 2],
        stamp_depth: Option<f32>,
        owner_geo_id: Option<scenevm::GeoId>,
        size: f32,
        color: [u8; 4],
        palette: &[[u8; 4]],
        opacity: f32,
        variation: u32,
        rotation: f32,
    ) {
        let opacity = opacity.clamp(0.0, 1.0);
        let dark = Self::iso_paint_stamp_palette_color(
            palette,
            0,
            Self::iso_paint_adjust_rgb(color, 0.68),
            opacity,
            220.0,
        );
        let mid = Self::iso_paint_stamp_palette_color(
            palette,
            1,
            Self::iso_paint_adjust_rgb(color, 0.98),
            opacity,
            230.0,
        );
        let light = Self::iso_paint_stamp_palette_color(
            palette,
            2,
            Self::iso_paint_adjust_rgb(color, 1.22),
            opacity,
            175.0,
        );
        let branch =
            Self::iso_paint_stamp_wood_color(palette, 3, [74, 49, 28, 255], opacity, 210.0);
        let bark_dark = Self::iso_paint_adjust_rgb(branch, 0.48);
        let root_y = screen[1];
        let art_size = (size * 0.58).clamp(1.0, 5.0);
        let stem_count = 2 + (variation % 2) as i32;
        let spread_x = (art_size * 3.0).round().clamp(3.0, 12.0) as i32;
        let stem_height = (art_size * 6.6).round().clamp(7.0, 30.0) as i32;

        for i in 0..stem_count {
            let seed = variation ^ (i as u32 + 1).wrapping_mul(0x45d9_f3b);
            let lane = i - stem_count / 2;
            let base_x = screen[0] + lane * spread_x / stem_count.max(1);
            let lean = (((seed >> 8) & 0xff) as i32 - 128) * spread_x / 360
                + (rotation.sin() * art_size * 1.2).round() as i32;
            let top = [
                base_x + lean,
                root_y - stem_height + (((seed >> 16) & 0x1f) as i32 * stem_height / 170),
            ];
            Self::iso_paint_blend_line(
                buffer,
                surface_buffer,
                stamp_depth,
                owner_geo_id,
                base_x + 1,
                root_y,
                top[0] + 1,
                top[1],
                bark_dark,
            );
            Self::iso_paint_blend_line(
                buffer,
                surface_buffer,
                stamp_depth,
                owner_geo_id,
                base_x,
                root_y,
                top[0],
                top[1],
                branch,
            );

            for node in 0..2 {
                let t = 0.42 + node as f32 * 0.25;
                let center = [
                    (base_x as f32 + (top[0] - base_x) as f32 * t).round() as i32,
                    (root_y as f32 + (top[1] - root_y) as f32 * t).round() as i32,
                ];
                let side = if (seed >> (node as u32)) & 1 == 0 {
                    -1.0
                } else {
                    1.0
                };
                let leaf_center = [
                    center[0] + (side * art_size * (0.95 + node as f32 * 0.42)).round() as i32,
                    center[1] - (art_size * 0.35).round() as i32,
                ];
                let leaf_seed = seed ^ (node as u32).wrapping_mul(0x9e37_79b9);
                Self::draw_iso_paint_leaf_mass(
                    buffer,
                    surface_buffer,
                    stamp_depth,
                    owner_geo_id,
                    leaf_center,
                    (art_size * (0.78 + node as f32 * 0.12)).clamp(1.6, 4.0),
                    (art_size * (0.98 + node as f32 * 0.12)).clamp(2.0, 5.4),
                    leaf_seed,
                    dark,
                    mid,
                    light,
                );
            }

            let tip_leaf = if i % 2 == 0 { light } else { mid };
            Self::draw_iso_paint_rotated_ellipse(
                buffer,
                surface_buffer,
                stamp_depth,
                owner_geo_id,
                top,
                (art_size * 0.72).clamp(1.3, 3.7),
                (art_size * 1.05).clamp(1.8, 5.2),
                rotation + lean as f32 * 0.03,
                tip_leaf,
                seed ^ 0xb055_0001,
            );

            Self::iso_paint_blend_line(
                buffer,
                surface_buffer,
                stamp_depth,
                owner_geo_id,
                base_x + 1,
                root_y,
                top[0] + 1,
                top[1],
                bark_dark,
            );
            Self::iso_paint_blend_line(
                buffer,
                surface_buffer,
                stamp_depth,
                owner_geo_id,
                base_x,
                root_y,
                top[0],
                top[1],
                branch,
            );
        }

        let base = Self::iso_paint_adjust_rgb(branch, 0.62);
        for dx in -spread_x / 2..=spread_x / 2 {
            if dx.abs() <= spread_x / 3 {
                Self::iso_paint_blend_stamp_pixel(
                    buffer,
                    surface_buffer,
                    stamp_depth,
                    owner_geo_id,
                    screen[0] + dx,
                    root_y,
                    base,
                );
            }
        }
    }

    fn draw_iso_paint_tree_stamp(
        buffer: &mut TheRGBABuffer,
        surface_buffer: Option<&scenevm::PaintSurfaceBuffer>,
        screen: [i32; 2],
        stamp_depth: Option<f32>,
        owner_geo_id: Option<scenevm::GeoId>,
        size: f32,
        color: [u8; 4],
        palette: &[[u8; 4]],
        opacity: f32,
        variation: u32,
        rotation: f32,
    ) {
        let opacity = opacity.clamp(0.0, 1.0);
        let leaf_dark = Self::iso_paint_stamp_palette_color(
            palette,
            0,
            Self::iso_paint_adjust_rgb(color, 0.7),
            opacity,
            230.0,
        );
        let leaf_mid = Self::iso_paint_stamp_palette_color(
            palette,
            1,
            Self::iso_paint_adjust_rgb(color, 1.02),
            opacity,
            235.0,
        );
        let trunk =
            Self::iso_paint_stamp_palette_color(palette, 2, [92, 58, 36, 255], opacity, 225.0);
        let leaf_light = Self::iso_paint_stamp_palette_color(
            palette,
            3,
            Self::iso_paint_adjust_rgb(color, 1.25),
            opacity,
            185.0,
        );
        let base = [screen[0], screen[1] + (size * 2.4).round() as i32];
        let canopy_center = [screen[0], screen[1] - (size * 12.5).round() as i32];
        let shadow = [6, 7, 5, (opacity * 68.0).round() as u8];
        Self::draw_iso_paint_rotated_ellipse(
            buffer,
            surface_buffer,
            stamp_depth,
            owner_geo_id,
            base,
            (size * 4.5).clamp(3.0, 18.0),
            (size * 1.8).clamp(1.4, 8.0),
            rotation * 0.2,
            shadow,
            variation ^ 0x7aee_0001,
        );

        let trunk_height = (size * 14.0).round().clamp(10.0, 46.0) as i32;
        let trunk_width = (size * 1.5).round().clamp(2.0, 7.0) as i32;
        for dx in -trunk_width..=trunk_width {
            let shade = if dx < 0 { 1.12 } else { 0.64 };
            let trunk_pixel = Self::iso_paint_adjust_rgb(trunk, shade);
            Self::iso_paint_blend_line(
                buffer,
                surface_buffer,
                stamp_depth,
                owner_geo_id,
                base[0] + dx,
                base[1],
                base[0] + dx / 2,
                base[1] - trunk_height,
                trunk_pixel,
            );
        }

        let bark_dark = Self::iso_paint_adjust_rgb(trunk, 0.42);
        for stripe in [-1, 1] {
            Self::iso_paint_blend_line(
                buffer,
                surface_buffer,
                stamp_depth,
                owner_geo_id,
                base[0] + stripe,
                base[1] - 1,
                base[0] + stripe / 2,
                base[1] - trunk_height + 2,
                bark_dark,
            );
        }

        for side in [-1.0_f32, 1.0] {
            let start = [base[0], base[1] - trunk_height * 2 / 3];
            let end = [
                start[0] + (side * size * 7.2).round() as i32,
                start[1] - (size * 5.3).round() as i32,
            ];
            Self::iso_paint_blend_line(
                buffer,
                surface_buffer,
                stamp_depth,
                owner_geo_id,
                start[0],
                start[1],
                end[0],
                end[1],
                bark_dark,
            );
            Self::iso_paint_blend_line(
                buffer,
                surface_buffer,
                stamp_depth,
                owner_geo_id,
                start[0],
                start[1] - 1,
                end[0],
                end[1] - 1,
                trunk,
            );
        }

        let crown = [
            (-8.5_f32, -7.0_f32, 7.4_f32, 7.2_f32),
            (6.5, -9.5, 8.2, 7.8),
            (-15.0, -1.8, 7.0, 6.8),
            (14.5, -1.2, 7.0, 6.6),
            (-5.0, 3.5, 9.2, 7.2),
            (6.5, 4.5, 8.4, 6.5),
            (0.0, -16.0, 7.4, 7.4),
            (-1.5, -3.6, 10.8, 8.2),
        ];
        for (i, (ox, oy, rx, ry)) in crown.iter().enumerate() {
            let seed = variation
                .wrapping_add((i as u32).wrapping_mul(0x85eb_ca6b))
                .rotate_left(((i * 6) as u32) & 15);
            let jitter_x = ((seed & 0xff) as f32 / 255.0 - 0.5) * size * 2.4;
            let jitter_y = (((seed >> 8) & 0xff) as f32 / 255.0 - 0.5) * size * 2.0;
            let center = [
                canopy_center[0] + (ox * size + jitter_x).round() as i32,
                canopy_center[1] + (oy * size + jitter_y).round() as i32,
            ];
            Self::draw_iso_paint_leaf_mass(
                buffer,
                surface_buffer,
                stamp_depth,
                owner_geo_id,
                center,
                (rx * size).clamp(4.0, 28.0),
                (ry * size).clamp(3.0, 24.0),
                seed,
                leaf_dark,
                leaf_mid,
                leaf_light,
            );
        }

        for i in 0..18 {
            let seed = variation ^ (i as u32).wrapping_mul(0x27d4_eb2d);
            if (seed & 3) == 0 {
                continue;
            }
            let x = canopy_center[0] + (((seed >> 8) & 0xff) as i32 - 128) * (size as i32 + 10) / 9;
            let y =
                canopy_center[1] + (((seed >> 16) & 0xff) as i32 - 128) * (size as i32 + 8) / 11;
            let fleck = if (seed & 8) == 0 {
                leaf_light
            } else {
                leaf_dark
            };
            Self::iso_paint_blend_stamp_pixel(
                buffer,
                surface_buffer,
                stamp_depth,
                owner_geo_id,
                x,
                y,
                fleck,
            );
        }
    }

    fn draw_iso_paint_candles_stamp(
        buffer: &mut TheRGBABuffer,
        surface_buffer: Option<&scenevm::PaintSurfaceBuffer>,
        screen: [i32; 2],
        stamp_depth: Option<f32>,
        owner_geo_id: Option<scenevm::GeoId>,
        size: f32,
        color: [u8; 4],
        palette: &[[u8; 4]],
        opacity: f32,
        variation: u32,
    ) {
        let opacity = opacity.clamp(0.0, 1.0);
        let wax = Self::iso_paint_stamp_palette_color(palette, 0, color, opacity, 235.0);
        let side = Self::iso_paint_stamp_palette_color(
            palette,
            1,
            Self::iso_paint_adjust_rgb(wax, 0.66),
            opacity,
            210.0,
        );
        let flame =
            Self::iso_paint_stamp_palette_color(palette, 2, [255, 151, 45, 230], opacity, 230.0);
        let core =
            Self::iso_paint_stamp_palette_color(palette, 3, [255, 239, 142, 245], opacity, 245.0);
        let candle_count = 1 + (variation % 3) as i32;
        let shadow = [8, 6, 4, (opacity * 62.0).round() as u8];
        let glow = [255, 151, 45, (opacity * 32.0).round() as u8];
        for i in 0..candle_count {
            let seed = variation
                .wrapping_add((i as u32).wrapping_mul(0x85eb_ca6b))
                .rotate_left(((i * 6) as u32) & 15);
            let offset = (i - (candle_count - 1) / 2) as f32;
            let jitter = ((seed & 0xff) as f32 / 255.0 - 0.5) * size * 4.0;
            let base = [
                screen[0] + (offset * size * 6.0 + jitter).round() as i32,
                screen[1] + (((seed >> 8) & 0x3f) as f32 / 63.0 * size * 3.0).round() as i32,
            ];
            let height = (size * (8.0 + ((seed >> 14) & 0x7f) as f32 / 13.0))
                .round()
                .clamp(7.0, 28.0) as i32;
            let half_width = (size * (1.45 + ((seed >> 22) & 0x3f) as f32 / 72.0))
                .round()
                .clamp(1.0, 6.0) as i32;
            let top_y = base[1] - height;
            Self::draw_iso_paint_rotated_ellipse(
                buffer,
                surface_buffer,
                stamp_depth,
                owner_geo_id,
                [base[0], base[1] + 1],
                half_width as f32 + 1.2,
                (size * 1.0).clamp(0.8, 3.2),
                0.0,
                shadow,
                seed ^ 0x1188_0001,
            );
            Self::draw_iso_paint_rotated_ellipse(
                buffer,
                surface_buffer,
                stamp_depth,
                owner_geo_id,
                [base[0], top_y - (size * 5.3).round() as i32],
                (size * 4.4).clamp(2.0, 14.0),
                (size * 5.6).clamp(2.6, 18.0),
                0.0,
                glow,
                seed ^ 0x7f7f_0009,
            );
            for dx in -half_width..=half_width {
                let mut body = if dx > half_width / 2 { side } else { wax };
                let shade = if dx < -half_width / 2 { 1.09 } else { 1.0 };
                body = Self::iso_paint_adjust_rgb(body, shade);
                Self::iso_paint_blend_line(
                    buffer,
                    surface_buffer,
                    stamp_depth,
                    owner_geo_id,
                    base[0] + dx,
                    base[1],
                    base[0] + dx,
                    top_y,
                    body,
                );
            }
            Self::draw_iso_paint_rotated_ellipse(
                buffer,
                surface_buffer,
                stamp_depth,
                owner_geo_id,
                [base[0], base[1]],
                half_width as f32 + 0.6,
                (size * 0.85).clamp(0.7, 2.8),
                0.0,
                side,
                seed ^ 0x5511_0001,
            );
            Self::draw_iso_paint_rotated_ellipse(
                buffer,
                surface_buffer,
                stamp_depth,
                owner_geo_id,
                [base[0], top_y],
                half_width as f32 + 0.4,
                (size * 0.75).clamp(0.7, 2.4),
                0.0,
                wax,
                seed ^ 0x5511_0002,
            );
            let wick = [24, 17, 12, (opacity * 185.0).round() as u8];
            Self::iso_paint_blend_line(
                buffer,
                surface_buffer,
                stamp_depth,
                owner_geo_id,
                base[0],
                top_y,
                base[0],
                top_y - (size * 2.3).round().max(2.0) as i32,
                wick,
            );
            let flame_y = top_y - (size * 4.2).round().max(4.0) as i32;
            Self::draw_iso_paint_rotated_ellipse(
                buffer,
                surface_buffer,
                stamp_depth,
                owner_geo_id,
                [base[0], flame_y],
                (size * 1.7).clamp(1.2, 5.5),
                (size * 3.2).clamp(2.0, 8.8),
                0.0,
                flame,
                seed ^ 0xf17e_0001,
            );
            Self::draw_iso_paint_rotated_ellipse(
                buffer,
                surface_buffer,
                stamp_depth,
                owner_geo_id,
                [base[0], flame_y + (size * 0.55).round() as i32],
                (size * 0.85).clamp(0.8, 3.0),
                (size * 1.55).clamp(1.0, 4.8),
                0.0,
                core,
                seed ^ 0xf17e_0002,
            );
        }
    }

    fn draw_iso_paint_stamp_shape(
        buffer: &mut TheRGBABuffer,
        stamp: &IsoPaintStamp,
        surface_buffer: Option<&scenevm::PaintSurfaceBuffer>,
        screen: [i32; 2],
        stamp_depth: Option<f32>,
        owner_geo_id: Option<scenevm::GeoId>,
        size: f32,
    ) {
        match stamp.kind.as_str() {
            "grass" | "grass_stamp" => Self::draw_iso_paint_grass_stamp(
                buffer,
                surface_buffer,
                screen,
                stamp_depth,
                owner_geo_id,
                size,
                stamp.color,
                stamp.opacity,
                stamp.variation,
                stamp.rotation,
            ),
            "rubble" => Self::draw_iso_paint_rubble_stamp(
                buffer,
                surface_buffer,
                screen,
                stamp_depth,
                owner_geo_id,
                size,
                stamp.color,
                stamp.opacity,
                stamp.variation,
                stamp.rotation,
            ),
            "leaves" => Self::draw_iso_paint_leaves_stamp(
                buffer,
                surface_buffer,
                screen,
                stamp_depth,
                owner_geo_id,
                size,
                stamp.color,
                stamp.opacity,
                stamp.variation,
                stamp.rotation,
            ),
            "flowers" => Self::draw_iso_paint_flowers_stamp(
                buffer,
                surface_buffer,
                screen,
                stamp_depth,
                owner_geo_id,
                size,
                stamp.color,
                &stamp.palette_colors,
                stamp.opacity,
                stamp.variant.as_str(),
                stamp.variation,
                stamp.rotation,
            ),
            "vines" => Self::draw_iso_paint_vines_stamp(
                buffer,
                surface_buffer,
                screen,
                stamp_depth,
                owner_geo_id,
                size,
                stamp.color,
                &stamp.palette_colors,
                stamp.opacity,
                stamp.variation,
                stamp.rotation,
            ),
            "roots" => Self::draw_iso_paint_roots_stamp(
                buffer,
                surface_buffer,
                screen,
                stamp_depth,
                owner_geo_id,
                size,
                stamp.color,
                &stamp.palette_colors,
                stamp.opacity,
                stamp.variation,
                stamp.rotation,
            ),
            "bushes" => Self::draw_iso_paint_bushes_stamp(
                buffer,
                surface_buffer,
                screen,
                stamp_depth,
                owner_geo_id,
                size,
                stamp.color,
                &stamp.palette_colors,
                stamp.opacity,
                stamp.variation,
                stamp.rotation,
            ),
            "tree" => Self::draw_iso_paint_tree_stamp(
                buffer,
                surface_buffer,
                screen,
                stamp_depth,
                owner_geo_id,
                size,
                stamp.color,
                &stamp.palette_colors,
                stamp.opacity,
                stamp.variation,
                stamp.rotation,
            ),
            "candles" => Self::draw_iso_paint_candles_stamp(
                buffer,
                surface_buffer,
                screen,
                stamp_depth,
                owner_geo_id,
                size,
                stamp.color,
                &stamp.palette_colors,
                stamp.opacity,
                stamp.variation,
            ),
            "footprints" => Self::draw_iso_paint_footprints_stamp(
                buffer,
                surface_buffer,
                screen,
                stamp_depth,
                owner_geo_id,
                size,
                stamp.color,
                stamp.opacity,
                stamp.variation,
                stamp.rotation,
            ),
            "mud" => Self::draw_iso_paint_mud_stamp(
                buffer,
                surface_buffer,
                screen,
                stamp_depth,
                owner_geo_id,
                size,
                stamp.color,
                stamp.opacity,
                stamp.variation,
                stamp.rotation,
            ),
            _ => {}
        }
    }

    fn iso_paint_write_stamp_material(
        material_pixels: &mut [u8],
        width: usize,
        height: usize,
        surface_buffer: Option<&scenevm::PaintSurfaceBuffer>,
        stamp: &IsoPaintStamp,
        screen: [i32; 2],
        owner_geo_id: Option<scenevm::GeoId>,
        size: f32,
    ) {
        if stamp.material_id == 0 || width == 0 || height == 0 {
            return;
        }

        let mut mask = TheRGBABuffer::new(TheDim::sized(width as i32, height as i32));
        Self::draw_iso_paint_stamp_shape(
            &mut mask,
            stamp,
            surface_buffer,
            screen,
            None,
            owner_geo_id,
            size,
        );

        for (index, pixel) in mask.pixels().chunks_exact(4).enumerate() {
            let coverage = pixel[3];
            if coverage == 0 {
                continue;
            }
            let x = (index % width) as i32;
            let y = (index / width) as i32;
            Self::iso_paint_set_stamp_material_pixel(
                material_pixels,
                width,
                height,
                surface_buffer,
                owner_geo_id,
                x,
                y,
                stamp.material_id,
                coverage,
            );
        }
    }

    fn iso_paint_stamp_screen_and_size(
        stamp: &IsoPaintStamp,
        target_width: i32,
        target_height: i32,
        current_camera_scale: Option<f32>,
        project_world_anchor: &impl Fn([f32; 3], i32, i32) -> Option<[i32; 2]>,
    ) -> ([i32; 2], f32) {
        let screen = stamp
            .world
            .and_then(|world| project_world_anchor(world, target_width, target_height))
            .unwrap_or(stamp.screen);
        let size = if let (Some(source_scale), Some(current_scale)) =
            (stamp.camera_scale, current_camera_scale)
        {
            stamp.size * (source_scale / current_scale.max(0.001)).clamp(0.05, 20.0)
        } else {
            stamp.size
        };
        (screen, size)
    }

    fn draw_iso_paint_footprints_stamp(
        buffer: &mut TheRGBABuffer,
        surface_buffer: Option<&scenevm::PaintSurfaceBuffer>,
        screen: [i32; 2],
        stamp_depth: Option<f32>,
        owner_geo_id: Option<scenevm::GeoId>,
        size: f32,
        color: [u8; 4],
        opacity: f32,
        variation: u32,
        rotation: f32,
    ) {
        let opacity = opacity.clamp(0.0, 1.0);
        let angle = rotation;
        let forward = [angle.cos(), angle.sin()];
        let side = [-forward[1], forward[0]];
        let step = (size * 4.6).round().clamp(3.0, 16.0);
        let stride = (size * 4.2).round().clamp(3.0, 16.0);
        let foot_len = (size * 3.5).clamp(3.0, 13.0);
        let foot_w = (size * 1.35).clamp(1.2, 5.0);
        let shadow = [8, 6, 5, (opacity * 45.0).round() as u8];
        for i in 0..2 {
            let phase = if i == 0 { -1.0 } else { 1.0 };
            let seed = variation ^ (i as u32 + 1).wrapping_mul(0x9e37_79b9);
            let center = [
                screen[0]
                    + (side[0] * step * phase + forward[0] * stride * phase * 0.55).round() as i32,
                screen[1]
                    + (side[1] * step * phase + forward[1] * stride * phase * 0.55).round() as i32,
            ];
            let foot_angle = angle + phase * 0.16;
            let shade = 0.64 + ((seed >> 12) & 0xff) as f32 / 255.0 * 0.26;
            let mut print = Self::iso_paint_adjust_rgb(color, shade);
            print[3] = (opacity * 190.0).round() as u8;
            Self::draw_iso_paint_rotated_ellipse(
                buffer,
                surface_buffer,
                stamp_depth,
                owner_geo_id,
                [center[0] + 1, center[1] + 1],
                foot_len,
                foot_w,
                foot_angle,
                shadow,
                seed ^ 0x5a5a_0011,
            );
            Self::draw_iso_paint_rotated_ellipse(
                buffer,
                surface_buffer,
                stamp_depth,
                owner_geo_id,
                center,
                foot_len,
                foot_w,
                foot_angle,
                print,
                seed,
            );
            let toe = [
                center[0] + (forward[0] * foot_len * 0.68).round() as i32,
                center[1] + (forward[1] * foot_len * 0.68).round() as i32,
            ];
            for toe_side in [-0.9_f32, 0.0, 0.9] {
                let toe_center = [
                    toe[0] + (side[0] * foot_w * toe_side).round() as i32,
                    toe[1] + (side[1] * foot_w * toe_side).round() as i32,
                ];
                Self::draw_iso_paint_rotated_ellipse(
                    buffer,
                    surface_buffer,
                    stamp_depth,
                    owner_geo_id,
                    toe_center,
                    foot_w * 0.48,
                    foot_w * 0.42,
                    foot_angle,
                    print,
                    seed ^ ((toe_side.to_bits()).wrapping_mul(0x45d9_f3b)),
                );
            }
        }
    }

    fn draw_iso_paint_mud_stamp(
        buffer: &mut TheRGBABuffer,
        surface_buffer: Option<&scenevm::PaintSurfaceBuffer>,
        screen: [i32; 2],
        stamp_depth: Option<f32>,
        owner_geo_id: Option<scenevm::GeoId>,
        size: f32,
        color: [u8; 4],
        opacity: f32,
        variation: u32,
        rotation: f32,
    ) {
        let opacity = opacity.clamp(0.0, 1.0);
        let spread = (size * 9.0).round().clamp(5.0, 34.0) as i32;
        let shadow = [8, 6, 5, (opacity * 48.0).round() as u8];
        let mut base = Self::iso_paint_adjust_rgb(color, 0.78);
        base[3] = (opacity * 165.0).round() as u8;
        for i in 0..3 {
            let seed = variation
                .wrapping_add((i as u32).wrapping_mul(0x4cf5_ad43))
                .rotate_left(((i * 4) as u32) & 15);
            let ox = if i == 0 {
                0
            } else {
                ((seed & 0xff) as i32 - 128) * spread / 260
            };
            let oy = if i == 0 {
                0
            } else {
                (((seed >> 8) & 0xff) as i32 - 128) * spread / 360
            };
            let center = [screen[0] + ox, screen[1] + oy];
            let angle = rotation * 0.18 + (((seed >> 18) & 0xff) as f32 / 255.0 - 0.5) * 0.5;
            let major = size * (4.3 + ((seed >> 10) & 0x7f) as f32 / 52.0);
            let minor = size * (2.0 + ((seed >> 25) & 0x3f) as f32 / 55.0);
            Self::draw_iso_paint_rotated_ellipse(
                buffer,
                surface_buffer,
                stamp_depth,
                owner_geo_id,
                [center[0] + 1, center[1] + 1],
                major,
                minor,
                angle,
                shadow,
                seed ^ 0x011d_1111,
            );
            Self::draw_iso_paint_rotated_ellipse(
                buffer,
                surface_buffer,
                stamp_depth,
                owner_geo_id,
                center,
                major,
                minor,
                angle,
                base,
                seed,
            );
        }

        let bubble_count = 3 + (variation % 4) as i32;
        for i in 0..bubble_count {
            let seed = variation
                .wrapping_add((i as u32).wrapping_mul(0x9e37_79b9))
                .rotate_left(((i * 7) as u32) & 15);
            let ox = ((seed & 0xff) as i32 - 128) * spread / 180;
            let oy = (((seed >> 8) & 0xff) as i32 - 128) * spread / 280;
            let center = [screen[0] + ox, screen[1] + oy];
            let radius = (size * (1.05 + ((seed >> 16) & 0x7f) as f32 / 92.0)).clamp(1.2, 6.0);
            let mut dome = Self::iso_paint_adjust_rgb(color, 1.18);
            dome[3] = (opacity * 122.0).round() as u8;
            let mut rim = Self::iso_paint_adjust_rgb(color, 0.54);
            rim[3] = (opacity * 120.0).round() as u8;
            Self::draw_iso_paint_rotated_ellipse(
                buffer,
                surface_buffer,
                stamp_depth,
                owner_geo_id,
                center,
                radius,
                radius * 0.72,
                rotation,
                rim,
                seed ^ 0x8b8b_0001,
            );
            Self::draw_iso_paint_rotated_ellipse(
                buffer,
                surface_buffer,
                stamp_depth,
                owner_geo_id,
                [center[0], center[1] - 1],
                radius * 0.68,
                radius * 0.44,
                rotation,
                dome,
                seed,
            );
            let highlight = [210, 224, 208, (opacity * 112.0).round() as u8];
            Self::iso_paint_blend_stamp_pixel(
                buffer,
                surface_buffer,
                stamp_depth,
                owner_geo_id,
                center[0] - radius.round() as i32 / 2,
                center[1] - radius.round() as i32 / 2,
                highlight,
            );
        }
    }

    fn draw_iso_paint_rubble_ellipse(
        buffer: &mut TheRGBABuffer,
        surface_buffer: Option<&scenevm::PaintSurfaceBuffer>,
        stamp_depth: Option<f32>,
        owner_geo_id: Option<scenevm::GeoId>,
        center: [i32; 2],
        radius_x: i32,
        radius_y: i32,
        color: [u8; 4],
        variation: u32,
    ) {
        let radius_x = radius_x.max(1);
        let radius_y = radius_y.max(1);
        let rx2 = (radius_x * radius_x).max(1) as f32;
        let ry2 = (radius_y * radius_y).max(1) as f32;
        for y in -radius_y..=radius_y {
            for x in -radius_x..=radius_x {
                let edge = x as f32 * x as f32 / rx2 + y as f32 * y as f32 / ry2;
                if edge > 1.0 {
                    continue;
                }
                let hash = iso_paint_brush::hash_u32(center[0] + x, center[1] + y, variation);
                let noise = (hash & 0xff) as f32 / 255.0;
                let shade = if y <= -radius_y / 3 && x <= radius_x / 3 {
                    1.18 + noise * 0.16
                } else if y >= radius_y / 3 || edge > 0.78 {
                    0.56 + noise * 0.16
                } else {
                    0.80 + noise * 0.24
                };
                let mut pixel = Self::iso_paint_adjust_rgb(color, shade);
                if edge > 0.88 {
                    pixel[3] = ((pixel[3] as f32) * 0.72).round() as u8;
                }
                Self::iso_paint_blend_stamp_pixel(
                    buffer,
                    surface_buffer,
                    stamp_depth,
                    owner_geo_id,
                    center[0] + x,
                    center[1] + y,
                    pixel,
                );
            }
        }
    }

    fn draw_iso_paint_rubble_stamp(
        buffer: &mut TheRGBABuffer,
        surface_buffer: Option<&scenevm::PaintSurfaceBuffer>,
        screen: [i32; 2],
        stamp_depth: Option<f32>,
        owner_geo_id: Option<scenevm::GeoId>,
        size: f32,
        color: [u8; 4],
        opacity: f32,
        variation: u32,
        rotation: f32,
    ) {
        let opacity = opacity.clamp(0.0, 1.0);
        let stone_count = 4 + (variation % 5) as i32;
        let spread = (size * 9.0).round().clamp(5.0, 36.0) as i32;
        let shadow = [9, 8, 7, (opacity * 72.0).round() as u8];
        for i in 0..stone_count {
            let seed = variation
                .wrapping_add((i as u32).wrapping_mul(0x85eb_ca6b))
                .rotate_left(((i * 3) as u32) & 15);
            let ox = ((seed & 0xff) as i32 - 128) * spread / 210;
            let oy = (((seed >> 8) & 0xff) as i32 - 128) * spread / 360;
            let lean = (rotation.sin() * spread as f32 * 0.18).round() as i32;
            let center = [screen[0] + ox + lean, screen[1] + oy];
            let radius_x = (size * (1.7 + ((seed >> 16) & 0x7f) as f32 / 90.0))
                .round()
                .clamp(2.0, 10.0) as i32;
            let radius_y = (radius_x as f32 * (0.42 + ((seed >> 23) & 0x3f) as f32 / 180.0))
                .round()
                .max(1.0) as i32;
            for sx in -radius_x..=radius_x {
                Self::iso_paint_blend_stamp_pixel(
                    buffer,
                    surface_buffer,
                    stamp_depth,
                    owner_geo_id,
                    center[0] + sx,
                    center[1] + radius_y,
                    shadow,
                );
            }
            let shade = 0.68 + ((seed >> 11) & 0xff) as f32 / 255.0 * 0.64;
            let mut stone = Self::iso_paint_adjust_rgb(color, shade);
            stone[3] = (opacity * 235.0).round() as u8;
            Self::draw_iso_paint_rubble_ellipse(
                buffer,
                surface_buffer,
                stamp_depth,
                owner_geo_id,
                center,
                radius_x,
                radius_y,
                stone,
                seed,
            );
        }
    }

    fn draw_iso_paint_grass_stamp(
        buffer: &mut TheRGBABuffer,
        surface_buffer: Option<&scenevm::PaintSurfaceBuffer>,
        screen: [i32; 2],
        stamp_depth: Option<f32>,
        owner_geo_id: Option<scenevm::GeoId>,
        size: f32,
        color: [u8; 4],
        opacity: f32,
        variation: u32,
        rotation: f32,
    ) {
        let opacity = opacity.clamp(0.0, 1.0);
        let blade_count = 5 + (variation % 5) as i32;
        let base_alpha = (opacity * 235.0).round() as u8;
        let base_color = [color[0], color[1], color[2], base_alpha];
        let shadow = [7, 11, 8, (opacity * 72.0).round() as u8];
        let height = (size * 12.0).round().clamp(10.0, 56.0) as i32;
        let spread = (size * 5.0).round().clamp(4.0, 28.0) as i32;
        let dim = *buffer.dim();
        let width = dim.width.max(0) as usize;
        let height_px = dim.height.max(0) as usize;
        for sx in -spread / 2..=spread / 2 {
            let x = screen[0] + sx;
            if Self::iso_paint_stamp_pixel_visible(
                surface_buffer,
                stamp_depth,
                owner_geo_id,
                x,
                screen[1],
            ) {
                Self::iso_paint_blend_pixel(
                    buffer.pixels_mut(),
                    width,
                    height_px,
                    x,
                    screen[1],
                    shadow,
                );
            }
        }
        for i in 0..blade_count {
            let lane = i - blade_count / 2;
            let seed = variation
                .wrapping_add((i as u32).wrapping_mul(0x9e37_79b9))
                .rotate_left((i as u32) & 15);
            let bend = ((seed & 0xff) as i32 - 128) * spread / 190;
            let lean = (rotation.sin() * spread as f32 * 0.45).round() as i32;
            let base_x = screen[0] + lane * spread / blade_count.max(1);
            let top_x = base_x + bend + lean;
            let top_y = screen[1] - height + ((seed >> 8) & 9) as i32;
            let shade = 0.68 + ((seed >> 16) & 0xff) as f32 / 255.0 * 0.68;
            let blade = Self::iso_paint_adjust_rgb(base_color, shade);
            Self::iso_paint_blend_line(
                buffer,
                surface_buffer,
                stamp_depth,
                owner_geo_id,
                base_x,
                screen[1],
                top_x,
                top_y,
                blade,
            );
            if size > 1.7 {
                Self::iso_paint_blend_line(
                    buffer,
                    surface_buffer,
                    stamp_depth,
                    owner_geo_id,
                    base_x + 1,
                    screen[1],
                    top_x + 1,
                    top_y,
                    blade,
                );
            }
        }
    }

    fn draw_iso_paint_stamps(
        buffer: &mut TheRGBABuffer,
        layer: &IsoPaintLayer,
        view: Mat4<f32>,
        proj: Mat4<f32>,
        surface_buffer: Option<&scenevm::PaintSurfaceBuffer>,
        camera: scenevm::Camera3D,
        current_camera_scale: Option<f32>,
    ) {
        if !layer.visible {
            return;
        }
        let dim = *buffer.dim();
        let mut stamps = Vec::new();
        for chunk in layer.chunks.values() {
            for stamp in &chunk.stamps {
                let screen = stamp
                    .world
                    .and_then(|world| {
                        Self::iso_paint_project_world(world, view, proj, dim.width, dim.height)
                    })
                    .unwrap_or(stamp.screen);
                stamps.push((screen[1] as f32 + stamp.sort_depth * 0.001, screen, stamp));
            }
        }
        stamps.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, screen, stamp) in stamps {
            let stamp_depth = stamp
                .world
                .and_then(|world| Self::iso_paint_world_depth(world, camera))
                .or_else(|| {
                    surface_buffer
                        .and_then(|surface| surface.pixel(screen[0], screen[1]))
                        .filter(|pixel| pixel.valid)
                        .map(|pixel| pixel.depth)
                });
            let owner_geo_id = stamp.owner.as_ref().map(Self::iso_paint_owner_geo_id);
            let size = if let (Some(source_scale), Some(current_scale)) =
                (stamp.camera_scale, current_camera_scale)
            {
                stamp.size * (source_scale / current_scale.max(0.001)).clamp(0.05, 20.0)
            } else {
                stamp.size
            };
            Self::draw_iso_paint_stamp_shape(
                buffer,
                stamp,
                surface_buffer,
                screen,
                stamp_depth,
                owner_geo_id,
                size,
            );
        }
    }

    fn iso_paint_stroke_anchor(
        stroke: &IsoPaintStroke,
    ) -> (Option<[i32; 2]>, Option<[f32; 3]>, Option<f32>) {
        if stroke.clip == "object"
            && let Some(point) = stroke
                .points
                .iter()
                .find(|point| point.world.is_some() && point.owner.is_some())
        {
            return (Some(point.screen), point.world, point.camera_scale);
        }
        for point in &stroke.points {
            if let Some(world) = point.world {
                return (Some(point.screen), Some(world), point.camera_scale);
            }
        }
        (stroke.points.first().map(|point| point.screen), None, None)
    }

    fn iso_paint_stroke_bounds(stroke: &IsoPaintStroke) -> ([i32; 2], [i32; 2]) {
        let pad = (stroke.size * 2.0).round().max(1.0) as i32 + 1;
        let min = [stroke.screen_bounds[0] - pad, stroke.screen_bounds[1] - pad];
        let max = [stroke.screen_bounds[2] + pad, stroke.screen_bounds[3] + pad];
        (min, max)
    }

    fn iso_paint_stroke_screen_points(stroke: &IsoPaintStroke) -> Vec<[i32; 2]> {
        stroke.points.iter().map(|point| point.screen).collect()
    }

    fn iso_paint_resampled_point(points: &[[f32; 2]], distance: f32) -> [f32; 2] {
        if points.is_empty() {
            return [0.0, 0.0];
        }
        if points.len() == 1 || distance <= 0.0 {
            return points[0];
        }

        let mut travelled = 0.0;
        for pair in points.windows(2) {
            let a = pair[0];
            let b = pair[1];
            let dx = b[0] - a[0];
            let dy = b[1] - a[1];
            let segment = (dx * dx + dy * dy).sqrt();
            if segment <= f32::EPSILON {
                continue;
            }
            if travelled + segment >= distance {
                let t = ((distance - travelled) / segment).clamp(0.0, 1.0);
                return [a[0] + dx * t, a[1] + dy * t];
            }
            travelled += segment;
        }

        *points.last().unwrap_or(&points[0])
    }

    fn iso_paint_stabilized_arch_points(stroke: &IsoPaintStroke) -> Vec<[i32; 2]> {
        let mut raw = Vec::new();
        for point in &stroke.points {
            let candidate = [point.screen[0] as f32, point.screen[1] as f32];
            if raw.last().is_none_or(|last: &[f32; 2]| {
                let dx = candidate[0] - last[0];
                let dy = candidate[1] - last[1];
                dx * dx + dy * dy >= 4.0
            }) {
                raw.push(candidate);
            }
        }

        if raw.len() < 3 {
            return raw
                .into_iter()
                .map(|point| [point[0].round() as i32, point[1].round() as i32])
                .collect();
        }

        let mut total = 0.0;
        for pair in raw.windows(2) {
            let dx = pair[1][0] - pair[0][0];
            let dy = pair[1][1] - pair[0][1];
            total += (dx * dx + dy * dy).sqrt();
        }
        if total <= f32::EPSILON {
            return Self::iso_paint_stroke_screen_points(stroke);
        }

        let spacing = (stroke.size * 0.65).clamp(3.0, 8.0);
        let count = (total / spacing).ceil().max(2.0) as usize + 1;
        let mut points = Vec::with_capacity(count);
        for index in 0..count {
            let t = index as f32 / (count.saturating_sub(1).max(1)) as f32;
            points.push(Self::iso_paint_resampled_point(&raw, total * t));
        }

        for _ in 0..5 {
            if points.len() < 3 {
                break;
            }
            let mut smoothed = points.clone();
            for index in 1..points.len() - 1 {
                smoothed[index][0] = points[index - 1][0] * 0.25
                    + points[index][0] * 0.5
                    + points[index + 1][0] * 0.25;
                smoothed[index][1] = points[index - 1][1] * 0.25
                    + points[index][1] * 0.5
                    + points[index + 1][1] * 0.25;
            }
            points = smoothed;
        }

        points
            .into_iter()
            .map(|point| [point[0].round() as i32, point[1].round() as i32])
            .collect()
    }

    fn iso_paint_screen_path_local(
        screen_points: &[[i32; 2]],
        origin: [i32; 2],
    ) -> (Vec<[f32; 2]>, Vec<f32>) {
        let mut points = Vec::new();
        for point in screen_points {
            let local = [(point[0] - origin[0]) as f32, (point[1] - origin[1]) as f32];
            if points
                .last()
                .is_none_or(|last: &[f32; 2]| last[0] != local[0] || last[1] != local[1])
            {
                points.push(local);
            }
        }

        let mut lengths = Vec::with_capacity(points.len());
        let mut total = 0.0;
        for index in 0..points.len() {
            if index > 0 {
                let previous = points[index - 1];
                let current = points[index];
                let dx = current[0] - previous[0];
                let dy = current[1] - previous[1];
                total += (dx * dx + dy * dy).sqrt();
            }
            lengths.push(total);
        }

        (points, lengths)
    }

    fn iso_paint_stroke_cache_key(stroke: &IsoPaintStroke) -> u64 {
        let mut hasher = DefaultHasher::new();
        stroke.id.hash(&mut hasher);
        stroke.order.hash(&mut hasher);
        stroke.operation.hash(&mut hasher);
        stroke.brush.hash(&mut hasher);
        stroke.brush_shape.hash(&mut hasher);
        stroke.material_id.hash(&mut hasher);
        stroke.material_mode.hash(&mut hasher);
        stroke.clip.hash(&mut hasher);
        stroke.color.hash(&mut hasher);
        stroke.palette_indices.hash(&mut hasher);
        stroke.palette_colors.hash(&mut hasher);
        stroke.pattern_kind.hash(&mut hasher);
        stroke.pattern_scale.to_bits().hash(&mut hasher);
        stroke.pattern_mortar.to_bits().hash(&mut hasher);
        stroke.pattern_detail.to_bits().hash(&mut hasher);
        stroke.pattern_variation.to_bits().hash(&mut hasher);
        stroke.size.to_bits().hash(&mut hasher);
        stroke.opacity.to_bits().hash(&mut hasher);
        stroke.screen_bounds.hash(&mut hasher);
        stroke.points.len().hash(&mut hasher);
        for point in &stroke.points {
            point.screen.hash(&mut hasher);
            if let Some(world) = point.world {
                for value in world {
                    value.to_bits().hash(&mut hasher);
                }
            }
            if let Some(uv) = point.surface_uv {
                for value in uv {
                    value.to_bits().hash(&mut hasher);
                }
            }
            if let Some(normal) = point.surface_normal {
                for value in normal {
                    value.to_bits().hash(&mut hasher);
                }
            }
            if let Some(camera_scale) = point.camera_scale {
                camera_scale.to_bits().hash(&mut hasher);
            }
            match &point.owner {
                Some(IsoPaintOwner::Unknown(id)) => (0_u8, *id).hash(&mut hasher),
                Some(IsoPaintOwner::Vertex(id)) => (1_u8, *id).hash(&mut hasher),
                Some(IsoPaintOwner::Linedef(id)) => (2_u8, *id).hash(&mut hasher),
                Some(IsoPaintOwner::Sector(id)) => (3_u8, *id).hash(&mut hasher),
                Some(IsoPaintOwner::Character(id)) => (4_u8, *id).hash(&mut hasher),
                Some(IsoPaintOwner::Item(id)) => (5_u8, *id).hash(&mut hasher),
                Some(IsoPaintOwner::Light(id)) => (6_u8, *id).hash(&mut hasher),
                Some(IsoPaintOwner::ItemLight(id)) => (7_u8, *id).hash(&mut hasher),
                Some(IsoPaintOwner::Triangle(id)) => (8_u8, *id).hash(&mut hasher),
                Some(IsoPaintOwner::Terrain { x, z }) => (9_u8, *x, *z).hash(&mut hasher),
                Some(IsoPaintOwner::GeometryObject(id)) => (10_u8, *id).hash(&mut hasher),
                Some(IsoPaintOwner::Hole { sector_id, hole_id }) => {
                    (11_u8, *sector_id, *hole_id).hash(&mut hasher)
                }
                Some(IsoPaintOwner::Gizmo(id)) => (12_u8, *id).hash(&mut hasher),
                None => 255_u8.hash(&mut hasher),
            }
        }
        hasher.finish()
    }

    fn build_iso_paint_stroke_caches(stroke: &IsoPaintStroke) -> Vec<IsoPaintStrokeRenderCache> {
        if stroke.points.is_empty() || stroke.operation == "pick" {
            return Vec::new();
        }

        let erase = stroke.operation == "erase";
        let (origin, max) = Self::iso_paint_stroke_bounds(stroke);
        let width = (max[0] - origin[0] + 1).max(1);
        let height = (max[1] - origin[1] + 1).max(1);
        let mut paint = TheRGBABuffer::new(TheDim::sized(width, height));
        let paint_w = width as usize;
        let paint_h = height as usize;

        let (screen_anchor, world_anchor, camera_scale) = Self::iso_paint_stroke_anchor(stroke);
        let clip_geo_id = stroke
            .points
            .iter()
            .find_map(|point| point.owner.as_ref().map(Self::iso_paint_owner_geo_id));
        let replace_material = stroke.material_mode == "replace";
        let replace_opacity = ((stroke.opacity.clamp(0.0, 1.0) * 254.0).round() as u8).min(254);
        let writes_material = stroke.brush != "screen";
        let color_coverage_scale =
            Self::iso_paint_color_coverage_scale(&stroke.brush, stroke.material_id);
        if !erase && stroke.brush == "brick" && world_anchor.is_none() {
            return Vec::new();
        }

        let color = if erase {
            [
                0,
                0,
                0,
                (stroke.opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
            ]
        } else if stroke.brush == "brick" {
            [
                255,
                255,
                255,
                (stroke.opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
            ]
        } else {
            Self::iso_paint_color_with_opacity(stroke.color, stroke.opacity)
        };
        let radius = (stroke.size * 2.0).round().max(1.0) as i32;
        let mut shape_hasher = DefaultHasher::new();
        stroke.id.hash(&mut shape_hasher);
        stroke.brush_shape.hash(&mut shape_hasher);
        let shape_seed = shape_hasher.finish() as u32;
        let pixels = paint.pixels_mut();
        let arch_pattern =
            stroke.brush == "brick" && matches!(stroke.pattern_kind.as_str(), "arch" | "trim");
        let render_points = if arch_pattern {
            Self::iso_paint_stabilized_arch_points(stroke)
        } else {
            Self::iso_paint_stroke_screen_points(stroke)
        };

        if render_points.len() == 1 {
            let point = render_points[0];
            Self::iso_paint_stamp_coverage(
                pixels,
                paint_w,
                paint_h,
                point[0] - origin[0],
                point[1] - origin[1],
                radius,
                color,
                &stroke.palette_colors,
                &stroke.brush,
                &stroke.brush_shape,
                shape_seed,
            );
        } else {
            for pair in render_points.windows(2) {
                Self::iso_paint_draw_segment_coverage(
                    pixels,
                    paint_w,
                    paint_h,
                    pair[0],
                    pair[1],
                    origin,
                    radius,
                    color,
                    &stroke.palette_colors,
                    &stroke.brush,
                    &stroke.brush_shape,
                    shape_seed,
                );
            }
        }

        let (path_points, path_lengths) = Self::iso_paint_screen_path_local(&render_points, origin);

        vec![IsoPaintStrokeRenderCache {
            order: stroke.order,
            origin,
            screen_anchor,
            world_anchor,
            camera_scale,
            clip_geo_id,
            color_coverage_scale,
            replace_material,
            replace_opacity,
            writes_material,
            brush: stroke.brush.clone(),
            clip: stroke.clip.clone(),
            material_id: stroke.material_id,
            color: Self::iso_paint_color_with_opacity(stroke.color, 1.0),
            pattern_kind: stroke.pattern_kind.clone(),
            pattern_scale: stroke.pattern_scale,
            pattern_mortar: stroke.pattern_mortar,
            pattern_detail: stroke.pattern_detail,
            pattern_variation: stroke.pattern_variation,
            path_points,
            path_lengths,
            erase,
            buffer: paint,
        }]
    }

    fn build_iso_paint_chunk_cache(
        chunk: &IsoPaintChunk,
        previous: Option<IsoPaintChunkRenderCache>,
    ) -> IsoPaintChunkRenderCache {
        let mut previous_strokes = previous
            .map(|cache| cache.stroke_caches)
            .unwrap_or_default();
        let mut stroke_caches = HashMap::new();
        let mut strokes = Vec::new();

        for stroke in &chunk.strokes {
            let key = Self::iso_paint_stroke_cache_key(stroke);
            let cached = previous_strokes
                .remove(&stroke.id)
                .filter(|cached| cached.key == key)
                .unwrap_or_else(|| IsoPaintCachedStrokeRender {
                    key,
                    strokes: Self::build_iso_paint_stroke_caches(stroke),
                });
            strokes.extend(cached.strokes.iter().cloned());
            stroke_caches.insert(stroke.id, cached);
        }

        IsoPaintChunkRenderCache {
            revision: chunk.revision,
            strokes,
            stroke_caches,
        }
    }

    fn ensure_iso_paint_chunk_caches(
        render_cache: &mut IsoPaintRenderCache,
        layer: &IsoPaintLayer,
    ) {
        render_cache
            .chunks
            .retain(|key, _| layer.chunks.contains_key(key));

        for (key, chunk) in &layer.chunks {
            let rebuild = render_cache
                .chunks
                .get(key)
                .map(|cached| cached.revision != chunk.revision)
                .unwrap_or(true);
            if rebuild {
                let previous = render_cache.chunks.remove(key);
                let cached = Self::build_iso_paint_chunk_cache(chunk, previous);
                render_cache.chunks.insert(key.clone(), cached);
            }
        }
    }

    fn iso_paint_render_order_key(
        order: u64,
        chunk_index: usize,
        local_index: usize,
    ) -> (u8, u64, usize, usize) {
        ((order != 0) as u8, order, chunk_index, local_index)
    }

    fn ordered_iso_paint_strokes<'a>(
        render_cache: &'a IsoPaintRenderCache,
        layer: &IsoPaintLayer,
    ) -> Vec<&'a IsoPaintStrokeRenderCache> {
        let mut strokes = Vec::new();
        for (chunk_index, (key, _chunk)) in layer.chunks.iter().enumerate() {
            let Some(cached) = render_cache.chunks.get(key) else {
                continue;
            };
            for (stroke_index, stroke) in cached.strokes.iter().enumerate() {
                strokes.push((
                    Self::iso_paint_render_order_key(stroke.order, chunk_index, stroke_index),
                    stroke,
                ));
            }
        }
        strokes.sort_by_key(|(key, _)| *key);
        strokes.into_iter().map(|(_, stroke)| stroke).collect()
    }

    fn ordered_iso_paint_render_items<'a>(
        render_cache: &'a IsoPaintRenderCache,
        layer: &'a IsoPaintLayer,
    ) -> Vec<IsoPaintRenderItem<'a>> {
        let mut items = Vec::new();
        for (chunk_index, (key, chunk)) in layer.chunks.iter().enumerate() {
            let stroke_count = render_cache
                .chunks
                .get(key)
                .map(|cached| {
                    for (stroke_index, stroke) in cached.strokes.iter().enumerate() {
                        items.push((
                            Self::iso_paint_render_order_key(
                                stroke.order,
                                chunk_index,
                                stroke_index,
                            ),
                            IsoPaintRenderItem::Stroke(stroke),
                        ));
                    }
                    cached.strokes.len()
                })
                .unwrap_or(0);

            for (stamp_index, stamp) in chunk.stamps.iter().enumerate() {
                items.push((
                    Self::iso_paint_render_order_key(
                        stamp.order,
                        chunk_index,
                        stroke_count + stamp_index,
                    ),
                    IsoPaintRenderItem::Stamp(stamp),
                ));
            }
        }
        items.sort_by_key(|(key, _)| *key);
        items.into_iter().map(|(_, item)| item).collect()
    }

    fn iso_paint_layer_key(layer: &IsoPaintLayer) -> u64 {
        let mut hasher = DefaultHasher::new();
        layer.visible.hash(&mut hasher);
        layer.chunks.len().hash(&mut hasher);
        for (key, chunk) in &layer.chunks {
            key.hash(&mut hasher);
            chunk.origin.hash(&mut hasher);
            chunk.revision.hash(&mut hasher);
            chunk.stamp_revision.hash(&mut hasher);
            chunk.strokes.len().hash(&mut hasher);
            chunk.stamps.len().hash(&mut hasher);
        }
        hasher.finish()
    }

    fn iso_paint_camera_key(camera: scenevm::Camera3D) -> u64 {
        let mut hasher = DefaultHasher::new();
        let kind = match camera.kind {
            scenevm::CameraKind::OrthoIso => 0_u8,
            scenevm::CameraKind::OrbitPersp => 1_u8,
            scenevm::CameraKind::FirstPersonPersp => 2_u8,
        };
        kind.hash(&mut hasher);
        for value in [
            camera.pos.x,
            camera.pos.y,
            camera.pos.z,
            camera.forward.x,
            camera.forward.y,
            camera.forward.z,
            camera.right.x,
            camera.right.y,
            camera.right.z,
            camera.up.x,
            camera.up.y,
            camera.up.z,
            camera.vfov_deg,
            camera.ortho_half_h,
            camera.near,
            camera.far,
        ] {
            value.to_bits().hash(&mut hasher);
        }
        hasher.finish()
    }

    fn iso_paint_overlay_key(
        region_id: Uuid,
        render_context: u8,
        layer: &IsoPaintLayer,
        target_dim: TheDim,
        paint_surface_key: u64,
        camera_key: u64,
        current_camera_scale: Option<f32>,
    ) -> IsoPaintPreparedOverlayKey {
        IsoPaintPreparedOverlayKey {
            region_id,
            render_context,
            width: target_dim.width,
            height: target_dim.height,
            layer_key: Self::iso_paint_layer_key(layer),
            surface_key: paint_surface_key,
            camera_key,
            camera_scale_bits: current_camera_scale.unwrap_or(0.0).to_bits(),
        }
    }

    fn build_iso_paint_overlay_gpu_commands(
        render_cache: &mut IsoPaintRenderCache,
        region_id: Uuid,
        render_context: u8,
        layer: &IsoPaintLayer,
        paint_surface_key: u64,
        camera_key: u64,
        current_camera_scale: Option<f32>,
        target_dim: TheDim,
        paint_surface: Option<&scenevm::PaintSurfaceBuffer>,
        project_world_anchor: impl Fn([f32; 3], i32, i32) -> Option<[i32; 2]>,
    ) -> Option<(
        IsoPaintPreparedOverlayKey,
        Vec<Raster3DPaintGpuStroke>,
        Vec<scenevm::GeoId>,
    )> {
        if render_cache.region_id != Some(region_id) {
            render_cache.region_id = Some(region_id);
            render_cache.chunks.clear();
            render_cache.prepared_key = None;
            render_cache.prepared_overlay = None;
            render_cache.uploaded_key = None;
        }

        if !layer.visible
            || layer.chunks.is_empty()
            || target_dim.width <= 0
            || target_dim.height <= 0
        {
            return None;
        }
        if layer.chunks.values().any(|chunk| {
            !chunk.stamps.is_empty() || chunk.strokes.iter().any(|s| s.brush == "brick")
        }) {
            return None;
        }
        let paint_surface_key = paint_surface
            .map(|surface| paint_surface_key ^ surface.content_key())
            .unwrap_or(paint_surface_key);
        let overlay_key = Self::iso_paint_overlay_key(
            region_id,
            render_context,
            layer,
            target_dim,
            paint_surface_key,
            camera_key,
            current_camera_scale,
        );

        Self::ensure_iso_paint_chunk_caches(render_cache, layer);

        let mut gpu_strokes = Vec::new();
        let mut paint_alpha_geo_ids = Vec::new();
        let mut seen_alpha_geo_ids = HashSet::new();

        for stroke in Self::ordered_iso_paint_strokes(render_cache, layer) {
            let mut draw_origin = stroke.origin;
            let mut draw_scale = 1.0;
            let mut start_screen = stroke.screen_anchor;
            if let (Some(screen_anchor), Some(world_anchor)) =
                (stroke.screen_anchor, stroke.world_anchor)
                && let Some(current_screen) =
                    project_world_anchor(world_anchor, target_dim.width, target_dim.height)
            {
                if let (Some(source_scale), Some(current_scale)) =
                    (stroke.camera_scale, current_camera_scale)
                {
                    draw_scale = (source_scale / current_scale.max(0.001)).clamp(0.05, 20.0);
                }
                let anchor_local_x = screen_anchor[0] - stroke.origin[0];
                let anchor_local_y = screen_anchor[1] - stroke.origin[1];
                draw_origin[0] =
                    (current_screen[0] as f32 - anchor_local_x as f32 * draw_scale).round() as i32;
                draw_origin[1] =
                    (current_screen[1] as f32 - anchor_local_y as f32 * draw_scale).round() as i32;
                start_screen = Some(current_screen);
            }

            let brush_dim = *stroke.buffer.dim();
            if brush_dim.width <= 0 || brush_dim.height <= 0 {
                continue;
            }
            let draw_width = ((brush_dim.width as f32) * draw_scale).round().max(1.0) as u32;
            let draw_height = ((brush_dim.height as f32) * draw_scale).round().max(1.0) as u32;
            let resolved_clip_geo_id = Self::iso_paint_brush_clip_geo_id(
                paint_surface,
                &stroke.clip,
                stroke.clip_geo_id,
                start_screen,
                &stroke.buffer,
                draw_origin,
                draw_scale,
            );
            gpu_strokes.push(Raster3DPaintGpuStroke {
                brush_width: brush_dim.width as u32,
                brush_height: brush_dim.height as u32,
                brush_rgba: stroke.buffer.pixels().to_vec(),
                draw_x: draw_origin[0],
                draw_y: draw_origin[1],
                draw_width,
                draw_height,
                scale: draw_scale,
                clip_mode: (stroke.clip != "none") as u32,
                start_screen,
                clip_geo_id: resolved_clip_geo_id,
                color_coverage_scale: stroke.color_coverage_scale,
                replace_material: stroke.replace_material,
                replace_opacity: stroke.replace_opacity,
                writes_material: stroke.writes_material,
                material_id: stroke.material_id,
                erase: stroke.erase,
            });

            if !stroke.erase
                && stroke.writes_material
                && (!stroke.replace_material
                    || !(stroke.replace_opacity == 254
                        && !Self::iso_paint_material_is_translucent(stroke.material_id)))
                && let Some(geo_id) = resolved_clip_geo_id
                && seen_alpha_geo_ids.insert(geo_id)
            {
                paint_alpha_geo_ids.push(geo_id);
            }
        }

        Some((overlay_key, gpu_strokes, paint_alpha_geo_ids))
    }

    fn build_iso_paint_overlay_prepared(
        render_cache: &mut IsoPaintRenderCache,
        region_id: Uuid,
        render_context: u8,
        layer: &IsoPaintLayer,
        paint_surface: Option<&scenevm::PaintSurfaceBuffer>,
        paint_surface_key: u64,
        camera_key: u64,
        current_camera_scale: Option<f32>,
        target_dim: TheDim,
        project_world_anchor: impl Fn([f32; 3], i32, i32) -> Option<[i32; 2]>,
    ) -> Option<(IsoPaintPreparedOverlayKey, IsoPaintPreparedOverlay, bool)> {
        if render_cache.region_id != Some(region_id) {
            render_cache.region_id = Some(region_id);
            render_cache.chunks.clear();
            render_cache.prepared_key = None;
            render_cache.prepared_overlay = None;
            render_cache.uploaded_key = None;
        }

        if !layer.visible
            || layer.chunks.is_empty()
            || target_dim.width <= 0
            || target_dim.height <= 0
        {
            return None;
        }

        let paint_surface_key = paint_surface
            .map(|surface| paint_surface_key ^ surface.content_key())
            .unwrap_or(paint_surface_key);
        let overlay_key = Self::iso_paint_overlay_key(
            region_id,
            render_context,
            layer,
            target_dim,
            paint_surface_key,
            camera_key,
            current_camera_scale,
        );
        if render_cache.prepared_key == Some(overlay_key)
            && let Some(overlay) = render_cache.prepared_overlay.as_ref()
        {
            return Some((overlay_key, overlay.clone(), false));
        }

        let mut paint_overlay = TheRGBABuffer::new(target_dim);
        let mut material_overlay =
            vec![0_u8; target_dim.width as usize * target_dim.height as usize * 4];
        for pixel in material_overlay.chunks_exact_mut(4) {
            pixel.copy_from_slice(&Self::iso_paint_material_pixel(0, None, 0));
        }

        Self::ensure_iso_paint_chunk_caches(render_cache, layer);

        for item in Self::ordered_iso_paint_render_items(render_cache, layer) {
            match item {
                IsoPaintRenderItem::Stroke(stroke) => {
                    // Iso Paint is authored for the fixed iso canvas. World anchors
                    // are used only in iso views so pan/zoom keeps screen-art aligned;
                    // first-person/orbit paths clear the overlay instead.
                    let mut draw_origin = stroke.origin;
                    let mut draw_scale = 1.0;
                    let mut start_screen = stroke.screen_anchor;
                    if let (Some(screen_anchor), Some(world_anchor)) =
                        (stroke.screen_anchor, stroke.world_anchor)
                    {
                        if let Some(current_screen) =
                            project_world_anchor(world_anchor, target_dim.width, target_dim.height)
                        {
                            if let (Some(source_scale), Some(current_scale)) =
                                (stroke.camera_scale, current_camera_scale)
                            {
                                draw_scale =
                                    (source_scale / current_scale.max(0.001)).clamp(0.05, 20.0);
                            }
                            let anchor_local_x = screen_anchor[0] - stroke.origin[0];
                            let anchor_local_y = screen_anchor[1] - stroke.origin[1];
                            draw_origin[0] = (current_screen[0] as f32
                                - anchor_local_x as f32 * draw_scale)
                                .round() as i32;
                            draw_origin[1] = (current_screen[1] as f32
                                - anchor_local_y as f32 * draw_scale)
                                .round() as i32;
                            start_screen = Some(current_screen);
                        }
                    }

                    if stroke.erase {
                        Self::iso_paint_clear_overlay_scaled_at(
                            &mut paint_overlay,
                            &mut material_overlay,
                            &stroke.buffer,
                            paint_surface,
                            &stroke.clip,
                            start_screen,
                            stroke.clip_geo_id,
                            stroke.writes_material,
                            draw_origin[0],
                            draw_origin[1],
                            draw_scale,
                        );
                    } else if stroke.brush == "brick" {
                        Self::iso_paint_composite_brick_overlay_scaled_at(
                            &mut paint_overlay,
                            &mut material_overlay,
                            &stroke.buffer,
                            paint_surface,
                            &stroke.clip,
                            stroke.material_id,
                            start_screen,
                            stroke.clip_geo_id,
                            stroke.replace_material,
                            stroke.replace_opacity,
                            draw_origin[0],
                            draw_origin[1],
                            draw_scale,
                            stroke.color,
                            &stroke.pattern_kind,
                            stroke.pattern_scale,
                            stroke.pattern_mortar,
                            stroke.pattern_detail,
                            stroke.pattern_variation,
                            &stroke.path_points,
                            &stroke.path_lengths,
                        );
                    } else {
                        Self::iso_paint_composite_overlay_scaled_at(
                            &mut paint_overlay,
                            &mut material_overlay,
                            &stroke.buffer,
                            paint_surface,
                            &stroke.clip,
                            stroke.material_id,
                            start_screen,
                            stroke.clip_geo_id,
                            stroke.color_coverage_scale,
                            stroke.replace_material,
                            stroke.replace_opacity,
                            stroke.writes_material,
                            draw_origin[0],
                            draw_origin[1],
                            draw_scale,
                        );
                    }
                }
                IsoPaintRenderItem::Stamp(stamp) => {
                    let (screen, size) = Self::iso_paint_stamp_screen_and_size(
                        stamp,
                        target_dim.width,
                        target_dim.height,
                        current_camera_scale,
                        &project_world_anchor,
                    );
                    let owner_geo_id = stamp.owner.as_ref().map(Self::iso_paint_owner_geo_id);
                    Self::iso_paint_write_stamp_material(
                        &mut material_overlay,
                        target_dim.width as usize,
                        target_dim.height as usize,
                        paint_surface,
                        stamp,
                        screen,
                        owner_geo_id,
                        size,
                    );
                }
            }
        }

        let mut paint_alpha_geo_ids = Self::iso_paint_alpha_geo_ids(
            &material_overlay,
            target_dim.width as usize,
            target_dim.height as usize,
            paint_surface,
        );
        let mut seen_paint_alpha_geo_ids: HashSet<scenevm::GeoId> =
            paint_alpha_geo_ids.iter().copied().collect();
        for chunk_cache in render_cache.chunks.values() {
            for stroke in &chunk_cache.strokes {
                if stroke.erase
                    || !stroke.writes_material
                    || (stroke.replace_material
                        && stroke.replace_opacity == 254
                        && !Self::iso_paint_material_is_translucent(stroke.material_id))
                {
                    continue;
                }
                if let Some(geo_id) = stroke.clip_geo_id
                    && seen_paint_alpha_geo_ids.insert(geo_id)
                {
                    paint_alpha_geo_ids.push(geo_id);
                }
            }
        }
        let overlay = IsoPaintPreparedOverlay {
            width: target_dim.width as u32,
            height: target_dim.height as u32,
            color_rgba: paint_overlay.pixels().to_vec(),
            material_rgba: material_overlay,
            paint_alpha_geo_ids,
        };
        render_cache.prepared_key = Some(overlay_key);
        render_cache.prepared_overlay = Some(overlay.clone());
        Some((overlay_key, overlay, true))
    }

    pub fn upload_overlay_cached(
        render_cache: &mut IsoPaintRenderCache,
        region_id: Uuid,
        render_context: u8,
        layer: &IsoPaintLayer,
        vm: &mut SceneVM,
        camera: Camera3D,
        view: Mat4<f32>,
        proj: Mat4<f32>,
        width: u32,
        height: u32,
        current_camera_scale: Option<f32>,
    ) -> bool {
        if width == 0
            || height == 0
            || !layer.visible
            || layer.chunks.is_empty()
            || !matches!(camera.kind, CameraKind::OrthoIso)
        {
            if render_cache.uploaded_key.take().is_some() {
                vm.execute(Atom::ClearRaster3DPaintOverlay);
                render_cache.prepared_key = None;
                render_cache.prepared_overlay = None;
                return true;
            }
            return false;
        }

        let target_dim = TheDim::sized(width as i32, height as i32);
        let base_paint_surface_key = vm.paint_surface_key(width, height);
        let camera_key = Self::iso_paint_camera_key(camera);
        let gpu_paint_surface_key = base_paint_surface_key ^ 0x4750_5553_5552_4643;
        let cpu_paint_surface = vm.paint_surface_buffer(width, height);
        let gpu_surface = Raster3DPaintGpuSurface::from_paint_surface(&cpu_paint_surface);
        if let Some((key, gpu_strokes, paint_alpha_geo_ids)) =
            Self::build_iso_paint_overlay_gpu_commands(
                render_cache,
                region_id,
                render_context,
                layer,
                gpu_paint_surface_key,
                camera_key,
                current_camera_scale,
                target_dim,
                Some(&cpu_paint_surface),
                |point, width, height| {
                    Self::iso_paint_project_world(point, view, proj, width, height)
                },
            )
        {
            if render_cache.uploaded_key == Some(key) {
                return false;
            }
            if vm.set_raster3d_paint_overlay_gpu(
                width,
                height,
                &gpu_strokes,
                Some(&gpu_surface),
                paint_alpha_geo_ids,
            ) {
                render_cache.uploaded_key = Some(key);
                render_cache.prepared_key = Some(key);
                render_cache.prepared_overlay = None;
                return true;
            }
        }

        let paint_surface = vm.paint_surface_buffer(width, height);
        let paint_surface_key = base_paint_surface_key;
        let overlay = Self::build_iso_paint_overlay_prepared(
            render_cache,
            region_id,
            render_context,
            layer,
            Some(&paint_surface),
            paint_surface_key,
            camera_key,
            current_camera_scale,
            target_dim,
            |point, width, height| Self::iso_paint_project_world(point, view, proj, width, height),
        );

        if let Some((key, overlay, changed)) = overlay {
            let needs_upload = changed || render_cache.uploaded_key != Some(key);
            if needs_upload {
                vm.execute(Atom::SetRaster3DPaintOverlay {
                    width: overlay.width,
                    height: overlay.height,
                    color_rgba: overlay.color_rgba,
                    material_rgba: overlay.material_rgba,
                    paint_alpha_geo_ids: overlay.paint_alpha_geo_ids,
                });
                render_cache.uploaded_key = Some(key);
            }
            needs_upload
        } else {
            if render_cache.uploaded_key.take().is_some() {
                vm.execute(Atom::ClearRaster3DPaintOverlay);
                return true;
            }
            false
        }
    }

    pub fn draw_stamps(
        buffer: &mut TheRGBABuffer,
        layer: &IsoPaintLayer,
        view: Mat4<f32>,
        proj: Mat4<f32>,
        surface_buffer: Option<&PaintSurfaceBuffer>,
        camera: Camera3D,
        current_camera_scale: Option<f32>,
    ) {
        Self::draw_iso_paint_stamps(
            buffer,
            layer,
            view,
            proj,
            surface_buffer,
            camera,
            current_camera_scale,
        );
    }
}
