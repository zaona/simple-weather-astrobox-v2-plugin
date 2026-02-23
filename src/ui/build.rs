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
        MainTab::PasteData => build_send_tab(&state),
        MainTab::Settings => build_settings_tab(&state),
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
        "设置",
        api_tab_svg(),
        state.current_tab == MainTab::Settings,
        TAB_SETTINGS_EVENT,
    );

    tabs_wrapper.child(tabs_container.child(paste_tab).child(custom_tab))
}

fn build_send_tab(state: &UiState) -> ui::Element {
    if state.advanced_mode {
        return build_advanced_send_tab(state);
    }

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

    let open_weather_button = build_icon_text_button_full(
        "打开天气数据查询网站",
        open_site_svg(),
        OPEN_WEATHER_EVENT,
    )
    .margin_bottom(20);

    let open_guide_button = build_icon_text_button_full(
        "打开数据传输教程",
        guide_svg(),
        OPEN_GUIDE_EVENT,
    )
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

fn build_settings_tab(state: &UiState) -> ui::Element {
    match state.settings_page {
        SettingsPage::Main => build_settings_main(state),
        SettingsPage::Api => build_settings_api(state),
    }
}

fn build_settings_main(state: &UiState) -> ui::Element {
    let title = ui::Element::new(ui::ElementType::P, Some("高级同步模式"))
        .size(16)
        .margin_bottom(8);

    let switch_container = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .margin_bottom(16);

    let enable_btn = ui::Element::new(ui::ElementType::Button, Some("开启"))
        .without_default_styles()
        .on(ui::Event::Click, ADV_MODE_ON_EVENT)
        .radius(999)
        .padding_top(10)
        .padding_bottom(10)
        .padding_left(18)
        .padding_right(18)
        .bg(if state.advanced_mode { "#2A2A2A" } else { "#1E1E1F" })
        .text_color(if state.advanced_mode { "#FFFFFF" } else { "#BBBBBB" })
        .margin_right(8);

    let disable_btn = ui::Element::new(ui::ElementType::Button, Some("关闭"))
        .without_default_styles()
        .on(ui::Event::Click, ADV_MODE_OFF_EVENT)
        .radius(999)
        .padding_top(10)
        .padding_bottom(10)
        .padding_left(18)
        .padding_right(18)
        .bg(if !state.advanced_mode { "#2A2A2A" } else { "#1E1E1F" })
        .text_color(if !state.advanced_mode { "#FFFFFF" } else { "#BBBBBB" });

    let desc = ui::Element::new(
        ui::ElementType::P,
        Some("开启后可直接在插件内搜索位置并获取天气数据"),
    )
    .size(13)
    .text_color("#888888")
    .margin_bottom(20);

    let api_entry = build_icon_text_button_full("进入API设置", api_tab_svg(), OPEN_SETTINGS_API_EVENT)
        .margin_bottom(16);

    ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .child(title)
        .child(switch_container.child(enable_btn).child(disable_btn))
        .child(desc)
        .child(api_entry)
}

fn build_settings_api(state: &UiState) -> ui::Element {
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

    let save_button = build_icon_text_button_full("保存并验证", save_svg(), API_SAVE_TEST_EVENT)
        .margin_bottom(16);

    let action_row = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row);

    let reset_button = build_icon_text_button_inline("重置", reset_svg(), API_RESET_EVENT)
        .margin_right(16);

    let help_icon = ui::Element::new(ui::ElementType::Svg, Some(&help_tab_svg()))
        .width(16)
        .height(16);

    let help_text = ui::Element::new(ui::ElementType::Span, Some("帮助"))
        .size(14);

    let help_button = ui::Element::new(ui::ElementType::Button, None)
        .without_default_styles()
        .on(ui::Event::Click, API_HELP_EVENT)
        .radius(8)
        .padding_top(14)
        .padding_bottom(14)
        .padding_left(20)
        .padding_right(20)
        .bg("#2A2A2A")
        .flex()
        .align_center()
        .child(help_icon)
        .child(ui::Element::new(ui::ElementType::Span, None).width(6))
        .child(help_text);

    ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .child(
            build_icon_text_button_full("返回设置", api_tab_svg(), SETTINGS_BACK_EVENT)
                .margin_bottom(16),
        )
        .child(api_label)
        .child(host_row)
        .child(key_label)
        .child(key_row)
        .child(save_button)
        .child(action_row.child(reset_button).child(help_button))
}

