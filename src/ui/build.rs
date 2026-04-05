use super::SYNC_CARD_ID;
use super::event_handler::*;
use super::state::*;
use crate::astrobox::psys_host;
use crate::astrobox::psys_host::ui;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn render_main_ui(element_id: &str) {
    {
        let mut state = ui_state()
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.root_element_id = Some(element_id.to_string());
    }

    crate::ui::state::load_api_settings_once();
    crate::ui::event_handler::resolve_recent_locations_if_needed();

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

    container.child(tabs).child(content)
}

pub fn render_sync_card(card_id: &str) {
    tracing::info!("render_sync_card called: card_id={}", card_id);
    if card_id != SYNC_CARD_ID {
        tracing::info!(
            "render_sync_card id mismatch: expected {}, got {}",
            SYNC_CARD_ID,
            card_id
        );
    }
    let text = build_sync_card_text();
    tracing::info!("render_sync_card content: {}", text);
    psys_host::ui::render_to_text_card(card_id, &text);
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
        "同步数据",
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
    build_advanced_send_tab(state)
}

fn build_settings_tab(state: &UiState) -> ui::Element {
    build_settings_main(state)
}

fn build_settings_main(_state: &UiState) -> ui::Element {
    let mut root = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full();

    let more_title = build_section_title("更多内容");
    root = root.child(more_title);

    let afd_card = build_settings_card(
        afd_svg(),
        "赞助支持",
        Some("简明天气完全免费开源"),
        Some(build_more_link_icon()),
        Some(OPEN_AFD_EVENT),
    );

    let help_card = build_settings_card(
        help_doc_svg(),
        "帮助文档",
        Some("操作步骤与常见问题解答"),
        Some(build_more_link_icon()),
        Some(OPEN_HELP_DOC_EVENT),
    );

    let qq_card = build_settings_card(
        qq_group_svg(),
        "QQ群",
        Some("947038648"),
        Some(build_more_link_icon()),
        Some(OPEN_QQ_GROUP_EVENT),
    );

    let build_title = build_section_title("构建信息");

    let build_time_raw = option_env!("AB_BUILD_TIME").unwrap_or("unknown");
    let build_user = option_env!("AB_BUILD_USER").unwrap_or("unknown");
    let build_branch = option_env!("AB_BUILD_GIT_BRANCH").unwrap_or("unknown");
    let build_hash = short_git_hash(option_env!("AB_BUILD_GIT_HASH").unwrap_or("unknown"));
    let build_time = format_beijing_time(build_time_raw);

    let build_time_row = build_settings_card(
        build_time_svg(),
        "构建时间",
        None,
        Some(build_value_text(&build_time)),
        None,
    );
    let build_user_row = build_settings_card(
        build_user_svg(),
        "构建用户",
        None,
        Some(build_value_text(build_user)),
        None,
    );
    let build_branch_row = build_settings_card(
        build_branch_svg(),
        "当前分支",
        None,
        Some(build_value_text(build_branch)),
        None,
    );
    let build_hash_row = build_settings_card(
        build_hash_svg(),
        "当前hash",
        None,
        Some(build_value_text(&build_hash)),
        None,
    );

    root.child(afd_card.margin_bottom(10))
        .child(help_card.margin_bottom(10))
        .child(qq_card.margin_bottom(18))
        .child(build_title)
        .child(build_time_row.margin_bottom(10))
        .child(build_user_row.margin_bottom(10))
        .child(build_branch_row.margin_bottom(10))
        .child(build_hash_row)
}

fn build_sync_card_text() -> String {
    let state = ui_state()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let location = if state.last_sync_location.is_empty() {
        "暂无"
    } else {
        state.last_sync_location.as_str()
    };

    let (time_text, expired) = if state.last_sync_time_ms == 0 {
        ("暂无".to_string(), false)
    } else {
        let elapsed = now_ms().saturating_sub(state.last_sync_time_ms);
        let expire_ms = state.selected_days as u64 * 24 * 60 * 60 * 1000;
        let expired = expire_ms > 0 && elapsed > expire_ms;
        (format_relative(elapsed), expired)
    };

    let expired_mark = if expired { " (已过期)" } else { "" };
    format!(
        "上次同步\n地点: {}\n时间: {}{}",
        location, time_text, expired_mark
    )
}

