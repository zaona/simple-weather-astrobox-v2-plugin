use crate::astrobox::psys_host;
use crate::astrobox::psys_host::ui;
use super::state::*;
use super::event_handler::*;

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

    let input_label = ui::Element::new(ui::ElementType::P, Some("请粘贴天气数据"))
        .size(16)
        .margin_bottom(8);

    let input_field = ui::Element::new(ui::ElementType::Input, Some(&state.weather_data))
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
        .width_full()
        .margin_bottom(20);

    let open_weather_button = ui::Element::new(ui::ElementType::Button, Some("打开天气数据查询网站"))
        .without_default_styles()
        .on(ui::Event::Click, OPEN_WEATHER_EVENT)
        .radius(8)
        .padding(14)
        .bg("#2A2A2A")
        .width_full()
        .margin_bottom(20);

    let open_guide_button = ui::Element::new(ui::ElementType::Button, Some("打开数据传输教程"))
        .without_default_styles()
        .on(ui::Event::Click, OPEN_GUIDE_EVENT)
        .radius(8)
        .padding(14)
        .bg("#2A2A2A")
        .width_full()
        .margin_bottom(20);

    let qq_tip = ui::Element::new(ui::ElementType::P, Some("QQ交流群：947038648"))
        .size(14)
        .text_color("#888888")
        .margin_bottom(20);

    container
        .child(input_label)
        .child(input_field)
        .child(send_button)
        .child(open_weather_button)
        .child(open_guide_button)
        .child(qq_tip)
}