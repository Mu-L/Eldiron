use crate::prelude::*;

pub struct Split {
    id: TheId,
    nodeui: TheNodeUI,
}

impl Action for Split {
    fn new() -> Self
    where
        Self: Sized,
    {
        let mut nodeui: TheNodeUI = TheNodeUI::default();

        let item = TheNodeUIItem::Markdown(
            "desc".into(),
            "Split the selected linedef(s) by adding a middle point. Thew new point gets added to all sectors the linedef is part of.".into(),
        );
        nodeui.add_item(item);

        Self {
            id: TheId::named("Split"),
            nodeui,
        }
    }

    fn id(&self) -> TheId {
        self.id.clone()
    }

    fn info(&self) -> &'static str {
        "Split a linedef."
    }

    fn role(&self) -> ActionRole {
        ActionRole::Geometry
    }

    fn accel(&self) -> Option<TheAccelerator> {
        None
    }

    fn is_applicable(&self, map: &Map, _ctx: &mut TheContext, _server_ctx: &ServerContext) -> bool {
        !map.selected_linedefs.is_empty()
    }

    fn apply(
        &self,
        map: &mut Map,
        _ui: &mut TheUI,
        _ctx: &mut TheContext,
        _server_ctx: &mut ServerContext,
    ) -> Option<RegionUndoAtom> {
        let mut changed = false;
        let prev = map.clone();

        for linedef_id in &map.selected_linedefs.clone() {
            map.add_midpoint(*linedef_id);
            changed = true;
        }

        if changed {
            Some(RegionUndoAtom::MapEdit(
                Box::new(prev),
                Box::new(map.clone()),
            ))
        } else {
            None
        }
    }

    fn params(&self) -> TheNodeUI {
        self.nodeui.clone()
    }

    fn handle_event(&mut self, event: &TheEvent) -> bool {
        self.nodeui.handle_event(event)
    }
}