fn build_advanced_send_tab(state: &UiState) -> ui::Element {
    let warn = if state.use_custom_api {
        None
    } else {
        Some(
            ui::Element::new(ui::ElementType::P, Some("高级模式需要先启用自定义API"))
                .size(13)
                .text_color("#FFB86C")
                .margin_bottom(12),
        )
    };

    let search_label = ui::Element::new(ui::ElementType::P, Some("搜索城市"))
        .size(16)
        .margin_bottom(8);

    let search_input = ui::Element::new(ui::ElementType::Input, Some(&state.search_query))
        .on(ui::Event::Change, SEARCH_INPUT_CHANGE_EVENT)
        .radius(8)
        .bg("#2A2A2A")
        .height(INPUT_HEIGHT)
        .width_full()
        .margin_bottom(12);

    let search_button = build_icon_text_button_full("搜索", send_tab_svg(), SEARCH_BUTTON_EVENT)
        .margin_bottom(16);

    let selected_text = if state.selected_location_name.is_empty() {
        "未选择位置".to_string()
    } else {
        format!("已选择：{}", state.selected_location_name)
    };

    let selected_label = ui::Element::new(ui::ElementType::P, Some(&selected_text))
        .size(14)
        .text_color("#BBBBBB")
        .margin_bottom(12);

    let results_container = build_location_results(state);

    let days_label = ui::Element::new(ui::ElementType::P, Some("同步天数"))
        .size(16)
        .margin_bottom(8);

    let days_row = build_days_row(state);

    let send_button = build_icon_text_button_full("发送", send_tab_svg(), SEND_BUTTON_EVENT)
        .margin_top(16);

    let mut root = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full();

    if let Some(warn) = warn {
        root = root.child(warn);
    }

    root
        .child(search_label)
        .child(search_input)
        .child(search_button)
        .child(selected_label)
        .child(results_container)
        .child(days_label)
        .child(days_row)
        .child(send_button)
}

fn build_location_results(state: &UiState) -> ui::Element {
    let mut container = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .margin_bottom(16);

    if state.search_results.is_empty() {
        return container.child(
            ui::Element::new(ui::ElementType::P, Some("暂无搜索结果"))
                .size(13)
                .text_color("#888888"),
        );
    }

    for (idx, item) in state.search_results.iter().enumerate() {
        let label = if item.adm1.is_empty() && item.adm2.is_empty() {
            item.name.clone()
        } else {
            format!("{} · {} {}", item.name, item.adm1, item.adm2).trim().to_string()
        };
        let btn = build_icon_text_button_full(
            &label,
            send_tab_svg(),
            &format!("{}{}", SELECT_LOCATION_PREFIX, idx),
        )
        .margin_bottom(8);
        container = container.child(btn);
    }

    container
}

