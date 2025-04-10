use crate::prelude::*;
use crate::undo::material_undo::MaterialUndoAtom;
use crate::undo::screen_undo::ScreenUndoAtom;
use theframework::prelude::*;

#[allow(clippy::large_enum_variant)]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum RegionUndoAtom {
    // GeoFXObjectsDeletion(Vec<GeoFXObject>),
    // GeoFXObjectEdit(Uuid, Option<GeoFXObject>, Option<GeoFXObject>),
    // GeoFXAddNode(Uuid, String, String),
    // GeoFXNodeEdit(Uuid, String, String),
    // HeightmapEdit(Box<Heightmap>, Box<Heightmap>),
    MapEdit(Box<Map>, Box<Map>),
    // RegionTileEdit(Vec2i, Option<RegionTile>, Option<RegionTile>),
    // RegionFXEdit(RegionFXObject, RegionFXObject),
    RegionEdit(Box<Region>, Box<Region>),
}

impl RegionUndoAtom {
    pub fn to_material_atom(self) -> Option<MaterialUndoAtom> {
        match self {
            RegionUndoAtom::MapEdit(map1, map2) => Some(MaterialUndoAtom::MapEdit(map1, map2)),
            _ => None, // Return None for unsupported variants
        }
    }
    pub fn to_screen_atom(self) -> Option<ScreenUndoAtom> {
        match self {
            RegionUndoAtom::MapEdit(map1, map2) => Some(ScreenUndoAtom::MapEdit(map1, map2)),
            _ => None, // Return None for unsupported variants
        }
    }

