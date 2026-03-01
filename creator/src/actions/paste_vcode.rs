use crate::{editor::DOCKMANAGER, prelude::*};

pub struct PasteVCode {
    id: TheId,
    nodeui: TheNodeUI,
}

impl Action for PasteVCode {
    fn new() -> Self
    where
        Self: Sized,
    {
        let mut nodeui: TheNodeUI = TheNodeUI::default();
        let item = TheNodeUIItem::Markdown("desc".into(), fl!("action_paste_vcode_desc"));
        nodeui.add_item(item);

        Self {
            id: TheId::named(&fl!("action_paste_vcode")),
            nodeui,
        }
    }

    fn id(&self) -> TheId {
        self.id.clone()
    }

    fn info(&self) -> String {
        fl!("action_paste_vcode_desc")
    }

    fn role(&self) -> ActionRole {
        ActionRole::Dock
    }

    fn accel(&self) -> Option<TheAccelerator> {
        None
    }

    fn is_applicable(
        &self,
        _map: &Map,
        _ctx: &mut TheContext,
        _server_ctx: &ServerContext,
    ) -> bool {
        DOCKMANAGER.read().unwrap().dock == "Visual Code"
    }

    fn apply_project(
        &self,
        project: &mut Project,
        ui: &mut TheUI,
        ctx: &mut TheContext,
        server_ctx: &mut ServerContext,
    ) {
        let mut content: Option<String> = None;
        if let Some(TheValue::Text(text)) = &ctx.ui.clipboard {
            if !text.trim().is_empty() {
                content = Some(text.clone());
            }
        }
        if content.is_none()
            && let Ok(mut clipboard) = arboard::Clipboard::new()
            && let Ok(text) = clipboard.get_text()
            && !text.trim().is_empty()
        {
            content = Some(text);
        }

        if let Some(text) = content {
            DOCKMANAGER
                .write()
                .unwrap()
                .import(text.clone(), ui, ctx, project, server_ctx);
            ctx.ui.clipboard = Some(TheValue::Text(text));
            ctx.ui.clipboard_app_type = Some("text/plain".to_string());
        }
    }

    fn params(&self) -> TheNodeUI {
        self.nodeui.clone()
    }

    fn handle_event(
        &mut self,
        event: &TheEvent,
        _project: &mut Project,
        _ui: &mut TheUI,
        _ctx: &mut TheContext,
        _server_ctx: &mut ServerContext,
    ) -> bool {
        self.nodeui.handle_event(event)
    }
}
