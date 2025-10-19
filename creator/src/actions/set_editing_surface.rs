use crate::prelude::*;

pub struct SetEditingSurface {
    id: TheId,
    nodeui: TheNodeUI,
}

impl Action for SetEditingSurface {
    fn new() -> Self
    where
        Self: Sized,
    {
        let mut nodeui: TheNodeUI = TheNodeUI::default();
        let item = TheNodeUIItem::Markdown(
            "desc".into(),
            "Make the selected surface the active 2D Profil for editing. Eldiron will switch to the 2D view and, if no profile exists yet, create one for this surface. To return to the Region map, click **Region** in the toolbar.".into(),
        );
        nodeui.add_item(item);

        Self {
            id: TheId::named("Set Editing Surface"),
            nodeui,
        }
    }

    fn id(&self) -> TheId {
        self.id.clone()
    }

    fn info(&self) -> &'static str {
        "Set the 2D editing surface."
    }

    fn role(&self) -> ActionRole {
        ActionRole::UI
    }

    fn accel(&self) -> Option<TheAccelerator> {
        Some(TheAccelerator::new(TheAcceleratorKey::ALT, 'u'))
    }

    fn is_applicable(&self, map: &Map, _ctx: &mut TheContext, server_ctx: &ServerContext) -> bool {
        map.selected_sectors.len() == 1 && server_ctx.editor_view_mode != EditorViewMode::D2
    }

    fn apply(
        &self,
        map: &mut Map,
        ui: &mut TheUI,
        ctx: &mut TheContext,
        server_ctx: &mut ServerContext,
    ) -> Option<RegionUndoAtom> {
        if let Some(sector_id) = map.selected_sectors.first().cloned() {
            let mut profile_to_add = None;

            if let Some(surface) = map.get_surface_for_sector_id_mut(sector_id) {
                // If there is no profile yet for the surface we add one
                if surface.profile.is_none() {
                    let profile = Map::default();
                    surface.profile = Some(profile.id);
                    profile_to_add = Some(profile);
                }

                let mut surface = surface.clone();
                if let Some(sector) = map.find_sector(sector_id) {
                    if let Some(vertices) = sector.vertices_world(map) {
                        surface.world_vertices = vertices;
                    }
                }

                server_ctx.editing_surface = Some(surface.clone());

                if let Some(widget) = ui.get_group_button("Editor View Switch") {
                    server_ctx.editor_view_mode = EditorViewMode::D2;
                    widget.set_index(0);
                }
            }

            if let Some(profile_to_add) = profile_to_add {
                map.profiles.insert(profile_to_add.id, profile_to_add);
            }

            ctx.ui.send(TheEvent::Custom(
                TheId::named("Update Action List"),
                TheValue::Empty,
            ));
        }

        None
    }

    fn params(&self) -> TheNodeUI {
        self.nodeui.clone()
    }

    fn handle_event(&mut self, event: &TheEvent) -> bool {
        self.nodeui.handle_event(event)
    }
}
