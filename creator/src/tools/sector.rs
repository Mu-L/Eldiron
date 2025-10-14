use crate::editor::NODEEDITOR;
use crate::hud::{Hud, HudMode};
use crate::prelude::*;
use MapEvent::*;
use ToolEvent::*;
use rusterix::{Assets, GeometrySource, PixelSource, Value};
use vek::Vec2;

pub struct SectorTool {
    id: TheId,
    click_pos: Vec2<f32>,
    rectangle_undo_map: Map,
    click_selected: bool,
    drag_changed: bool,
    was_clicked: bool,

    hud: Hud,
}

impl Tool for SectorTool {
    fn new() -> Self
    where
        Self: Sized,
    {
        Self {
            id: TheId::named("Sector Tool"),
            click_pos: Vec2::zero(),
            click_selected: false,
            drag_changed: false,
            rectangle_undo_map: Map::default(),
            was_clicked: false,

            hud: Hud::new(HudMode::Sector),
        }
    }

    fn id(&self) -> TheId {
        self.id.clone()
    }
    fn info(&self) -> String {
        str!("Sector Tool (E).")
    }
    fn icon_name(&self) -> String {
        str!("polygon")
    }
    fn accel(&self) -> Option<char> {
        Some('E')
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
                // Display the tile edit panel.
                ctx.ui
                    .send(TheEvent::SetStackIndex(TheId::named("Main Stack"), 0));

                if let Some(layout) = ui.get_sharedhlayout("Shared Panel Layout") {
                    layout.set_mode(TheSharedHLayoutMode::Right);
                    ctx.ui.relayout = true;
                }

                server_ctx.curr_map_tool_type = MapToolType::Sector;

                if let Some(map) = project.get_map_mut(server_ctx) {
                    map.selected_vertices.clear();
                    map.selected_linedefs.clear();
                }

                ctx.ui.send(TheEvent::Custom(
                    TheId::named("Map Selection Changed"),
                    TheValue::Empty,
                ));

                self.activate_map_tool_helper(ui, ctx, project, server_ctx);

                return true;
            }
            _ => {
                server_ctx.curr_map_tool_type = MapToolType::General;
            }
        };
        false
    }

    fn map_event(
        &mut self,
        map_event: MapEvent,
        ui: &mut TheUI,
        ctx: &mut TheContext,
        map: &mut Map,
        server_ctx: &mut ServerContext,
    ) -> Option<RegionUndoAtom> {
        let mut undo_atom: Option<RegionUndoAtom> = None;

        match map_event {
            MapKey(c) => {
                match c {
                    '1'..='9' => map.subdivisions = (c as u8 - b'0') as f32,
                    '0' => map.subdivisions = 10.0,
                    _ => {}
                }
                crate::editor::RUSTERIX.write().unwrap().set_dirty();
            }
            MapClicked(coord) => {
                if self.hud.clicked(coord.x, coord.y, map, ui, ctx, server_ctx) {
                    self.was_clicked = false;
                    crate::editor::RUSTERIX.write().unwrap().set_dirty();
                    return None;
                }
                self.was_clicked = true;

                self.click_selected = false;
                if server_ctx.hover.2.is_some() {
                    map.selected_entity_item = None;

                    if ui.shift {
                        // Add
                        if let Some(s) = server_ctx.hover.2 {
                            if !map.selected_sectors.contains(&s) {
                                map.selected_sectors.push(s);
                            }
                            self.click_selected = true;
                        }
                    } else if ui.alt {
                        // Subtract
                        if let Some(v) = server_ctx.hover.2 {
                            map.selected_sectors.retain(|&selected| selected != v);
                        }
                    } else {
                        // Replace
                        if let Some(v) = server_ctx.hover.2 {
                            map.selected_sectors = vec![v];
                        } else {
                            map.selected_sectors.clear();
                        }
                        self.click_selected = true;
                    }

                    //  Make the selected sector the editing surface if in 3D
                    if server_ctx.editor_view_mode != EditorViewMode::D2 {
                        if let Some(s) = server_ctx.hover.2 {
                            if map.selected_sectors.contains(&s) {
                                match server_ctx.hitinfo.geometry_source {
                                    rusterix::GeometrySource::Sector(id) => {
                                        if let Some(surface) = map.get_surface_for_sector_id(id) {
                                            let mut surface = surface.clone();
                                            if let Some(sector) = map.find_sector(id) {
                                                if let Some(vertices) = sector.vertices_world(map) {
                                                    surface.world_vertices = vertices;
                                                }
                                            }
                                            server_ctx.editing_surface = Some(surface);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                } else if server_ctx.editor_view_mode != EditorViewMode::D2 {
                    server_ctx.editing_surface = None;
                }

                self.click_pos = Vec2::new(coord.x as f32, coord.y as f32);
                self.rectangle_undo_map = map.clone();
            }
            MapDragged(coord) => {
                if self.hud.dragged(coord.x, coord.y, map, ui, ctx, server_ctx) {
                    crate::editor::RUSTERIX.write().unwrap().set_dirty();
                    return None;
                }

                if self.click_selected {
                    // Dragging selected sectors
                    if let Some(render_view) = ui.get_render_view("PolyView") {
                        let dim = *render_view.dim();
                        let click_pos = server_ctx.local_to_map_grid(
                            Vec2::new(dim.width as f32, dim.height as f32),
                            self.click_pos,
                            map,
                            map.subdivisions,
                        );
                        let drag_pos = server_ctx.local_to_map_grid(
                            Vec2::new(dim.width as f32, dim.height as f32),
                            Vec2::new(coord.x as f32, coord.y as f32),
                            map,
                            map.subdivisions,
                        );

                        let mut selected_vertices = vec![];

                        let drag_delta = click_pos - drag_pos;

                        for sector_id in self.rectangle_undo_map.selected_sectors.iter() {
                            if let Some(sector) = self.rectangle_undo_map.find_sector(*sector_id) {
                                for line_id in &sector.linedefs {
                                    if let Some(line) =
                                        self.rectangle_undo_map.find_linedef(*line_id)
                                    {
                                        selected_vertices.push(line.start_vertex);
                                        selected_vertices.push(line.end_vertex);
                                    }
                                }
                            }
                        }

                        for vertex_id in selected_vertices.iter() {
                            if let Some(original_vertex) =
                                self.rectangle_undo_map.find_vertex_mut(*vertex_id)
                            {
                                let new_pos = Vec2::new(
                                    original_vertex.x - drag_delta.x,
                                    original_vertex.y - drag_delta.y,
                                );
                                map.update_vertex(*vertex_id, new_pos);
                            }
                        }
                        server_ctx.hover_cursor = Some(drag_pos);

                        if drag_delta.x != 0.0 || drag_delta.y != 0.0 {
                            self.drag_changed = true;
                        }
                    }
                } else if let Some(render_view) = ui.get_render_view("PolyView") {
                    if !self.was_clicked {
                        return None;
                    }

                    let dim = *render_view.dim();
                    let click_pos = server_ctx.local_to_map_grid(
                        Vec2::new(dim.width as f32, dim.height as f32),
                        self.click_pos,
                        map,
                        map.subdivisions,
                    );
                    let drag_pos = server_ctx.local_to_map_grid(
                        Vec2::new(dim.width as f32, dim.height as f32),
                        Vec2::new(coord.x as f32, coord.y as f32),
                        map,
                        map.subdivisions,
                    );

                    let top_left =
                        Vec2::new(click_pos.x.min(drag_pos.x), click_pos.y.min(drag_pos.y));
                    let bottom_right =
                        Vec2::new(click_pos.x.max(drag_pos.x), click_pos.y.max(drag_pos.y));

                    let mut selection =
                        server_ctx.geometry_in_rectangle(top_left, bottom_right, map);

                    selection.0 = vec![];
                    selection.1 = vec![];

                    *map = self.rectangle_undo_map.clone();
                    map.curr_rectangle = Some((click_pos, drag_pos));

                    if ui.shift {
                        // Add
                        map.add_to_selection(selection.0, selection.1, selection.2);
                    } else if ui.alt {
                        // Remove
                        map.remove_from_selection(selection.0, selection.1, selection.2);
                    } else {
                        // Replace
                        map.selected_sectors = selection.2;
                    }
                }
                crate::editor::RUSTERIX.write().unwrap().set_dirty();
            }
            MapUp(_) => {
                if self.click_selected {
                    if self.drag_changed {
                        undo_atom = Some(RegionUndoAtom::MapEdit(
                            Box::new(self.rectangle_undo_map.clone()),
                            Box::new(map.clone()),
                        ));
                        ctx.ui.send(TheEvent::Custom(
                            TheId::named("Map Selection Changed"),
                            TheValue::Empty,
                        ));
                    }
                } else if map.curr_rectangle.is_some() {
                    map.curr_rectangle = None;
                }
                self.drag_changed = false;
                self.click_selected = false;
            }
            MapHover(coord) => {
                if self.hud.hovered(coord.x, coord.y, map, ui, ctx, server_ctx) {
                    crate::editor::RUSTERIX.write().unwrap().set_dirty();
                    return None;
                }

                if let Some(render_view) = ui.get_render_view("PolyView") {
                    if server_ctx.editor_view_mode == EditorViewMode::D2 {
                        let dim = *render_view.dim();
                        let h = server_ctx.geometry_at(
                            Vec2::new(dim.width as f32, dim.height as f32),
                            Vec2::new(coord.x as f32, coord.y as f32),
                            map,
                        );
                        server_ctx.hover.2 = h.2;

                        let cp = server_ctx.local_to_map_grid(
                            Vec2::new(dim.width as f32, dim.height as f32),
                            Vec2::new(coord.x as f32, coord.y as f32),
                            map,
                            map.subdivisions,
                        );
                        ctx.ui.send(TheEvent::Custom(
                            TheId::named("Cursor Pos Changed"),
                            TheValue::Float2(cp),
                        ));
                        server_ctx.hover_cursor = Some(cp);
                    } else {
                        if server_ctx.hitinfo.has_hit() {
                            match server_ctx.hitinfo.geometry_source {
                                GeometrySource::Sector(id) => {
                                    server_ctx.hover = (None, None, Some(id));
                                }
                                _ => {
                                    server_ctx.hover = (None, None, None);
                                }
                            }
                        } else {
                            server_ctx.hover = (None, None, None);
                        }

                        if let Some(cp) = server_ctx.hover_cursor {
                            ctx.ui.send(TheEvent::Custom(
                                TheId::named("Cursor Pos Changed"),
                                TheValue::Float2(cp),
                            ));
                        }
                    }
                }
            }
            MapDelete => {
                if !map.selected_sectors.is_empty() {
                    let prev = map.clone();
                    let sectors = map.selected_sectors.clone();

                    #[allow(clippy::useless_vec)]
                    map.delete_elements(&vec![], &vec![], &sectors);
                    map.selected_sectors.clear();

                    undo_atom = Some(RegionUndoAtom::MapEdit(
                        Box::new(prev),
                        Box::new(map.clone()),
                    ));
                    ctx.ui.send(TheEvent::Custom(
                        TheId::named("Map Selection Changed"),
                        TheValue::Empty,
                    ));
                    crate::editor::RUSTERIX.write().unwrap().set_dirty();
                }
            }
            MapEscape => {
                // Hover is empty, check if we need to clear selection
                map.selected_sectors.clear();
                crate::editor::RUSTERIX.write().unwrap().set_dirty();
            }
        }
        undo_atom
    }

    fn draw_hud(
        &mut self,
        buffer: &mut TheRGBABuffer,
        map: &mut Map,
        ctx: &mut TheContext,
        server_ctx: &mut ServerContext,
        assets: &Assets,
    ) {
        let id = if !map.selected_sectors.is_empty() {
            Some(map.selected_sectors[0])
        } else {
            None
        };
        self.hud.draw(buffer, map, ctx, server_ctx, id, assets);
    }

    fn handle_event(
        &mut self,
        event: &TheEvent,
        _ui: &mut TheUI,
        ctx: &mut TheContext,
        project: &mut Project,
        server_ctx: &mut ServerContext,
    ) -> bool {
        let redraw = false;
        #[allow(clippy::single_match)]
        match event {
            TheEvent::StateChanged(id, state) => {
                #[allow(clippy::collapsible_if)]
                if id.name == "Apply Map Properties" && *state == TheWidgetState::Clicked {
                    let mut source: Option<Value> = None;

                    if server_ctx.curr_map_tool_helper == MapToolHelper::TilePicker {
                        if let Some(id) = server_ctx.curr_tile_id {
                            source = Some(Value::Source(PixelSource::TileId(id)));
                        }
                    }
                    /*else if server_ctx.curr_map_tool_helper == MapToolHelper::MaterialPicker {
                        if let Some(id) = server_ctx.curr_material_id {
                            source = Some(Value::Source(PixelSource::MaterialId(id)));
                        }
                    }*/
                    else if server_ctx.curr_map_tool_helper == MapToolHelper::NodeEditor {
                        let node_editor = NODEEDITOR.read().unwrap();
                        if !node_editor.graph.nodes.is_empty() {
                            source = Some(Value::Source(PixelSource::ShapeFXGraphId(
                                node_editor.graph.id,
                            )));
                        }
                    }

                    if let Some(source) = source {
                        if let Some(map) = project.get_map_mut(server_ctx) {
                            let prev = map.clone();
                            let context = NODEEDITOR.read().unwrap().context;
                            for sector_id in &map.selected_sectors.clone() {
                                if let Some(sector) = map.find_sector_mut(*sector_id) {
                                    if context == NodeContext::Screen
                                        && server_ctx.curr_map_tool_helper
                                            == MapToolHelper::NodeEditor
                                    {
                                        sector.properties.set("screen_graph", source.clone());
                                    } else if context == NodeContext::Region
                                        && server_ctx.curr_map_tool_helper
                                            == MapToolHelper::NodeEditor
                                    {
                                        sector.properties.set("region_graph", source.clone());
                                    } else if context == NodeContext::Shape {
                                        sector.properties.set("shape_graph", source.clone());
                                    } else if self.hud.selected_icon_index == 0
                                        || context == NodeContext::Material
                                    {
                                        sector.properties.set("source", source.clone());
                                    } else if self.hud.selected_icon_index == 1 {
                                        sector.properties.set("ceiling_source", source.clone());
                                    }
                                }
                            }

                            // Force update
                            if server_ctx.curr_map_tool_helper == MapToolHelper::NodeEditor {
                                NODEEDITOR.read().unwrap().force_update(ctx, map);
                            }

                            let undo_atom =
                                RegionUndoAtom::MapEdit(Box::new(prev), Box::new(map.clone()));

                            if server_ctx.get_map_context() == MapContext::Region {
                                crate::editor::UNDOMANAGER.write().unwrap().add_region_undo(
                                    &server_ctx.curr_region,
                                    undo_atom,
                                    ctx,
                                );
                            } else if server_ctx.get_map_context() == MapContext::Model {
                                /*
                                if let Some(material_undo_atom) = undo_atom.to_material_atom() {
                                    crate::editor::UNDOMANAGER
                                        .write()
                                        .unwrap()
                                        .add_material_undo(material_undo_atom, ctx);
                                    ctx.ui.send(TheEvent::Custom(
                                        TheId::named("Update Materialpicker"),
                                        TheValue::Empty,
                                    ));
                                }*/
                            }

                            if server_ctx.get_map_context() == MapContext::Region {
                                ctx.ui.send(TheEvent::Custom(
                                    TheId::named("Render SceneManager Map"),
                                    TheValue::Empty,
                                ));
                            }

                            crate::editor::RUSTERIX.write().unwrap().set_dirty();
                        }
                    }
                } else if id.name == "Remove Map Properties" && *state == TheWidgetState::Clicked {
                    if let Some(map) = project.get_map_mut(server_ctx) {
                        let prev = map.clone();
                        let context = NODEEDITOR.read().unwrap().context;
                        for sector_id in map.selected_sectors.clone() {
                            if let Some(sector) = map.find_sector_mut(sector_id) {
                                if context == NodeContext::Region
                                    && server_ctx.curr_map_tool_helper == MapToolHelper::NodeEditor
                                {
                                    sector.properties.remove("region_graph");
                                } else if context == NodeContext::Screen
                                    && server_ctx.curr_map_tool_helper == MapToolHelper::NodeEditor
                                {
                                    sector.properties.remove("screen_graph");
                                } else if self.hud.selected_icon_index == 0 {
                                    if sector.properties.contains("floor_light") {
                                        sector.properties.remove("floor_light");
                                    } else {
                                        sector
                                            .properties
                                            .set("source", Value::Source(PixelSource::Off));
                                    }
                                } else if self.hud.selected_icon_index == 1 {
                                    if sector.properties.contains("ceiling_light") {
                                        sector.properties.remove("ceiling_light");
                                    } else {
                                        sector
                                            .properties
                                            .set("ceiling_source", Value::Source(PixelSource::Off));
                                    }
                                }
                            }
                        }

                        // Force node update
                        if server_ctx.curr_map_tool_helper == MapToolHelper::NodeEditor {
                            NODEEDITOR.read().unwrap().force_update(ctx, map);
                        }

                        let undo_atom =
                            RegionUndoAtom::MapEdit(Box::new(prev), Box::new(map.clone()));

                        if server_ctx.get_map_context() == MapContext::Region {
                            crate::editor::UNDOMANAGER.write().unwrap().add_region_undo(
                                &server_ctx.curr_region,
                                undo_atom,
                                ctx,
                            );
                        } else if server_ctx.get_map_context() == MapContext::Model {
                            /*
                            if let Some(material_undo_atom) = undo_atom.to_material_atom() {
                                crate::editor::UNDOMANAGER
                                    .write()
                                    .unwrap()
                                    .add_material_undo(material_undo_atom, ctx);
                                ctx.ui.send(TheEvent::Custom(
                                    TheId::named("Update Materialpicker"),
                                    TheValue::Empty,
                                ));
                            }*/
                        }

                        if server_ctx.get_map_context() == MapContext::Region {
                            ctx.ui.send(TheEvent::Custom(
                                TheId::named("Render SceneManager Map"),
                                TheValue::Empty,
                            ));
                        }

                        crate::editor::RUSTERIX.write().unwrap().set_dirty();
                    }
                }
            }
            _ => {}
        }
        redraw
    }
}
