use crate::astrobox::psys_host;
use crate::astrobox::psys_host::ui;
use super::state::*;
use super::event_handler::*;

pub fn render_main_ui(element_id: &str) {
    {
        let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.root_element_id = Some(element_id.to_string());
    }

    crate::ui::state::load_api_settings_once();

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

    let tabs = build_tabs(&state);
    let content = match state.current_tab {
        MainTab::PasteData => build_paste_tab(&state),
        MainTab::CustomApi => build_custom_api_tab(&state),
    };

    container
        .child(tabs)
        .child(content)
}

fn build_tabs(state: &UiState) -> ui::Element {
    let tabs_wrapper = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .justify_center()
        .margin_bottom(20);

    let tabs_container = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .bg("#1E1E1F")
        .radius(999)
        .padding(4);

    let paste_tab = build_tab_button(
        "发送数据",
        send_tab_svg(),
        state.current_tab == MainTab::PasteData,
        TAB_PASTE_EVENT,
    )
    .margin_right(4);

    let custom_tab = build_tab_button(
        "API设置",
        api_tab_svg(),
        state.current_tab == MainTab::CustomApi,
        TAB_CUSTOM_API_EVENT,
    );

    tabs_wrapper.child(tabs_container.child(paste_tab).child(custom_tab))
}

fn build_paste_tab(state: &UiState) -> ui::Element {
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

    ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .child(input_label)
        .child(input_field)
        .child(send_button)
        .child(open_weather_button)
        .child(open_guide_button)
        .child(qq_tip)
}

fn build_custom_api_tab(state: &UiState) -> ui::Element {
    let api_label = ui::Element::new(ui::ElementType::P, Some("API Host"))
        .size(16)
        .margin_bottom(8);

    let host_display = mask_value(&state.custom_api_host, state.show_api_host);
    let api_input = ui::Element::new(ui::ElementType::Input, Some(&host_display))
        .on(ui::Event::Change, CUSTOM_API_HOST_CHANGE_EVENT)
        .radius(8)
        .bg("#2A2A2A")
        .height(INPUT_HEIGHT)
        .width_full()
        .padding_right(48);

    let host_toggle_button = build_eye_toggle_button(
        if state.show_api_host { "隐藏" } else { "查看" },
        state.show_api_host,
        TOGGLE_SHOW_API_HOST_EVENT,
    )
    .margin_left(8);

    let host_row = ui::Element::new(ui::ElementType::Div, None)
        .relative()
        .width_full()
        .margin_bottom(16)
        .child(api_input)
        .child(
            host_toggle_button
                .absolute()
                .right(ICON_RIGHT)
                .top(ICON_TOP),
        );

    let key_label = ui::Element::new(ui::ElementType::P, Some("API Key"))
        .size(16)
        .margin_bottom(8);

    let key_display = mask_value(&state.custom_api_key, state.show_api_key);
    let key_input = ui::Element::new(ui::ElementType::Input, Some(&key_display))
        .on(ui::Event::Change, CUSTOM_API_KEY_CHANGE_EVENT)
        .radius(8)
        .bg("#2A2A2A")
        .height(INPUT_HEIGHT)
        .width_full()
        .padding_right(48);

    let key_toggle_button = build_eye_toggle_button(
        if state.show_api_key { "隐藏" } else { "查看" },
        state.show_api_key,
        TOGGLE_SHOW_API_KEY_EVENT,
    )
    .margin_left(8);

    let key_row = ui::Element::new(ui::ElementType::Div, None)
        .relative()
        .width_full()
        .margin_bottom(24)
        .child(key_input)
        .child(
            key_toggle_button
                .absolute()
                .right(ICON_RIGHT)
                .top(ICON_TOP),
        );

    let save_button = ui::Element::new(ui::ElementType::Button, Some("保存并验证"))
        .without_default_styles()
        .on(ui::Event::Click, API_SAVE_TEST_EVENT)
        .radius(8)
        .padding(14)
        .bg("#2A2A2A")
        .width_full()
        .margin_bottom(16);

    let action_row = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row);

    let reset_button = ui::Element::new(ui::ElementType::Button, Some("重置"))
        .without_default_styles()
        .on(ui::Event::Click, API_RESET_EVENT)
        .radius(8)
        .padding_top(14)
        .padding_bottom(14)
        .padding_left(24)
        .padding_right(24)
        .bg("#2A2A2A")
        .margin_right(16);

    let help_button = ui::Element::new(ui::ElementType::Button, Some("帮助"))
        .without_default_styles()
        .on(ui::Event::Click, API_HELP_EVENT)
        .radius(8)
        .padding_top(14)
        .padding_bottom(14)
        .padding_left(24)
        .padding_right(24)
        .bg("#2A2A2A");

    ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .child(api_label)
        .child(host_row)
        .child(key_label)
        .child(key_row)
        .child(save_button)
        .child(action_row.child(reset_button).child(help_button))
}

