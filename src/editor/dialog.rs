use server::asset::Asset;
use server::gamedata::behavior::BehaviorNodeType;

use crate::atom::{ AtomWidget, AtomWidgetType, AtomData };
use crate::widget::{ WidgetKey, WidgetState };

use crate::context::ScreenContext;

use evalexpr::*;

#[derive(PartialEq, Debug)]
pub enum DialogEntry {
    None,
    NodeNumber,
    NodeExpression,
    NodeText,
}

#[derive(PartialEq, Debug)]
pub enum DialogState {
    Closed,
    Open,
    Opening,
    Closing
}

pub struct DialogWidget {
    pub rect                    : (usize, usize, usize, usize),
    pub text                    : String,

    pub widgets                 : Vec<AtomWidget>,

    dirty                       : bool,
    buffer                      : Vec<u8>,

    clicked_id                  : String,
}

impl DialogWidget {

    pub fn new() -> Self {

        let mut widgets : Vec<AtomWidget> = vec![];

        let cancel_button = AtomWidget::new(vec!["Cancel".to_string()], AtomWidgetType::ToolBarButton,
        AtomData::new_as_int("Cancel".to_string(), 0));
        widgets.push(cancel_button);

        let ok_button = AtomWidget::new(vec!["Accept".to_string()], AtomWidgetType::ToolBarButton,
        AtomData::new_as_int("Accept".to_string(), 0));
        widgets.push(ok_button);

        Self {
            rect                : (0, 0, 600, 200),
            text                : "".to_string(),

            widgets             : widgets,

            dirty               : true,
            buffer              : vec![0],

            clicked_id          : "".to_string(),
        }
    }

