use crate::prelude::*;

pub struct CopyTileID {
    id: TheId,
    nodeui: TheNodeUI,
}

impl Action for CopyTileID {
    fn new() -> Self
    where
        Self: Sized,
    {
        let mut nodeui: TheNodeUI = TheNodeUI::default();
        let item = TheNodeUIItem::Markdown(
            "desc".into(),
            "Copies the ID of the tile to the clipboard for later use in the code editor.".into(),
        );
        nodeui.add_item(item);

        Self {
            id: TheId::named("Copy Tile ID"),
            nodeui,
        }
    }

    fn id(&self) -> TheId {
        self.id.clone()
    }

    fn info(&self) -> &'static str {
        "Copies the ID of the selected tile to the clipboard."
    }

    fn role(&self) -> ActionRole {
        ActionRole::UI
    }

    fn accel(&self) -> Option<TheAccelerator> {
        None
    }

    fn is_applicable(&self, _map: &Map, _ctx: &mut TheContext, server_ctx: &ServerContext) -> bool {
        server_ctx.curr_map_tool_helper == MapToolHelper::TilePicker
            && server_ctx.curr_tile_id.is_some()
    }

    fn apply(
        &self,
        _map: &mut Map,
        _ui: &mut TheUI,
        ctx: &mut TheContext,
        server_ctx: &mut ServerContext,
    ) -> Option<RegionUndoAtom> {
        server_ctx.no_rect_geo_on_map = !server_ctx.no_rect_geo_on_map;

        if let Some(tile_id) = server_ctx.curr_tile_id {
            let txt = format!("\"{tile_id}\"");
            ctx.ui.clipboard = Some(TheValue::Text(txt.clone()));
            let mut clipboard = arboard::Clipboard::new().unwrap();
            clipboard.set_text(txt.clone()).unwrap();
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
