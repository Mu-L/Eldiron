use crate::editor::MODELFXEDITOR;
use crate::prelude::*;

pub struct ModelNodeEditTool {
    id: TheId,
}

impl Tool for ModelNodeEditTool {
    fn new() -> Self
    where
        Self: Sized,
    {
        Self {
            id: TheId::named("Edit Tool (E)."),
        }
    }

    fn id(&self) -> TheId {
        self.id.clone()
    }
    fn info(&self) -> String {
        str!("Edit Tool (E). Edit the nodes of the Geometry.")
    }
    fn icon_name(&self) -> String {
        str!("graph")
    }
    fn accel(&self) -> Option<char> {
        Some('e')
    }

    fn tool_event(
        &mut self,
        tool_event: ToolEvent,
        _tool_context: ToolContext,
        ui: &mut TheUI,
        ctx: &mut TheContext,
        _project: &mut Project,
        _server: &mut Server,
        _client: &mut Client,
        _server_ctx: &mut ServerContext,
    ) -> bool {
        if let ToolEvent::Activate = tool_event {
            MODELFXEDITOR.lock().unwrap().set_geometry_mode(false);

            if let Some(layout) = ui.get_sharedvlayout("Shared VLayout") {
                layout.set_mode(TheSharedVLayoutMode::Shared);
                layout.set_shared_ratio(0.42);
            }

            ctx.ui
                .send(TheEvent::SetStackIndex(TheId::named("Main Stack"), 6));

            if let Some(layout) = ui.get_hlayout("Model Tool Params") {
                layout.clear();

                let mut add_button = TheTraybarButton::new(TheId::named("Add To Models"));
                add_button.set_text(str!("Add To Models"));
                add_button.set_status_text("Adds the current model to the Model list.");

                add_button.set_context_menu(Some(TheContextMenu {
                    items: vec![TheContextMenuItem::new(
                        "Add".to_string(),
                        TheId::named("Add"),
                    )],
                    ..Default::default()
                }));

                layout.add_widget(Box::new(add_button));
                layout.set_reverse_index(Some(1));
            }
        } else if let ToolEvent::DeActivate = tool_event {
            if let Some(layout) = ui.get_hlayout("Material Tool Params") {
                layout.clear();
                layout.set_reverse_index(None);
            }
            if let Some(layout) = ui.get_sharedvlayout("Shared VLayout") {
                layout.set_mode(TheSharedVLayoutMode::Shared);
                layout.set_shared_ratio(crate::DEFAULT_VLAYOUT_RATIO);
            }
            MODELFXEDITOR.lock().unwrap().set_geometry_mode(true);
        }
        false
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_event(
        &mut self,
        event: &TheEvent,
        ui: &mut TheUI,
        ctx: &mut TheContext,
        project: &mut Project,
        _server: &mut Server,
        _client: &mut Client,
        server_ctx: &mut ServerContext,
    ) -> bool {
        let mut redraw = false;
        #[allow(clippy::single_match)]
        match event {
            TheEvent::Drop(coord, drop) => {
                if drop.id.name == "Model Item" {
                    if let Some(mut model) = project.models.get(&drop.id.uuid).cloned() {
                        if let Some(geo_obj_id) = server_ctx.curr_geo_object {
                            if let Some(region) = project.get_region_mut(&server_ctx.curr_region) {
                                if let Some(geo_obj) = region.geometry.get_mut(&geo_obj_id) {
                                    let node_offset = geo_obj.nodes.len() as u16;

                                    // Get the original position of node[0] in the dropped graph
                                    let original_pos = model.nodes[0].position;

                                    // Calculate the displacement for the new node[0] position
                                    let displacement =
                                        (coord + geo_obj.scroll_offset) - original_pos;

                                    // Adjust positions
                                    for node in &mut model.nodes {
                                        node.position += displacement;
                                    }

                                    geo_obj.nodes.extend(model.nodes.clone());

                                    for (
                                        src_node_idx,
                                        src_terminal,
                                        dest_node_idx,
                                        dest_terminal,
                                    ) in &model.connections
                                    {
                                        geo_obj.connections.push((
                                            src_node_idx + node_offset,
                                            *src_terminal,
                                            dest_node_idx + node_offset,
                                            *dest_terminal,
                                        ));
                                    }

                                    let node_canvas = geo_obj.to_canvas();
                                    ui.set_node_canvas("Model NodeCanvas", node_canvas);

                                    redraw = true
                                }
                            }
                        }
                    }
                }
            }
            TheEvent::ContextMenuSelected(id, _) => {
                if id.name == "Add To Models" {
                    let mut to_add = None;
                    if let Some(geo_obj_id) = server_ctx.curr_geo_object {
                        if let Some(region) = project.get_region_mut(&server_ctx.curr_region) {
                            if let Some(geo_obj) = region.geometry.get_mut(&geo_obj_id) {
                                if !geo_obj.nodes.is_empty() {
                                    to_add = Some(geo_obj.clone());
                                }
                            }
                        }
                    }
                    if let Some(mut geo_obj) = to_add {
                        geo_obj.id = Uuid::new_v4();
                        geo_obj.name = str!("New Model");

                        project.models.insert(geo_obj.id, geo_obj);

                        ctx.ui.send(TheEvent::Custom(
                            TheId::named("Update Model List"),
                            TheValue::Empty,
                        ));
                    }
                }
            }
            _ => {}
        }

        redraw
    }
}
