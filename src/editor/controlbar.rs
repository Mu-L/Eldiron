
use crate::atom::AtomData;
use crate::widget::*;
use server::asset::Asset;

use crate::widget::atom:: { AtomWidget, AtomWidgetType };
use crate::widget::context::ScreenContext;

pub struct ControlBar {
    rect                    : (usize, usize, usize, usize),
    pub widgets             : Vec<AtomWidget>,
}

impl Widget for ControlBar {

    fn new(_text: Vec<String>, rect: (usize, usize, usize, usize), asset: &Asset, context: &ScreenContext) -> Self where Self: Sized {

        let mut widgets : Vec<AtomWidget> = vec![];

        let mut undo_button = AtomWidget::new(vec!["Undo".to_string()], AtomWidgetType::ToolBarButton,
            AtomData::new_as_int("Undo".to_string(), 0));
        undo_button.no_border = true;
        undo_button.state = WidgetState::Disabled;
        undo_button.set_rect((rect.0 + 10, rect.1, 80, rect.3), asset, context);
        widgets.push(undo_button);

        let mut redo_button = AtomWidget::new(vec!["Redo".to_string()], AtomWidgetType::ToolBarButton,
            AtomData::new_as_int("Redo".to_string(), 0));
        redo_button.no_border = true;
        redo_button.state = WidgetState::Disabled;
        redo_button.set_rect((rect.0 + 100, rect.1, 80, rect.3), asset, context);
        widgets.push(redo_button);

        let mut play_button = AtomWidget::new(vec!["Play".to_string()], AtomWidgetType::ToolBarButton,
            AtomData::new_as_int("Play".to_string(), 0));
        play_button.no_border = true;
        play_button.state = WidgetState::Disabled;
        play_button.set_rect((rect.2 - 100 - 100, rect.1, 80, rect.3), asset, context);
        widgets.push(play_button);

        let mut debug_button = AtomWidget::new(vec!["Debug".to_string()], AtomWidgetType::ToolBarButton,
            AtomData::new_as_int("Debug".to_string(), 0));
        debug_button.no_border = true;
        debug_button.set_rect((rect.2 - 110, rect.1, 100, rect.3), asset, context);
        widgets.push(debug_button);

        Self {
            rect,
            widgets             : widgets,
        }
    }

    fn resize(&mut self, width: usize, height: usize, _context: &ScreenContext) {
        self.rect.2 = width;
        self.rect.3 = height;
        self.widgets[0].rect.0 = width - 110;
    }

    fn draw(&mut self, frame: &mut [u8], anim_counter: usize, asset: &mut Asset, context: &mut ScreenContext) {
        context.draw2d.draw_rect(frame, &self.rect, context.width, &context.color_black);

        for atom in &mut self.widgets {
            atom.draw(frame, context.width, anim_counter, asset, context);
        }
    }

    fn draw_overlay(&mut self, frame: &mut [u8], rect: &(usize, usize, usize, usize), anim_counter: usize, asset: &mut Asset, context: &mut ScreenContext) {
        for atom in &mut self.widgets {
            atom.draw_overlay(frame, rect, anim_counter, asset, context);
        }
    }

    fn mouse_down(&mut self, pos: (usize, usize), asset: &mut Asset, context: &mut ScreenContext) -> bool {
        for atom_widget in &mut self.widgets {
            if atom_widget.mouse_down(pos, asset, context) {
                if atom_widget.atom_data.id == "Debug" {
                    if context.is_running == false {
                        context.data.create_behavior_instances();
                        context.data.activate_region_instances(context.data.regions_ids[context.curr_region_index]);
                        context.is_running = true;
                        atom_widget.text[0] = "Stop".to_string();
                        context.data.messages = vec![];
                    } else {
                        context.data.clear_instances();
                        context.is_running = false;
                        atom_widget.text[0] = "Debug".to_string();
                        context.just_stopped_running = true;
                    }
                }
                return true;
            }
        }
        false
    }

    fn mouse_up(&mut self, pos: (usize, usize), asset: &mut Asset, context: &mut ScreenContext) -> bool {
        let mut consumed = false;

        for atom in &mut self.widgets {
            if atom.mouse_up(pos, asset, context) {
                consumed = true;
            }
        }
        consumed
    }

    fn mouse_dragged(&mut self, pos: (usize, usize), asset: &mut Asset, context: &mut ScreenContext) -> bool {
        for atom in &mut self.widgets {
            if atom.mouse_dragged(pos, asset, context) {
                return true;
            }
        }
        false
    }

    fn mouse_hover(&mut self, pos: (usize, usize), asset: &mut Asset, context: &mut ScreenContext) -> bool {
        for atom in &mut self.widgets {
            if atom.mouse_hover(pos, asset, context) {
                return true;
            }
        }
        false
    }

    fn get_rect(&self) -> &(usize, usize, usize, usize) {
        return &self.rect;
    }
}