    pub fn draw(&mut self, frame: &mut [u8], anim_counter: usize, asset: &mut Asset, context: &mut ScreenContext) {

        let mut rect = (0_usize, 0_usize, self.rect.2, self.rect.3);

        // Animation
        if context.dialog_state == DialogState::Opening {
            context.dialog_height += 20;
            rect.3 = context.dialog_height;
            if context.dialog_height >= self.rect.3 {
                context.dialog_state = DialogState::Open;
                context.target_fps = context.default_fps;

                self.widgets[0].state = WidgetState::Normal;
                self.widgets[1].state = WidgetState::Normal;

                if context.dialog_entry == DialogEntry::NodeNumber {
                    self.text = format!("{}", context.dialog_node_behavior_value.0);
                } else
                if context.dialog_entry == DialogEntry::NodeExpression || context.dialog_entry == DialogEntry::NodeText {
                    self.text = context.dialog_node_behavior_value.4.clone();
                }
            }
            self.dirty = true;
        } else
        if context.dialog_state == DialogState::Closing {
            context.dialog_height -= 20;
            rect.3 = context.dialog_height;
            if context.dialog_height <= 20 {
                context.dialog_state = DialogState::Closed;
                context.target_fps = context.default_fps;
                return;
            }
            self.dirty = true;
        }

        if self.buffer.len() != rect.2 * rect.3 * 4 {
            self.buffer = vec![0;rect.2 * rect.3 * 4];
        }

        let buffer_frame = &mut self.buffer[..];

        if self.dirty {

            buffer_frame.iter_mut().map(|x| *x = 0).count();

            context.draw2d.draw_rounded_rect_with_border(buffer_frame, &rect, rect.2, &(rect.2 as f64 - 1.0, rect.3 as f64 - 1.0), &context.color_black, &(20.0, 0.0, 20.0, 0.0), &context.color_light_gray, 1.5);

            if context.dialog_state == DialogState::Open {

                let mut border_color : [u8; 4] = context.color_light_gray;

                if context.dialog_entry == DialogEntry::NodeNumber {
                    context.draw2d.draw_text(buffer_frame, &(40, 10), rect.2, &asset.open_sans, 40.0, &"Number".to_string(), &context.color_white, &context.color_black);

                    if self.text.parse::<f64>().is_err() {
                        border_color = context.color_red;
                        self.widgets[1].state = WidgetState::Disabled;
                    } else
                    if self.widgets[1].state == WidgetState::Disabled {
                        self.widgets[1].state = WidgetState::Normal;
                    }
                } else
                if context.dialog_entry == DialogEntry::NodeExpression {
                    context.draw2d.draw_text(buffer_frame, &(40, 10), rect.2, &asset.open_sans, 40.0, &"Expression".to_string(), &context.color_white, &context.color_black);

                    let mut cont = HashMapContext::new();
                    let behavior_id = context.dialog_node_behavior_id.0.clone();
                    if let Some(behavior) = context.data.behaviors.get_mut(&behavior_id) {
                        for n in &behavior.data.nodes {
                            if n.1.behavior_type == BehaviorNodeType::VariableNumber {
                                let t = format!("{} = {}", n.1.name, n.1.values.get("value").unwrap().0);
                                let _ = eval_empty_with_context_mut(t.as_str(), &mut cont);
                            }
                        }
                    }
                    let exp = eval_boolean_with_context(&self.text, &cont);
                    //println!("{:?}", exp);
                    if exp.is_err(){
                        border_color = context.color_red;
                        self.widgets[1].state = WidgetState::Disabled;
                    } else
                    if self.widgets[1].state == WidgetState::Disabled {
                        self.widgets[1].state = WidgetState::Normal;
                    }
                } else
                if context.dialog_entry == DialogEntry::NodeText {
                    context.draw2d.draw_text(buffer_frame, &(40, 10), rect.2, &asset.open_sans, 40.0, &"Text".to_string(), &context.color_white, &context.color_black);
                }

                let input_rect = (20, 60, rect.2 - 40, 60);
                context.draw2d.draw_rounded_rect_with_border(buffer_frame, &input_rect, rect.2, &(input_rect.2 as f64 - 1.0, input_rect.3 as f64 - 1.0), &context.color_black, &(20.0, 20.0, 20.0, 20.0), &border_color, 1.5);

                if !self.text.is_empty() {
                    context.draw2d.draw_text_rect(buffer_frame, &input_rect, rect.2, &asset.open_sans, 30.0, &self.text, &context.color_white, &context.color_black, crate::draw2d::TextAlignment::Center);
                }

                // Draw Cancel / Accept buttons
                self.widgets[0].set_rect((rect.2 - 280, rect.3 - 60, 120, 40), asset, context);
                self.widgets[1].set_rect((rect.2 - 140, rect.3 - 60, 120, 40), asset, context);

                for atom in &mut self.widgets {
                    atom.draw(buffer_frame, rect.2, anim_counter, asset, context);
                }
            }
        }
        self.dirty = false;
        context.draw2d.blend_slice(frame, buffer_frame, &(self.rect.0, self.rect.1, rect.2, rect.3), context.width);
    }

    /// Accepts the given value (if correct)
    pub fn accept_value(&mut self, context: &mut ScreenContext) -> bool {

        if context.dialog_entry == DialogEntry::NodeNumber {
            let int_value = self.text.parse::<i64>();
            if int_value.is_ok() {
                context.dialog_node_behavior_value.0 = int_value.unwrap() as f64;
                context.data.set_behavior_id_value(context.dialog_node_behavior_id.clone(), context.dialog_node_behavior_value.clone());
                return true;
            }
            let float_value = self.text.parse::<f64>();
            if float_value.is_ok() {
                context.dialog_node_behavior_value.0 = float_value.unwrap();
                context.data.set_behavior_id_value(context.dialog_node_behavior_id.clone(), context.dialog_node_behavior_value.clone());
                return true;
            }
        } else
        if context.dialog_entry == DialogEntry::NodeExpression {

            let mut cont = HashMapContext::new();
            let behavior_id = context.dialog_node_behavior_id.0.clone();
            if let Some(behavior) = context.data.behaviors.get_mut(&behavior_id) {
                for n in &behavior.data.nodes {
                    if n.1.behavior_type == BehaviorNodeType::VariableNumber {
                        let t = format!("{} = {}", n.1.name, n.1.values.get("value").unwrap().0);
                        let _ = eval_empty_with_context_mut(t.as_str(), &mut cont);
                    }
                }
            }
            let exp = eval_boolean_with_context(&self.text, &cont);
            if exp.is_ok() {
                context.dialog_node_behavior_value.4 = self.text.clone();
                context.data.set_behavior_id_value(context.dialog_node_behavior_id.clone(), context.dialog_node_behavior_value.clone());
                return true;
            }
        } else
        if context.dialog_entry == DialogEntry::NodeText {
            context.dialog_node_behavior_value.4 = self.text.clone();
            context.data.set_behavior_id_value(context.dialog_node_behavior_id.clone(), context.dialog_node_behavior_value.clone());
            return true;
        }
        false
    }