    pub fn undo(&self, region: &mut Region, _ui: &mut TheUI, ctx: &mut TheContext) {
        match self {
            // RegionUndoAtom::GeoFXObjectsDeletion(objects) => {
            //     for object in objects {
            //         region.geometry.insert(object.id, object.clone());
            //     }
            //     region.update_geometry_areas();
            // }
            // RegionUndoAtom::GeoFXObjectEdit(id, prev, _) => {
            //     if let Some(prev) = prev {
            //         region.geometry.insert(*id, prev.clone());
            //     } else {
            //         region.geometry.remove(id);
            //     }
            //     region.update_geometry_areas();
            // }
            // RegionUndoAtom::GeoFXAddNode(id, prev, _)
            // | RegionUndoAtom::GeoFXNodeEdit(id, prev, _) => {
            //     if let Some(geo_obj) = region.geometry.get_mut(id) {
            //         *geo_obj = GeoFXObject::from_json(prev);

            //         let node_canvas = geo_obj.to_canvas();
            //         ui.set_node_canvas("Model NodeCanvas", node_canvas);

            //         ctx.ui.send(TheEvent::Custom(
            //             TheId::named_with_id("Update GeoFX Node", geo_obj.id),
            //             TheValue::Empty,
            //         ));
            //     }
            // }
            // RegionUndoAtom::HeightmapEdit(prev, _) => {
            //     region.heightmap = *prev.clone();
            //     ctx.ui.send(TheEvent::Custom(
            //         TheId::named("Update Minimaps"),
            //         TheValue::Empty,
            //     ));
            // }
            RegionUndoAtom::MapEdit(prev, _) => {
                region.map = *prev.clone();
                region.map.clear_temp();
                ctx.ui.send(TheEvent::Custom(
                    TheId::named("Update Minimaps"),
                    TheValue::Empty,
                ));
                ctx.ui.send(TheEvent::Custom(
                    TheId::named("Map Selection Changed"),
                    TheValue::Empty,
                ));

                crate::editor::RUSTERIX.write().unwrap().set_dirty();
            }
            // RegionUndoAtom::RegionTileEdit(pos, prev, _) => {
            //     if let Some(prev) = prev {
            //         region.tiles.insert((pos.x, pos.y), prev.clone());
            //     } else {
            //         region.tiles.remove(&(pos.x, pos.y));
            //     }
            // }
            // RegionUndoAtom::RegionFXEdit(prev, _) => {
            //     region.regionfx = prev.clone();

            //     let node_canvas = region.regionfx.to_canvas();
            //     ui.set_node_canvas("RegionFX NodeCanvas", node_canvas);

            //     ctx.ui.send(TheEvent::Custom(
            //         TheId::named("Show RegionFX Node"),
            //         TheValue::Empty,
            //     ));
            // }
            RegionUndoAtom::RegionEdit(prev, _) => {
                *region = *prev.clone();
            }
        }
    }
    pub fn redo(&self, region: &mut Region, _ui: &mut TheUI, ctx: &mut TheContext) {
        match self {
            // RegionUndoAtom::GeoFXObjectsDeletion(objects) => {
            //     for object in objects {
            //         region.geometry.remove(&object.id);
            //     }
            //     region.update_geometry_areas();
            // }

            // RegionUndoAtom::GeoFXObjectEdit(id, _, next) => {
            //     if let Some(next) = next {
            //         region.geometry.insert(*id, next.clone());
            //     } else {
            //         region.geometry.remove(id);
            //     }
            //     region.update_geometry_areas();
            // }
            // RegionUndoAtom::GeoFXAddNode(id, _, next)
            // | RegionUndoAtom::GeoFXNodeEdit(id, _, next) => {
            //     if let Some(geo_obj) = region.geometry.get_mut(id) {
            //         *geo_obj = GeoFXObject::from_json(next);

            //         let node_canvas = geo_obj.to_canvas();
            //         ui.set_node_canvas("Model NodeCanvas", node_canvas);

            //         ctx.ui.send(TheEvent::Custom(
            //             TheId::named_with_id("Update GeoFX Node", geo_obj.id),
            //             TheValue::Empty,
            //         ));
            //     }
            // }
            // RegionUndoAtom::HeightmapEdit(_, next) => {
            //     region.heightmap = *next.clone();
            //     ctx.ui.send(TheEvent::Custom(
            //         TheId::named("Update Minimaps"),
            //         TheValue::Empty,
            //     ));
            // }
            RegionUndoAtom::MapEdit(_, next) => {
                region.map = *next.clone();
                region.map.clear_temp();
                ctx.ui.send(TheEvent::Custom(
                    TheId::named("Update Minimaps"),
                    TheValue::Empty,
                ));
                ctx.ui.send(TheEvent::Custom(
                    TheId::named("Map Selection Changed"),
                    TheValue::Empty,
                ));

                crate::editor::RUSTERIX.write().unwrap().set_dirty();
            }
            // RegionUndoAtom::RegionTileEdit(pos, _, next) => {
            //     if let Some(next) = next {
            //         region.tiles.insert((pos.x, pos.y), next.clone());
            //     } else {
            //         region.tiles.remove(&(pos.x, pos.y));
            //     }
            // }
            // RegionUndoAtom::RegionFXEdit(_, next) => {
            //     region.regionfx = next.clone();

            //     let node_canvas = region.regionfx.to_canvas();
            //     ui.set_node_canvas("RegionFX NodeCanvas", node_canvas);

            //     ctx.ui.send(TheEvent::Custom(
            //         TheId::named("Show RegionFX Node"),
            //         TheValue::Empty,
            //     ));
            // }
            RegionUndoAtom::RegionEdit(_, next) => {
                *region = *next.clone();
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RegionUndo {
    pub stack: Vec<RegionUndoAtom>,
    pub index: isize,
}

impl Default for RegionUndo {
    fn default() -> Self {
        Self::new()
    }
}

impl RegionUndo {
    pub fn new() -> Self {
        Self {
            stack: vec![],
            index: -1,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    pub fn clear(&mut self) {
        self.stack = vec![];
        self.index = -1;
    }

    pub fn has_undo(&self) -> bool {
        self.index >= 0
    }

    pub fn has_redo(&self) -> bool {
        if self.index >= -1 && self.index < self.stack.len() as isize - 1 {
            return true;
        }
        false
    }

    pub fn add(&mut self, atom: RegionUndoAtom) {
        let to_remove = self.stack.len() as isize - self.index - 1;
        for _i in 0..to_remove {
            self.stack.pop();
        }
        self.stack.push(atom);
        self.index += 1;
    }

    pub fn undo(&mut self, region: &mut Region, ui: &mut TheUI, ctx: &mut TheContext) {
        if self.index >= 0 {
            self.stack[self.index as usize].undo(region, ui, ctx);
            self.index -= 1;
        }
    }

    pub fn redo(&mut self, region: &mut Region, ui: &mut TheUI, ctx: &mut TheContext) {
        if self.index < self.stack.len() as isize - 1 {
            self.index += 1;
            self.stack[self.index as usize].redo(region, ui, ctx);
        }
    }

    pub fn truncate_to_limit(&mut self, limit: usize) {
        if self.stack.len() > limit {
            let excess = self.stack.len() - limit;

            // Remove the oldest `excess` entries from the front
            self.stack.drain(0..excess);

            // Adjust the index accordingly
            self.index -= excess as isize;

            // Clamp to -1 minimum in case we truncated everything
            if self.index < -1 {
                self.index = -1;
            }
        }
    }
}
