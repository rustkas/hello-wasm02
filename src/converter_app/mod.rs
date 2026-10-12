pub mod state;
pub mod ui_builder;
pub mod handlers;
pub mod pipeline;
pub mod metrics;

use web_sys::HtmlElement;

pub fn build_converter_section() -> HtmlElement {
    let (section, ui, state) = ui_builder::build();
    handlers::wire(&ui, state);
    section
}