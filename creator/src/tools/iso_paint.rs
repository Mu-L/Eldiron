use crate::editor::{DOCKMANAGER, RUSTERIX};
use crate::prelude::*;
use MapEvent::*;
use ToolEvent::*;
use rusterix::material_library::MaterialDefinition;

const ISO_PAINT_MIN_BRUSH_SIZE: f32 = 0.05;
const ISO_PAINT_MAX_PAINT_BRUSH_SIZE: f32 = 16.0;
const ISO_PAINT_MAX_STAMP_BRUSH_SIZE: f32 = 8.0;
const ISO_PAINT_PATTERN_KIND: &str = "Iso Paint Pattern Kind";
const ISO_PAINT_PATTERN_SCALE: &str = "Iso Paint Pattern Scale";
const ISO_PAINT_MORTAR: &str = "Iso Paint Mortar";
const ISO_PAINT_PATTERN_DETAIL: &str = "Iso Paint Pattern Detail";
const ISO_PAINT_PATTERN_VARIATION: &str = "Iso Paint Pattern Variation";

pub struct IsoPaintTool {
    id: TheId,
    painting: bool,
    previous_dock: Option<String>,
    active_stroke: Option<Uuid>,
    last_stamp_screen: Option<[i32; 2]>,
    stamp_clip_owner: Option<IsoPaintOwner>,
    stroke_before: Option<Region>,
    stroke_changed: bool,
}

impl IsoPaintTool {
    fn active_size_max(layer: &IsoPaintLayer) -> f32 {
        if Self::is_stamp_mode(layer) {
            ISO_PAINT_MAX_STAMP_BRUSH_SIZE
        } else {
            ISO_PAINT_MAX_PAINT_BRUSH_SIZE
        }
    }

    fn neutral_material_palette(project: &Project) -> (u16, [u8; 4]) {
        let target = [132i32, 132i32, 128i32];
        let mut best: Option<(u16, [u8; 4], i32)> = None;
        for (index, entry) in project.art_palette.colors.iter().enumerate() {
            let Some(color) = entry.as_ref() else {
                continue;
            };
            let mut color = color.to_u8_array();
            color[3] = 255;
            let dr = color[0] as i32 - target[0];
            let dg = color[1] as i32 - target[1];
            let db = color[2] as i32 - target[2];
            let score = dr * dr + dg * dg + db * db;
            if best.map_or(true, |(_, _, best_score)| score < best_score) {
                best = Some((index as u16, color, score));
            }
        }
        best.map(|(index, color, _)| (index, color))
            .unwrap_or((6, [132, 132, 128, 255]))
    }

    fn material_color_needs_gray(index: Option<u16>, color: [u8; 4]) -> bool {
        let average = (color[0] as u16 + color[1] as u16 + color[2] as u16) / 3;
        index == Some(0) || average < 58 || (color[0] > 150 && color[1] < 90 && color[2] < 90)
    }

    fn ensure_initial_material_settings(region: &mut Region, neutral: (u16, [u8; 4])) {
        let active_brush = region.iso_paint.active_brush.as_str();
        let active_index = region.iso_paint.active_palette_indices.first().copied();
        let active_color = region
            .iso_paint
            .active_palette_colors
            .first()
            .copied()
            .unwrap_or(region.iso_paint.active_color);
        let needs_material_seed = active_brush.is_empty()
            || active_brush == "screen"
            || (active_brush == "material"
                && (region.iso_paint.active_palette_colors.is_empty()
                    || Self::material_color_needs_gray(active_index, active_color)));

        if !needs_material_seed {
            return;
        }

        let (palette_index, color) = neutral;
        let size = if region.iso_paint.active_size <= 1.001 {
            8.0
        } else {
            region.iso_paint.active_size
        };
        let opacity = if region.iso_paint.active_opacity <= 0.0 {
            1.0
        } else {
            region.iso_paint.active_opacity
        };
        let material_id = MaterialDefinition::from_preset_finish("default", "natural").id();
        region.iso_paint.set_active_settings(
            "draw",
            "material",
            "solid",
            "default",
            "natural",
            material_id,
            "coat",
            "object",
            color,
            vec![palette_index],
            vec![color],
            region.iso_paint.active_pattern_kind.clone(),
            region.iso_paint.active_pattern_scale,
            region.iso_paint.active_pattern_mortar,
            region.iso_paint.active_pattern_detail,
            region.iso_paint.active_pattern_variation,
            region.iso_paint.active_stamp_density,
            region.iso_paint.active_stamp_size_jitter,
            region.iso_paint.active_stamp_rotation_jitter,
            "wildflowers",
            size,
            opacity,
        );
    }