fn build_days_row(state: &UiState) -> ui::Element {
    let options = [3u32, 7, 10, 15, 30];
    let mut row = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .width_full()
        .margin_bottom(12);

    for (i, day) in options.iter().enumerate() {
        let is_active = *day == state.selected_days;
        let btn = ui::Element::new(ui::ElementType::Button, Some(&format!("{}天", day)))
            .without_default_styles()
            .on(ui::Event::Click, &format!("{}{}", SELECT_DAYS_PREFIX, day))
            .radius(999)
            .padding_top(8)
            .padding_bottom(8)
            .padding_left(16)
            .padding_right(16)
            .bg(if is_active { "#2A2A2A" } else { "#1E1E1F" })
            .text_color(if is_active { "#FFFFFF" } else { "#BBBBBB" });

        row = row.child(btn);
        if i < options.len() - 1 {
            row = row.child(ui::Element::new(ui::ElementType::Span, None).width(6));
        }
    }

    row
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

fn build_icon_text_button_full(label: &str, icon_svg: String, event_id: &str) -> ui::Element {
    let icon = ui::Element::new(ui::ElementType::Svg, Some(&icon_svg))
        .width(16)
        .height(16);

    let text = ui::Element::new(ui::ElementType::Span, Some(label))
        .size(14);

    ui::Element::new(ui::ElementType::Button, None)
        .without_default_styles()
        .on(ui::Event::Click, event_id)
        .radius(8)
        .padding(14)
        .bg("#2A2A2A")
        .width_full()
        .flex()
        .align_center()
        .child(icon)
        .child(ui::Element::new(ui::ElementType::Span, None).width(6))
        .child(text)
}

fn build_icon_text_button_inline(label: &str, icon_svg: String, event_id: &str) -> ui::Element {
    let icon = ui::Element::new(ui::ElementType::Svg, Some(&icon_svg))
        .width(16)
        .height(16);

    let text = ui::Element::new(ui::ElementType::Span, Some(label))
        .size(14);

    ui::Element::new(ui::ElementType::Button, None)
        .without_default_styles()
        .on(ui::Event::Click, event_id)
        .radius(8)
        .padding_top(14)
        .padding_bottom(14)
        .padding_left(20)
        .padding_right(20)
        .bg("#2A2A2A")
        .flex()
        .align_center()
        .child(icon)
        .child(ui::Element::new(ui::ElementType::Span, None).width(6))
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

fn help_tab_svg() -> String {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256">
<rect width="256" height="256" fill="none"/>
<circle cx="128" cy="180" r="12" fill="#FFFFFF"/>
<path d="M128,144v-8c17.67,0,32-12.54,32-28s-14.33-28-32-28S96,92.54,96,108v4" fill="none" stroke="#FFFFFF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
<circle cx="128" cy="128" r="96" fill="none" stroke="#FFFFFF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
</svg>"##
        .to_string()
}

fn guide_svg() -> String {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256">
<rect width="256" height="256" fill="none"/>
<circle cx="128" cy="180" r="12" fill="#FFFFFF"/>
<path d="M128,144v-8c17.67,0,32-12.54,32-28s-14.33-28-32-28S96,92.54,96,108v4" fill="none" stroke="#FFFFFF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
<circle cx="128" cy="128" r="96" fill="none" stroke="#FFFFFF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
</svg>"##
        .to_string()
}

fn open_site_svg() -> String {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256">
<rect width="256" height="256" fill="none"/>
<rect x="32" y="48" width="192" height="160" rx="8" fill="none" stroke="#FFFFFF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
<line x1="32" y1="96" x2="224" y2="96" fill="none" stroke="#FFFFFF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
</svg>"##
        .to_string()
}

fn save_svg() -> String {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256">
<rect width="256" height="256" fill="none"/>
<path d="M216,83.31V208a8,8,0,0,1-8,8H48a8,8,0,0,1-8-8V48a8,8,0,0,1,8-8H172.69a8,8,0,0,1,5.65,2.34l35.32,35.32A8,8,0,0,1,216,83.31Z" fill="none" stroke="#FFFFFF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
<path d="M80,216V152a8,8,0,0,1,8-8h80a8,8,0,0,1,8,8v64" fill="none" stroke="#FFFFFF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
<line x1="152" y1="72" x2="96" y2="72" fill="none" stroke="#FFFFFF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
</svg>"##
        .to_string()
}

fn reset_svg() -> String {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256">
<rect width="256" height="256" fill="none"/>
<path d="M112,224a95.2,95.2,0,0,1-29-48" fill="none" stroke="#FFFFFF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
<path d="M192,152c0,31.67,13.31,59,40,72H61A103.65,103.65,0,0,1,32,152c0-28.21,11.23-50.89,29.47-69.64a8,8,0,0,1,8.67-1.81L95.52,90.83a16,16,0,0,0,20.82-9l21-53.11c4.15-10,15.47-15.32,25.63-11.53a20,20,0,0,1,11.51,26.4L153.13,96.69a16,16,0,0,0,8.93,20.76L187,127.29a8,8,0,0,1,5,7.43Z" fill="none" stroke="#FFFFFF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
<line x1="43.93" y1="105.57" x2="192.8" y2="165.12" fill="none" stroke="#FFFFFF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
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
