use crate::prelude::*;

pub struct ActionList {
    pub actions: Vec<Box<dyn Action>>,
}

impl Default for ActionList {
    fn default() -> Self {
        Self::new()
    }
}

impl ActionList {
    pub fn new() -> Self {
        let actions: Vec<Box<dyn Action>> = vec![
            Box::new(crate::tools::apply_shader::ApplyShader::new()),
            Box::new(crate::tools::add_shader_library::AddShaderLibrary::new()),
            Box::new(crate::tools::apply_tile::ApplyTile::new()),
            Box::new(crate::tools::clear_shader::ClearShader::new()),
            Box::new(crate::tools::clear_tile::ClearTile::new()),
            Box::new(crate::tools::edit_linedef::EditLinedef::new()),
            Box::new(crate::tools::edit_sector::EditSector::new()),
            Box::new(crate::tools::edit_vertex::EditVertex::new()),
            Box::new(crate::tools::extrude::Extrude::new()),
            Box::new(crate::tools::load_shader::LoadShader::new()),
            Box::new(crate::tools::new_shader::NewShader::new()),
            Box::new(crate::tools::recess::Recess::new()),
            Box::new(crate::tools::relief::Relief::new()),
            Box::new(crate::tools::set_editing_surface::SetEditingSurface::new()),
            Box::new(crate::tools::split::Split::new()),
            Box::new(crate::tools::toggle_rect_geo::ToggleRectGeo::new()),
        ];
        Self { actions }
    }

    /// Returns an action by the given id.
    pub fn get_action_by_id(&self, id: Uuid) -> Option<&Box<dyn Action>> {
        for action in &self.actions {
            if action.id().uuid == id {
                return Some(action);
            }
        }
        None
    }

    /// Returns an mutable action by the given id.
    pub fn get_action_by_id_mut(&mut self, id: Uuid) -> Option<&mut Box<dyn Action>> {
        for action in &mut self.actions {
            if action.id().uuid == id {
                return Some(action);
            }
        }
        None
    }
}