    fn is_stamp_mode(layer: &IsoPaintLayer) -> bool {
        matches!(
            layer.active_brush.as_str(),
            "grass"
                | "rubble"
                | "leaves"
                | "flowers"
                | "vines"
                | "roots"
                | "bushes"
                | "tree"
                | "candles"
                | "footprints"
                | "mud"
        ) && layer.active_material_mode == "stamp"
    }

    fn stamp_label(layer: &IsoPaintLayer) -> &'static str {
        match layer.active_brush.as_str() {
            "rubble" => "rubble",
            "leaves" => "leaves",
            "flowers" => "flowers",
            "vines" => "vines",
            "roots" => "roots",
            "bushes" => "bushes",
            "tree" => "tree",
            "candles" => "candles",
            "footprints" => "footprints",
            "mud" => "mud",
            _ => "grass",
        }
    }

    fn should_place_stamp(
        last: Option<[i32; 2]>,
        coord: Vec2<i32>,
        size: f32,
        density: f32,
    ) -> bool {
        let Some(last) = last else {
            return true;
        };
        let density = density.clamp(0.0, 1.0);
        let spacing_scale = 1.55 - density * 0.9;
        let spacing = (size * 9.0 * spacing_scale).round().clamp(5.0, 42.0) as i32;
        let dx = coord.x - last[0];
        let dy = coord.y - last[1];
        dx * dx + dy * dy >= spacing * spacing
    }

    fn stamp_clip_owner(layer: &IsoPaintLayer, point: &IsoPaintPoint) -> Option<IsoPaintOwner> {
        (layer.active_clip == "object")
            .then(|| point.owner.clone())
            .flatten()
    }

    fn stamp_point_matches_clip(point: &IsoPaintPoint, clip_owner: Option<&IsoPaintOwner>) -> bool {
        clip_owner.is_none_or(|clip_owner| {
            point
                .owner
                .as_ref()
                .is_some_and(|owner| clip_owner.same_paint_object(owner))
        })
    }

    fn apply_stamp_at(
        region: &mut Region,
        point: IsoPaintPoint,
        clip_owner: Option<&IsoPaintOwner>,
    ) -> bool {
        if !Self::stamp_point_matches_clip(&point, clip_owner) {
            return false;
        }
        if region.iso_paint.active_operation == "erase" {
            let active_brush = region.iso_paint.active_brush.clone();
            region.iso_paint.erase_stamps_near_owner_kind(
                point.screen,
                region.iso_paint.active_size,
                clip_owner,
                Some(active_brush.as_str()),
            )
        } else if region.iso_paint.active_operation == "draw" {
            region.iso_paint.add_stamp(point);
            true
        } else {
            false
        }
    }

    fn sync_live_paint_settings(ui: &mut TheUI, region: &mut Region) {
        if let Some(opacity) = ui
            .get_widget_value("Iso Paint Tool Opacity")
            .and_then(|value| value.to_f32())
        {
            region.iso_paint.active_opacity = opacity.clamp(0.0, 1.0);
        }
        if let Some(TheValue::Int(index)) = ui.get_widget_value("Iso Paint Material Mode") {
            region.iso_paint.active_material_mode = match index {
                1 => "replace".to_string(),
                2 => "stamp".to_string(),
                _ => "coat".to_string(),
            };
        }
        if let Some(TheValue::Int(index)) = ui.get_widget_value(ISO_PAINT_PATTERN_KIND) {
            region.iso_paint.active_pattern_kind = match index {
                0 => "tile".to_string(),
                2 => "arch".to_string(),
                _ => "brick".to_string(),
            };
        }
        if let Some(pattern_scale) = ui
            .get_widget_value(ISO_PAINT_PATTERN_SCALE)
            .and_then(|value| value.to_f32())
        {
            region.iso_paint.active_pattern_scale = pattern_scale.clamp(0.25, 4.0);
        }
        if let Some(mortar) = ui
            .get_widget_value(ISO_PAINT_MORTAR)
            .and_then(|value| value.to_f32())
        {
            region.iso_paint.active_pattern_mortar = mortar.clamp(0.0, 0.4);
        }
        if let Some(detail) = ui
            .get_widget_value(ISO_PAINT_PATTERN_DETAIL)
            .and_then(|value| value.to_f32())
        {
            region.iso_paint.active_pattern_detail = detail.clamp(0.0, 1.0);
        }
        if let Some(variation) = ui
            .get_widget_value(ISO_PAINT_PATTERN_VARIATION)
            .and_then(|value| value.to_f32())
        {
            region.iso_paint.active_pattern_variation = variation.clamp(0.0, 1.0);
        }
        if let Some(size) = ui
            .get_widget_value("Iso Paint Tool Size")
            .and_then(|value| value.to_f32())
        {
            region.iso_paint.active_size = size.clamp(
                ISO_PAINT_MIN_BRUSH_SIZE,
                Self::active_size_max(&region.iso_paint),
            );
        }
        if let Some(size_jitter) = ui
            .get_widget_value("Iso Paint Stamp Size Jitter")
            .and_then(|value| value.to_f32())
        {
            region.iso_paint.active_stamp_size_jitter = size_jitter.clamp(0.0, 1.0);
        }
        if let Some(rotation_jitter) = ui
            .get_widget_value("Iso Paint Stamp Rotation Jitter")
            .and_then(|value| value.to_f32())
        {
            region.iso_paint.active_stamp_rotation_jitter = rotation_jitter.clamp(0.0, 1.0);
        }
    }

    fn hit_status(server_ctx: &ServerContext) -> String {
        if server_ctx.geo_hit.is_some() {
            fl!("status_iso_paint_hit")
        } else if server_ctx.hover_cursor_3d.is_some() {
            fl!("status_iso_paint_ground")
        } else {
            fl!("status_iso_paint_active")
        }
    }

    fn owner_from_geo_id(geo_id: scenevm::GeoId) -> IsoPaintOwner {
        match geo_id {
            scenevm::GeoId::Unknown(id) => IsoPaintOwner::Unknown(id),
            scenevm::GeoId::Vertex(id) => IsoPaintOwner::Vertex(id),
            scenevm::GeoId::Linedef(id) => IsoPaintOwner::Linedef(id),
            scenevm::GeoId::Sector(id) => IsoPaintOwner::Sector(id),
            scenevm::GeoId::Character(id) => IsoPaintOwner::Character(id),
            scenevm::GeoId::Item(id) => IsoPaintOwner::Item(id),
            scenevm::GeoId::Light(id) => IsoPaintOwner::Light(id),
            scenevm::GeoId::ItemLight(id) => IsoPaintOwner::ItemLight(id),
            scenevm::GeoId::Triangle(id) => IsoPaintOwner::Triangle(id),
            scenevm::GeoId::Terrain(x, z) => IsoPaintOwner::Terrain { x, z },
            scenevm::GeoId::GeometryObject(id) => IsoPaintOwner::GeometryObject(id),
            scenevm::GeoId::Hole(sector_id, hole_id) => IsoPaintOwner::Hole { sector_id, hole_id },
            scenevm::GeoId::Gizmo(id) => IsoPaintOwner::Gizmo(id),
        }
    }

    fn paint_point(coord: Vec2<i32>, server_ctx: &ServerContext) -> IsoPaintPoint {
        let owner = server_ctx.geo_hit.map(Self::owner_from_geo_id);
        let world = if server_ctx.geo_hit.is_some() {
            Some(server_ctx.geo_hit_pos)
        } else {
            server_ctx.hover_cursor_3d
        };
        let surface_uv = server_ctx.hover_surface.as_ref().and_then(|surface| {
            server_ctx
                .hover_surface_hit_pos
                .map(|pos| surface.world_to_uv(pos))
        });
        let surface_normal = server_ctx.hover_surface_normal.or_else(|| {
            server_ctx
                .hover_surface
                .as_ref()
                .map(|surface| surface.plane.normal)
        });
        let camera_scale = RUSTERIX
            .read()
            .ok()
            .map(|rusterix| rusterix.client.camera_d3.scale());
        IsoPaintPoint::new([coord.x, coord.y], world, owner)
            .with_surface_uv(surface_uv)
            .with_surface_normal(surface_normal)
            .with_camera_scale(camera_scale)
    }

    fn request_paint_redraw(ctx: &mut TheContext) {
        ctx.ui.redraw_all = true;
    }

    fn reset_stroke(&mut self) {
        self.painting = false;
        self.active_stroke = None;
        self.last_stamp_screen = None;
        self.stamp_clip_owner = None;
        self.stroke_before = None;
        self.stroke_changed = false;
    }
}