fn format_relative(elapsed_ms: u64) -> String {
    let seconds = elapsed_ms / 1000;
    if seconds < 60 {
        return "刚刚".to_string();
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{}分钟前", minutes);
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{}小时前", hours);
    }
    let days = hours / 24;
    format!("{}天前", days)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn format_beijing_time(raw: &str) -> String {
    if let Some((y, m, d, hh, mm, ss)) = parse_iso_utc(raw) {
        let (y2, m2, d2, hh2) = add_hours(y, m, d, hh, 8);
        return format!(
            "{:04}‑{:02}‑{:02}_{:02}:{:02}:{:02}",
            y2, m2, d2, hh2, mm, ss
        );
    }
    raw.to_string()
}

fn parse_iso_utc(raw: &str) -> Option<(i32, i32, i32, i32, i32, i32)> {
    if raw.len() < 19 {
        return None;
    }
    let base = &raw[..19];
    let mut parts = base.split('T');
    let date = parts.next()?;
    let time = parts.next()?;
    let mut dparts = date.split('-');
    let y: i32 = dparts.next()?.parse().ok()?;
    let m: i32 = dparts.next()?.parse().ok()?;
    let d: i32 = dparts.next()?.parse().ok()?;
    let mut tparts = time.split(':');
    let hh: i32 = tparts.next()?.parse().ok()?;
    let mm: i32 = tparts.next()?.parse().ok()?;
    let ss: i32 = tparts.next()?.parse().ok()?;
    Some((y, m, d, hh, mm, ss))
}

fn add_hours(mut y: i32, mut m: i32, mut d: i32, mut hh: i32, add: i32) -> (i32, i32, i32, i32) {
    hh += add;
    while hh >= 24 {
        hh -= 24;
        d += 1;
        let dim = days_in_month(y, m);
        if d > dim {
            d = 1;
            m += 1;
            if m > 12 {
                m = 1;
                y += 1;
            }
        }
    }
    (y, m, d, hh)
}

fn days_in_month(y: i32, m: i32) -> i32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(y) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

fn is_leap_year(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

fn build_value_text(value: &str) -> ui::Element {
    ui::Element::new(ui::ElementType::P, Some(value))
        .size(13)
        .text_color("#BBBBBB")
}

fn build_time_svg() -> String {
    r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256"><rect width="256" height="256" fill="none"/><circle cx="128" cy="128" r="96" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/><polyline points="128 72 128 128 184 128" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/></svg>"#.to_string()
}

fn build_user_svg() -> String {
    r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256"><rect width="256" height="256" fill="none"/><circle cx="128" cy="96" r="64" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/><path d="M32,216c19.37-33.47,54.55-56,96-56s76.63,22.53,96,56" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/></svg>"#.to_string()
}

fn build_branch_svg() -> String {
    r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256"><rect width="256" height="256" fill="none"/><path d="M80,168V144a16,16,0,0,1,16-16h88a16,16,0,0,0,16-16V88" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/><line x1="80" y1="88" x2="80" y2="168" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/><circle cx="80" cy="64" r="24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/><circle cx="200" cy="64" r="24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/><circle cx="80" cy="192" r="24" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/></svg>"#.to_string()
}

fn build_hash_svg() -> String {
    r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256"><rect width="256" height="256" fill="none"/><line x1="48" y1="96" x2="224" y2="96" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/><line x1="176" y1="40" x2="144" y2="216" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/><line x1="112" y1="40" x2="80" y2="216" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/><line x1="32" y1="160" x2="208" y2="160" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/></svg>"#.to_string()
}

fn build_advanced_send_tab(state: &UiState) -> ui::Element {
    let search_label = ui::Element::new(ui::ElementType::P, Some("搜索城市"))
        .size(16)
        .margin_bottom(8);

    let search_input = ui::Element::new(ui::ElementType::Input, Some(&state.search_query))
        .on(ui::Event::Change, SEARCH_INPUT_CHANGE_EVENT)
        .on(ui::Event::Input, SEARCH_INPUT_SUBMIT_EVENT)
        .radius(8)
        .bg("#2A2A2A")
        .height(INPUT_HEIGHT)
        .width_full()
        .margin_right(8);

    let search_button = build_search_inline_button(SEARCH_BUTTON_EVENT);

    let search_row = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .width_full()
        .margin_bottom(12)
        .child(search_input)
        .child(search_button);

    let recent_container = build_recent_locations(state);
    let results_container = build_location_results(state);

    let days_label = ui::Element::new(ui::ElementType::P, Some("同步天数"))
        .size(16)
        .margin_bottom(8);

    let days_row = build_days_row(state);

    let hourly_card = build_settings_card(
        hourly_sync_svg(),
        "同步逐小时天气数据",
        Some("开启后同步最近 7天逐小时天气"),
        Some(build_switch(
            state.sync_hourly_enabled,
            HOURLY_SYNC_TOGGLE_EVENT,
        )),
        None,
    )
    .margin_top(12)
    .margin_bottom(4);

    let send_button =
        build_icon_text_button_full("同步数据", send_tab_svg_blue(), SEND_BUTTON_EVENT)
            .bg("#0090FF26")
            .text_color("#0090FF")
            .margin_top(16);

    let root = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full();

    root.child(search_label)
        .child(search_row)
        .child(recent_container)
        .child(results_container)
        .child(days_label)
        .child(days_row)
        .child(hourly_card)
        .child(send_button)
}

fn hourly_sync_svg() -> String {
    r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256"><rect width="256" height="256" fill="none"/><path d="M128,32a96,96,0,1,0,96,96A96,96,0,0,0,128,32Zm40,104H128a8,8,0,0,1-8-8V72a8,8,0,0,1,16,0v48h32a8,8,0,0,1,0,16Z" fill="currentColor"/></svg>"#.to_string()
}

fn build_recent_locations(state: &UiState) -> ui::Element {
    let mut container = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .margin_bottom(8);

    if state.recent_locations.is_empty() {
        return container;
    }

    let label = ui::Element::new(ui::ElementType::P, Some("最近地点"))
        .size(14)
        .margin_bottom(8)
        .text_color("#BBBBBB");
    container = container.child(label);

    let mut row = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .margin_bottom(8);
    let mut count = 0usize;

    for (idx, item) in state.recent_locations.iter().enumerate() {
        let label = build_location_label(item);
        let is_selected = item.id == state.selected_location_id;
        let btn = build_location_chip(
            &label,
            location_pin_svg(is_selected),
            &format!("{}{}", SELECT_RECENT_PREFIX, idx),
            is_selected,
        );

        row = row.child(btn);
        count += 1;
        if count < 3 {
            row = row.child(ui::Element::new(ui::ElementType::Span, None).width(8));
        } else {
            container = container.child(row);
            row = ui::Element::new(ui::ElementType::Div, None)
                .flex()
                .flex_direction(ui::FlexDirection::Row)
                .align_center()
                .margin_bottom(8);
            count = 0;
        }
    }

    if count > 0 {
        container = container.child(row);
    }

    container
}

fn build_location_results(state: &UiState) -> ui::Element {
    let mut container = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .margin_bottom(16);

    if state.search_query.trim().is_empty() {
        return container;
    }

    if state.search_results.is_empty() {
        return container.child(
            ui::Element::new(ui::ElementType::P, Some("暂无搜索结果"))
                .size(13)
                .text_color("#888888"),
        );
    }

    let label = ui::Element::new(ui::ElementType::P, Some("具体地点"))
        .size(14)
        .margin_bottom(8)
        .text_color("#BBBBBB");
    container = container.child(label);

    let mut row = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .margin_bottom(8);
    let mut count = 0usize;

    for (idx, item) in state.search_results.iter().enumerate() {
        let label = build_location_label(item);
        let is_selected = item.id == state.selected_location_id;
        let btn = build_location_chip(
            &label,
            location_pin_svg(is_selected),
            &format!("{}{}", SELECT_LOCATION_PREFIX, idx),
            is_selected,
        );

        row = row.child(btn);
        count += 1;
        if count < 3 {
            row = row.child(ui::Element::new(ui::ElementType::Span, None).width(8));
        } else {
            container = container.child(row);
            row = ui::Element::new(ui::ElementType::Div, None)
                .flex()
                .flex_direction(ui::FlexDirection::Row)
                .align_center()
                .margin_bottom(8);
            count = 0;
        }
    }

    if count > 0 {
        container = container.child(row);
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
        let text = ui::Element::new(ui::ElementType::Span, Some(&format!("{}天", day))).size(14);

        let btn = ui::Element::new(ui::ElementType::Button, None)
            .without_default_styles()
            .on(ui::Event::Click, &format!("{}{}", SELECT_DAYS_PREFIX, day))
            .radius(999)
            .padding_top(8)
            .padding_bottom(8)
            .padding_left(16)
            .padding_right(16)
            .bg(if is_active { "#2A2A2A" } else { "#1E1E1F" })
            .text_color(if is_active { "#FFFFFF" } else { "#BBBBBB" })
            .child(text);

        row = row.child(btn);
        if i < options.len() - 1 {
            row = row.child(ui::Element::new(ui::ElementType::Span, None).width(6));
        }
    }

    row
}

pub fn rerender_main_ui() {
    let element_id = {
        let state = ui_state()
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.root_element_id.clone()
    };

    if let Some(element_id) = element_id {
        let ui_tree = build_main_ui();
        psys_host::ui::render(&element_id, ui_tree);
    }
}

const INPUT_HEIGHT: u32 = 40;
const SWITCH_W: u32 = 42;
const SWITCH_H: u32 = 24;

fn build_switch(is_on: bool, event_id: &str) -> ui::Element {
    let svg = if is_on {
        switch_on_svg()
    } else {
        switch_off_svg()
    };

    let icon = ui::Element::new(ui::ElementType::Svg, Some(&svg))
        .width(SWITCH_W)
        .height(SWITCH_H);

    ui::Element::new(ui::ElementType::Button, None)
        .without_default_styles()
        .on(ui::Event::Click, event_id)
        .width(SWITCH_W)
        .height(SWITCH_H)
        .child(icon)
}

fn build_settings_card(
    icon_svg: String,
    title: &str,
    desc: Option<&str>,
    right: Option<ui::Element>,
    click_event: Option<&str>,
) -> ui::Element {
    let icon = ui::Element::new(ui::ElementType::Svg, Some(&icon_svg))
        .width(24)
        .height(24)
        .text_color("#FFFFFF");

    let icon_wrap = ui::Element::new(ui::ElementType::Div, None)
        .width(24)
        .height(24)
        .flex()
        .align_center()
        .justify_center()
        .child(icon);

    let mut text_col = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full();

    let title_el = ui::Element::new(ui::ElementType::P, Some(title)).size(15);
    text_col = text_col.child(title_el);

    if let Some(desc_text) = desc {
        let desc_el = ui::Element::new(ui::ElementType::P, Some(desc_text))
            .size(13)
            .text_color("#888888");
        text_col = text_col.child(desc_el);
    }

    let mut row = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .width_full()
        .bg("#1E1E1F")
        .radius(24)
        .padding_left(12)
        .padding_right(12)
        .padding_top(10)
        .padding_bottom(10)
        .child(icon_wrap)
        .child(ui::Element::new(ui::ElementType::Span, None).width(10))
        .child(text_col);

    if let Some(right_el) = right {
        let right_wrap = ui::Element::new(ui::ElementType::Div, None)
            .flex()
            .align_center()
            .justify_end()
            .child(right_el);
        row = row.child(right_wrap);
    }

    if let Some(event_id) = click_event {
        row = row.on(ui::Event::Click, event_id);
    }

    row
}

fn build_section_title(text: &str) -> ui::Element {
    ui::Element::new(ui::ElementType::P, Some(text))
        .size(13)
        .text_color("#888888")
        .margin_left(15)
        .margin_bottom(8)
}

fn build_more_link_icon() -> ui::Element {
    let svg = more_link_svg();
    ui::Element::new(ui::ElementType::Svg, Some(&svg))
        .width(18)
        .height(18)
        .text_color("#0088FF")
}

fn more_link_svg() -> String {
    r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256"><rect width="256" height="256" fill="none"/><polyline points="216 104 215.99 40.01 152 40" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/><line x1="136" y1="120" x2="216" y2="40" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/><path d="M184,136v72a8,8,0,0,1-8,8H48a8,8,0,0,1-8-8V80a8,8,0,0,1,8-8h72" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/></svg>"#.to_string()
}

fn help_doc_svg() -> String {
    help_svg()
}

fn qq_group_svg() -> String {
    r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256"><rect width="256" height="256" fill="none"/><circle cx="84" cy="108" r="52" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/><path d="M10.23,200a88,88,0,0,1,147.54,0" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/><path d="M172,160a87.93,87.93,0,0,1,73.77,40" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/><path d="M152.69,59.7A52,52,0,1,1,172,160" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/></svg>"#.to_string()
}

fn afd_svg() -> String {
    r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256"><rect width="256" height="256" fill="none"/><path d="M48,208H16a8,8,0,0,1-8-8V160a8,8,0,0,1,8-8H48" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/><path d="M112,160h32l67-15.41a16.61,16.61,0,0,1,21,16h0a16.59,16.59,0,0,1-9.18,14.85L184,192l-64,16H48V152l25-25a24,24,0,0,1,17-7H140a20,20,0,0,1,20,20h0a20,20,0,0,1-20,20Z" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/><path d="M96.73,120C87,107.72,80,94.56,80,80c0-21.69,17.67-40,39.46-40A39.12,39.12,0,0,1,156,64a39.12,39.12,0,0,1,36.54-24C214.33,40,232,58.31,232,80c0,29.23-28.18,55.07-50.22,71.32" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/></svg>"#.to_string()
}

fn location_pin_svg(selected: bool) -> String {
    if selected {
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256"><rect width="256" height="256" fill="none"/><line x1="128" y1="240" x2="128" y2="208" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/><circle cx="128" cy="128" r="80" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/><line x1="128" y1="16" x2="128" y2="48" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/><line x1="16" y1="128" x2="48" y2="128" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/><line x1="240" y1="128" x2="208" y2="128" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/><circle cx="128" cy="128" r="32" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/></svg>"#.to_string()
    } else {
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256"><rect width="256" height="256" fill="none"/><line x1="128" y1="240" x2="128" y2="208" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/><circle cx="128" cy="128" r="80" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/><line x1="128" y1="16" x2="128" y2="48" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/><line x1="16" y1="128" x2="48" y2="128" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/><line x1="240" y1="128" x2="208" y2="128" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/></svg>"#.to_string()
    }
}

fn switch_off_svg() -> String {
    r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" fill="none" version="1.1" width="35" height="20" viewBox="0 0 35 20"><rect x="0" y="0" width="35" height="20" rx="10" fill="#00000F" fill-opacity="0.30000001192092896"/><rect x="0.5" y="0.5" width="34" height="19" rx="9.5" fill-opacity="0" stroke-opacity="0.15000000596046448" stroke="#FFFFFF" fill="none" stroke-width="1"/><ellipse cx="10" cy="10" rx="9" ry="9" fill="#FFFFFF" fill-opacity="1"/></svg>"##.to_string()
}

fn switch_on_svg() -> String {
    r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" fill="none" version="1.1" width="35" height="20" viewBox="0 0 35 20"><rect x="0" y="0" width="35" height="20" rx="10" fill="#0090FF" fill-opacity="1"/><rect x="0.5" y="0.5" width="34" height="19" rx="9.5" fill-opacity="0" stroke-opacity="0.15000000596046448" stroke="#FFFFFF" fill="none" stroke-width="1"/><ellipse cx="25" cy="10" rx="9" ry="9" fill="#FFFFFF" fill-opacity="1"/></svg>"##.to_string()
}

fn build_tab_button(label: &str, icon_svg: String, is_active: bool, event_id: &str) -> ui::Element {
    let icon = ui::Element::new(ui::ElementType::Svg, Some(&icon_svg))
        .width(22)
        .height(22);

    let text = ui::Element::new(ui::ElementType::Span, Some(label)).size(14);

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

fn short_git_hash(hash: &str) -> String {
    let trimmed = hash.trim();
    if trimmed.is_empty() || trimmed == "unknown" {
        return "unknown".to_string();
    }
    trimmed.chars().take(7).collect()
}

fn build_icon_text_button_full(label: &str, icon_svg: String, event_id: &str) -> ui::Element {
    let icon = ui::Element::new(ui::ElementType::Svg, Some(&icon_svg))
        .width(24)
        .height(24);

    let text = ui::Element::new(ui::ElementType::Span, Some(label)).size(14);

    ui::Element::new(ui::ElementType::Button, None)
        .without_default_styles()
        .on(ui::Event::Click, event_id)
        .radius(18)
        .padding(14)
        .bg("#2A2A2A")
        .width_full()
        .flex()
        .align_center()
        .child(icon)
        .child(ui::Element::new(ui::ElementType::Span, None).width(6))
        .child(text)
}

fn build_search_inline_button(event_id: &str) -> ui::Element {
    let icon = ui::Element::new(ui::ElementType::Svg, Some(&search_svg()))
        .width(16)
        .height(16);

    ui::Element::new(ui::ElementType::Button, None)
        .without_default_styles()
        .on(ui::Event::Click, event_id)
        .radius(18)
        .height(INPUT_HEIGHT)
        .padding_left(10)
        .padding_right(10)
        .bg("#2A2A2A")
        .width(44)
        .flex()
        .align_center()
        .justify_center()
        .child(icon)
}

fn build_location_chip(
    label: &str,
    icon_svg: String,
    event_id: &str,
    selected: bool,
) -> ui::Element {
    let icon = ui::Element::new(ui::ElementType::Svg, Some(&icon_svg))
        .width(16)
        .height(16);

    let text = ui::Element::new(ui::ElementType::Span, Some(label)).size(14);

    ui::Element::new(ui::ElementType::Button, None)
        .without_default_styles()
        .on(ui::Event::Click, event_id)
        .radius(18)
        .padding_top(8)
        .padding_bottom(8)
        .padding_left(12)
        .padding_right(12)
        .bg(if selected { "#0090FF26" } else { "#1E1E1F" })
        .text_color(if selected { "#0090FF" } else { "#FFFFFF" })
        .flex()
        .align_center()
        .child(icon)
        .child(ui::Element::new(ui::ElementType::Span, None).width(6))
        .child(text)
}

fn build_location_label(item: &LocationOption) -> String {
    if item.name.trim().is_empty() {
        if !item.lon.is_empty() && !item.lat.is_empty() {
            return format!("{}, {}", item.lon, item.lat);
        }
        return "未知地点".to_string();
    }
    if item.adm1.is_empty() && item.adm2.is_empty() {
        item.name.clone()
    } else {
        format!("{} · {} {}", item.name, item.adm1, item.adm2)
            .trim()
            .to_string()
    }
}

fn search_svg() -> String {
    r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256"><rect width="256" height="256" fill="none"/><circle cx="112" cy="112" r="80" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/><line x1="168.57" y1="168.57" x2="224" y2="224" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/></svg>"#.to_string()
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

fn send_tab_svg_blue() -> String {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256">
<rect width="256" height="256" fill="none"/>
<path d="M191.11,112.89c24-24,25.5-52.55,24.75-65.28a8,8,0,0,0-7.47-7.47c-12.73-.75-41.26.73-65.28,24.75L80,128l48,48Z" fill="none" stroke="#0090FF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
<path d="M136,72H74.35a8,8,0,0,0-5.65,2.34L34.35,108.69a8,8,0,0,0,4.53,13.57L80,128" fill="none" stroke="#0090FF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
<path d="M184,120v61.65a8,8,0,0,1-2.34,5.65l-34.35,34.35a8,8,0,0,1-13.57-4.53L128,176" fill="none" stroke="#0090FF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
<path d="M94.56,187.82C90.69,196.31,77.65,216,40,216c0-37.65,19.69-50.69,28.18-54.56" fill="none" stroke="#0090FF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
</svg>"##
        .to_string()
}

fn help_svg() -> String {
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256">
<rect width="256" height="256" fill="none"/>
<circle cx="128" cy="180" r="12" fill="#FFFFFF"/>
<path d="M128,144v-8c17.67,0,32-12.54,32-28s-14.33-28-32-28S96,92.54,96,108v4" fill="none" stroke="#FFFFFF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
<circle cx="128" cy="128" r="96" fill="none" stroke="#FFFFFF" stroke-linecap="round" stroke-linejoin="round" stroke-width="16"/>
</svg>"##
        .to_string()
}
