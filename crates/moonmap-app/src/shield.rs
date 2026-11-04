//! A `ui.view` that takes the pointer. Moonframe's floats let input through
//! to what lies behind them, so without one under it the place sheet's
//! scrolling and dragging would also zoom and pan the map.

use moonframe::{
    HostView, ViewInit,
    gpui_kit::{Context, InteractiveElement as _, IntoElement, Render, Styled as _, Window, div},
};

pub struct Shield;

impl Shield {
    pub fn new(_: &ViewInit, _: &mut Window, _: &mut Context<Self>) -> Self {
        Self
    }
}

impl HostView for Shield {}

impl Render for Shield {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().occlude()
    }
}