    pub fn key_down(&mut self, char: Option<char>, key: Option<WidgetKey>, _asset: &mut Asset, context: &mut ScreenContext) -> bool {
        //println!("dialog {:?}, {:?}", char, key);

        if let Some(key) = key {
            match key {
                WidgetKey::Delete => {
                    self.text.pop();
                    self.dirty = true;
                    return  true;
                },
                WidgetKey::Escape => {
                    context.dialog_state = DialogState::Closing;
                    context.target_fps = 60;
                    return  true;
                },
                WidgetKey::Return => {
                    if self.accept_value(context) {
                        context.dialog_state = DialogState::Closing;
                        context.target_fps = 60;
                        return  true;
                    }
                },
                _ => {}
            }
        }

        if let Some(c) = char {
            if c.is_ascii() && c.is_control() == false {
                self.text.push(c);
                self.dirty = true;
                return true;
            }
        }
        false
    }

    pub fn mouse_down(&mut self, pos: (usize, usize), asset: &mut Asset, context: &mut ScreenContext) -> bool {
        self.clicked_id = "".to_string();

        if pos.0 < self.rect.0 || pos.1 < self.rect.1 { return false; }
        let local = (pos.0 - self.rect.0, pos.1 - self.rect.1);
        for atom in &mut self.widgets {
            if atom.mouse_down(local, asset, context) {
                self.dirty = true;
                self.clicked_id = atom.atom_data.id.clone();
                return true;
            }
        }
        false
    }

    pub fn mouse_up(&mut self, pos: (usize, usize), asset: &mut Asset, context: &mut ScreenContext) -> bool {

        if pos.0 < self.rect.0 || pos.1 < self.rect.1 { return false; }
        let local = (pos.0 - self.rect.0, pos.1 - self.rect.1);
        for atom in &mut self.widgets {
            if atom.mouse_up(local, asset, context) {
                self.dirty = true;

                if self.clicked_id == "Cancel" {
                    context.dialog_state = DialogState::Closing;
                    context.target_fps = 60;
                } else
                if self.clicked_id == "Accept" {
                    if self.accept_value(context) {
                        context.dialog_state = DialogState::Closing;
                        context.target_fps = 60;
                    }
                }

                return true;
            }
        }

        false
    }

    pub fn mouse_dragged(&mut self, pos: (usize, usize), asset: &mut Asset, context: &mut ScreenContext) -> bool {
        if pos.0 < self.rect.0 || pos.1 < self.rect.1 { return false; }
        let local = (pos.0 - self.rect.0, pos.1 - self.rect.1);
        for atom in &mut self.widgets {
            if atom.mouse_dragged(local, asset, context) {
                self.dirty = true;
                return true;
            }
        }
        false
    }

    pub fn mouse_hover(&mut self, pos: (usize, usize), asset: &mut Asset, context: &mut ScreenContext) -> bool {
        if pos.0 < self.rect.0 || pos.1 < self.rect.1 { return false; }
        let local = (pos.0 - self.rect.0, pos.1 - self.rect.1);
        for atom in &mut self.widgets {
            if atom.mouse_hover(local, asset, context) {
                self.dirty = true;
                return true;
            }
        }
        false
    }
}