impl Tool for IsoPaintTool {
    fn new() -> Self
    where
        Self: Sized,
    {
        Self {
            id: TheId::named("Iso Paint Tool"),
            painting: false,
            previous_dock: None,
            active_stroke: None,
            last_stamp_screen: None,
            stamp_clip_owner: None,
            stroke_before: None,
            stroke_changed: false,
        }
    }

    fn id(&self) -> TheId {
        self.id.clone()
    }

    fn info(&self) -> String {
        fl!("tool_iso_paint")
    }

    fn icon_name(&self) -> String {
        "paint-brush".to_string()
    }

    fn accel(&self) -> Option<char> {
        Some('I')
    }

    fn help_url(&self) -> Option<String> {
        Some("docs/creator/tools/iso-paint".to_string())
    }

    fn tool_event(
        &mut self,
        tool_event: ToolEvent,
        ui: &mut TheUI,
        ctx: &mut TheContext,
        project: &mut Project,
        server_ctx: &mut ServerContext,
    ) -> bool {
        match tool_event {
            Activate => {
                self.reset_stroke();
                server_ctx.curr_map_tool_type = MapToolType::IsoPaint;
                server_ctx.editor_view_mode = EditorViewMode::Iso;
                server_ctx.geometry_edit_mode = GeometryEditMode::Geometry;
                server_ctx.hover_cursor = None;
                server_ctx.iso_paint_hover_screen = None;

                let neutral_material = Self::neutral_material_palette(project);
                if let Some(region) = project.get_region_mut(&server_ctx.curr_region) {
                    region.map.camera = MapCamera::ThreeDIso;
                    region.map.clear_selection();
                    region.map.clear_temp();
                    Self::ensure_initial_material_settings(region, neutral_material);
                    if matches!(region.iso_paint.active_brush.as_str(), "material" | "brick")
                        && region.iso_paint.active_size <= 1.001
                    {
                        region.iso_paint.active_size = 8.0;
                    }
                }

                let current_dock = DOCKMANAGER.read().unwrap().dock.clone();
                if current_dock != "Iso Paint" {
                    self.previous_dock = if current_dock.is_empty() {
                        None
                    } else {
                        Some(current_dock)
                    };
                }
                DOCKMANAGER.write().unwrap().set_dock(
                    "Iso Paint".into(),
                    ui,
                    ctx,
                    project,
                    server_ctx,
                );

                ctx.ui.send(TheEvent::SetStatusText(
                    TheId::empty(),
                    fl!("status_iso_paint_active"),
                ));
                RUSTERIX.write().unwrap().set_overlay_dirty();
                ctx.ui.redraw_all = true;
                true
            }
            DeActivate => {
                self.reset_stroke();
                server_ctx.curr_map_tool_type = MapToolType::General;
                server_ctx.hover_cursor = None;
                server_ctx.hover_cursor_3d = None;
                server_ctx.iso_paint_hover_screen = None;
                if DOCKMANAGER.read().unwrap().dock == "Iso Paint"
                    && let Some(prev) = self.previous_dock.take()
                {
                    DOCKMANAGER
                        .write()
                        .unwrap()
                        .set_dock(prev, ui, ctx, project, server_ctx);
                }
                true
            }
            _ => false,
        }
    }

