
use crate::widget::*;
use crate::Asset;

//use crate::tileselector::{ TileSelectorWidget, TileSelectorHelper };

use crate::button::ButtonWidget;
use crate::widget::context::ScreenContext;

pub struct ToolBar {
    rect                    : (usize, usize, usize, usize),
    state                   : WidgetState,
    button_widget           : ButtonWidget,
    clicked                 : bool,
}

impl Widget for ToolBar {

    fn new(_text: Vec<String>, rect: (usize, usize, usize, usize), asset: &Asset, context: &ScreenContext) -> Self where Self: Sized {

        let button_widget = ButtonWidget::new(vec!["Game".to_string()], (rect.0 + 10, rect.1, 100, rect.3), asset, context);

        Self {
            rect,
            state               : WidgetState::Normal,
            button_widget,
            clicked             : false,
        }
    }    

    fn draw(&mut self, frame: &mut [u8], anim_counter: usize, asset: &mut Asset, context: &ScreenContext) {
        context.draw2d.draw_rect(frame, &self.rect, context.width, &[25, 25, 25, 255]);
        self.button_widget.draw(frame, anim_counter, asset, context);
    }

    fn get_rect(&self) -> &(usize, usize, usize, usize) {
        return &self.rect;
    }
}