pub fn rerender_main_ui() {
    let element_id = {
        let state = ui_state().read().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.root_element_id.clone()
    };

    if let Some(element_id) = element_id {
        let ui_tree = build_main_ui();
        psys_host::ui::render(&element_id, ui_tree);
    }
}

fn mask_value(value: &str, show: bool) -> String {
    if show || value.is_empty() {
        value.to_string()
    } else {
        "*".repeat(value.chars().count().max(6))
    }
}

const INPUT_HEIGHT: u32 = 32;
const ICON_SIZE: u32 = 24;
const ICON_TOP: u32 = (INPUT_HEIGHT - ICON_SIZE) / 2;
const ICON_RIGHT: u32 = 8;

fn build_eye_toggle_button(_label: &str, is_open: bool, event_id: &str) -> ui::Element {
    let eye_svg = if is_open { eye_open_svg() } else { eye_closed_svg() };

    let icon = ui::Element::new(ui::ElementType::Svg, Some(&eye_svg))
        .width(16)
        .height(16);

    ui::Element::new(ui::ElementType::Button, None)
        .without_default_styles()
        .on(ui::Event::Click, event_id)
        .width(ICON_SIZE)
        .height(ICON_SIZE)
        .flex()
        .justify_center()
        .align_center()
        .child(icon)
}

fn build_tab_button(label: &str, icon_svg: String, is_active: bool, event_id: &str) -> ui::Element {
    let icon = ui::Element::new(ui::ElementType::Svg, Some(&icon_svg))
        .width(20)
        .height(20);

    let text = ui::Element::new(ui::ElementType::Span, Some(label))
        .size(14);

    ui::Element::new(ui::ElementType::Button, None)
        .without_default_styles()
        .on(ui::Event::Click, event_id)
        .radius(999)
        .padding_top(10)
        .padding_bottom(10)
        .padding_left(14)
        .padding_right(14)
        .bg(if is_active { "#2A2A2A" } else { "#1E1E1F" })
        .text_color(if is_active { "#FFFFFF" } else { "#BBBBBB" })
        .flex()
        .align_center()
        .child(icon)
        .child(ui::Element::new(ui::ElementType::Span, None).width(5))
        .child(text)
}

fn api_tab_svg() -> String {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256">
<rect width="256" height="256" fill="none"/>
<line x1="184" y1="80" x2="216" y2="80" fill="none" stroke="#FFFFFF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
<line x1="40" y1="80" x2="152" y2="80" fill="none" stroke="#FFFFFF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
<line x1="120" y1="176" x2="216" y2="176" fill="none" stroke="#FFFFFF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
<line x1="40" y1="176" x2="88" y2="176" fill="none" stroke="#FFFFFF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
<line x1="152" y1="56" x2="152" y2="104" fill="none" stroke="#FFFFFF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
<line x1="88" y1="152" x2="88" y2="200" fill="none" stroke="#FFFFFF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
</svg>"##
        .to_string()
}

fn send_tab_svg() -> String {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256">
<rect width="256" height="256" fill="none"/>
<path d="M191.11,112.89c24-24,25.5-52.55,24.75-65.28a8,8,0,0,0-7.47-7.47c-12.73-.75-41.26.73-65.28,24.75L80,128l48,48Z" fill="none" stroke="#FFFFFF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
<path d="M136,72H74.35a8,8,0,0,0-5.65,2.34L34.35,108.69a8,8,0,0,0,4.53,13.57L80,128" fill="none" stroke="#FFFFFF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
<path d="M184,120v61.65a8,8,0,0,1-2.34,5.65l-34.35,34.35a8,8,0,0,1-13.57-4.53L128,176" fill="none" stroke="#FFFFFF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
<path d="M94.56,187.82C90.69,196.31,77.65,216,40,216c0-37.65,19.69-50.69,28.18-54.56" fill="none" stroke="#FFFFFF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
</svg>"##
        .to_string()
}

fn eye_open_svg() -> String {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256">
<rect width="256" height="256" fill="none"/>
<path d="M128,56C48,56,16,128,16,128s32,72,112,72,112-72,112-72S208,56,128,56Z" fill="none" stroke="#FFFFFF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
<circle cx="128" cy="128" r="40" fill="none" stroke="#FFFFFF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
</svg>"##
        .to_string()
}

fn eye_closed_svg() -> String {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256">
<rect width="256" height="256" fill="none"/>
<path d="M32,104c16.81,20.81,47.63,48,96,48s79.19-27.19,96-48" fill="none" stroke="#FFFFFF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
<line x1="224" y1="168" x2="200.62" y2="127.09" fill="none" stroke="#FFFFFF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
<line x1="160" y1="192" x2="152.91" y2="149.45" fill="none" stroke="#FFFFFF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
<line x1="96" y1="192" x2="103.09" y2="149.45" fill="none" stroke="#FFFFFF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
<line x1="32" y1="168" x2="55.38" y2="127.09" fill="none" stroke="#FFFFFF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
</svg>"##
        .to_string()
}