    fn map_event(
        &mut self,
        map_event: MapEvent,
        _ui: &mut TheUI,
        ctx: &mut TheContext,
        _map: &mut Map,
        server_ctx: &mut ServerContext,
    ) -> Option<ProjectUndoAtom> {
        match map_event {
            MapClicked(_) => {
                self.painting = true;
                ctx.ui.send(TheEvent::SetStatusText(
                    TheId::empty(),
                    Self::hit_status(server_ctx),
                ));
            }
            MapDragged(_) => {
                if self.painting {
                    ctx.ui.send(TheEvent::SetStatusText(
                        TheId::empty(),
                        Self::hit_status(server_ctx),
                    ));
                }
            }
            MapHover(_) => {
                if !self.painting {
                    ctx.ui.send(TheEvent::SetStatusText(
                        TheId::empty(),
                        Self::hit_status(server_ctx),
                    ));
                }
            }
            MapUp(_) => {
                self.painting = false;
                server_ctx.iso_paint_hover_screen = None;
                ctx.ui.send(TheEvent::SetStatusText(
                    TheId::empty(),
                    fl!("status_iso_paint_active"),
                ));
            }
            MapEscape => {
                self.painting = false;
                server_ctx.iso_paint_hover_screen = None;
                ctx.ui.send(TheEvent::SetStatusText(
                    TheId::empty(),
                    fl!("status_iso_paint_active"),
                ));
            }
            _ => {}
        }

        None
    }

