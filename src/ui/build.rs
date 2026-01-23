use crate::astrobox::psys_host;
use crate::astrobox::psys_host::ui;
use super::state::*;

pub fn render_main_ui(element_id: &str) {
    {
        let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.root_element_id = Some(element_id.to_string());
    }

    let ui_tree = build_main_ui();
    psys_host::ui::render(element_id, ui_tree);
}

pub fn build_main_ui() -> ui::Element {
    let state = ui_state()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let container = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .padding(20);

    let message_element = if let Some(ref msg) = state.message {
        let bg_color = if state.is_success { "#4CAF50" } else { "#FF4444" };
        Some(
            ui::Element::new(ui::ElementType::Div, None)
                .bg(bg_color)
                .radius(8)
                .padding(12)
                .margin_bottom(20)
                .child(
                    ui::Element::new(ui::ElementType::P, Some(msg))
                        .size(14)
                        .text_color("#FFFFFF"),
                ),
        )
    } else {
        None
    };

    let input_label = ui::Element::new(ui::ElementType::P, Some("输入要发送的数据"))
        .size(16)
        .margin_bottom(8);

    let input_field = ui::Element::new(ui::ElementType::Input, Some(&state.input_value))
        .on(ui::Event::Change, INPUT_CHANGE_EVENT)
        .radius(8)
        .bg("#2A2A2A")
        .width_full()
        .margin_bottom(20);

    let send_button = ui::Element::new(ui::ElementType::Button, Some("发送"))
        .without_default_styles()
        .on(ui::Event::Click, SEND_BUTTON_EVENT)
        .radius(8)
        .padding(14)
        .bg("#2A2A2A")
        .width_full();

    let mut ui_tree = container;

    if let Some(msg_el) = message_element {
        ui_tree = ui_tree.child(msg_el);
    }

    ui_tree
        .child(input_label)
        .child(input_field)
        .child(send_button)
}