    fn region_map_event(
        &mut self,
        map_event: MapEvent,
        _ui: &mut TheUI,
        ctx: &mut TheContext,
        region: &mut Region,
        server_ctx: &mut ServerContext,
    ) -> Option<ProjectUndoAtom> {
        match map_event {
            MapClicked(coord) => {
                server_ctx.iso_paint_hover_screen = Some(coord);
                Self::sync_live_paint_settings(_ui, region);
                self.painting = true;
                self.stroke_before = Some(region.clone());
                if Self::is_stamp_mode(&region.iso_paint) {
                    let point = Self::paint_point(coord, server_ctx);
                    self.stamp_clip_owner = Self::stamp_clip_owner(&region.iso_paint, &point);
                    let clip_owner = self.stamp_clip_owner.clone();
                    let changed = Self::apply_stamp_at(region, point, clip_owner.as_ref());
                    self.active_stroke = None;
                    self.last_stamp_screen = Some([coord.x, coord.y]);
                    self.stroke_changed = changed;
                    Self::request_paint_redraw(ctx);
                    ctx.ui.send(TheEvent::SetStatusText(
                        TheId::empty(),
                        format!(
                            "{} {} stamp",
                            Self::hit_status(server_ctx),
                            Self::stamp_label(&region.iso_paint)
                        ),
                    ));
                    return None;
                }
                let stroke_id = region
                    .iso_paint
                    .begin_stroke(Self::paint_point(coord, server_ctx));
                let (stroke_opacity, stroke_material_mode) = region
                    .iso_paint
                    .chunks
                    .values()
                    .flat_map(|chunk| chunk.strokes.iter())
                    .find(|stroke| stroke.id == stroke_id)
                    .map(|stroke| (stroke.opacity, stroke.material_mode.clone()))
                    .unwrap_or((
                        region.iso_paint.active_opacity,
                        region.iso_paint.active_material_mode.clone(),
                    ));
                self.active_stroke = Some(stroke_id);
                self.stroke_changed = true;
                Self::request_paint_redraw(ctx);
                ctx.ui.send(TheEvent::SetStatusText(
                    TheId::empty(),
                    format!(
                        "{} opacity {:.3} mode {}",
                        Self::hit_status(server_ctx),
                        stroke_opacity,
                        stroke_material_mode
                    ),
                ));
            }
            MapDragged(coord) => {
                server_ctx.iso_paint_hover_screen = Some(coord);
                if self.painting
                    && Self::is_stamp_mode(&region.iso_paint)
                    && Self::should_place_stamp(
                        self.last_stamp_screen,
                        coord,
                        region.iso_paint.active_size,
                        region.iso_paint.active_stamp_density,
                    )
                {
                    let point = Self::paint_point(coord, server_ctx);
                    let changed =
                        Self::apply_stamp_at(region, point, self.stamp_clip_owner.as_ref());
                    if changed {
                        self.last_stamp_screen = Some([coord.x, coord.y]);
                    }
                    self.stroke_changed |= changed;
                    Self::request_paint_redraw(ctx);
                    return None;
                }
                if self.painting
                    && let Some(stroke_id) = self.active_stroke
                    && region
                        .iso_paint
                        .append_point(stroke_id, Self::paint_point(coord, server_ctx))
                {
                    self.stroke_changed = true;
                    Self::request_paint_redraw(ctx);
                }
            }
            MapHover(coord) => {
                server_ctx.iso_paint_hover_screen = Some(coord);
                Self::request_paint_redraw(ctx);
            }
            MapUp(coord) => {
                server_ctx.iso_paint_hover_screen = Some(coord);
                if self.painting
                    && Self::is_stamp_mode(&region.iso_paint)
                    && Self::should_place_stamp(
                        self.last_stamp_screen,
                        coord,
                        region.iso_paint.active_size,
                        region.iso_paint.active_stamp_density,
                    )
                {
                    let point = Self::paint_point(coord, server_ctx);
                    let changed =
                        Self::apply_stamp_at(region, point, self.stamp_clip_owner.as_ref());
                    self.stroke_changed |= changed;
                } else if self.painting
                    && let Some(stroke_id) = self.active_stroke
                    && region
                        .iso_paint
                        .append_point(stroke_id, Self::paint_point(coord, server_ctx))
                {
                    self.stroke_changed = true;
                }

                let undo_atom = if self.stroke_changed {
                    self.stroke_before.take().map(|old_region| {
                        ProjectUndoAtom::RegionEdit(
                            ProjectContext::Region(region.id),
                            Box::new(old_region),
                            Box::new(region.clone()),
                        )
                    })
                } else {
                    None
                };

                self.reset_stroke();
                Self::request_paint_redraw(ctx);
                ctx.ui.send(TheEvent::SetStatusText(
                    TheId::empty(),
                    fl!("status_iso_paint_active"),
                ));
                return undo_atom;
            }
            MapEscape => {
                server_ctx.iso_paint_hover_screen = None;
                if let Some(old_region) = self.stroke_before.take() {
                    *region = old_region;
                }
                self.reset_stroke();
                Self::request_paint_redraw(ctx);
                ctx.ui.send(TheEvent::SetStatusText(
                    TheId::empty(),
                    fl!("status_iso_paint_active"),
                ));
            }
            _ => {}
        }

        None
    }
}
