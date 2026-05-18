use super::SYNC_CARD_ID;
use super::event_handler::*;
use super::icons;
use super::state::*;
use crate::astrobox::psys_host;
use crate::astrobox::psys_host::ui_v3 as ui;
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
    psys_host::ui_v3::render(element_id, ui_tree);
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
    psys_host::ui_v3::render_to_text_card(card_id, &text);
}

fn build_tabs(state: &UiState) -> ui::Element {
    let tabs_root = ui::Element::new(ui::ElementType::TabsRoot, None)
        .flex()
        .justify_center()
        .margin_bottom(20);

    let tabs_list = ui::Element::new(ui::ElementType::TabsList, None)
        .flex()
        .bg("#1E1E1F")
        .radius(999)
        .padding(4)
        .gap(4);

    let paste_trigger = build_tab_trigger(
        "同步数据",
        icons::send_tab_svg(),
        state.current_tab == MainTab::PasteData,
        TAB_PASTE_EVENT,
    );

    let settings_trigger = build_tab_trigger(
        "设置",
        icons::api_tab_svg(),
        state.current_tab == MainTab::Settings,
        TAB_SETTINGS_EVENT,
    );

    tabs_root.child(tabs_list.child(paste_trigger).child(settings_trigger))
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
        .width_full()
        .gap(8);

    let more_title = build_section_title("更多内容");
    root = root.child(more_title);

    let afd_card = build_settings_card(
        icons::afd_svg(),
        "支持本项目",
        Some("赞助可帮助维护天气服务并支持后续功能开发"),
        Some(build_more_link_icon()),
        Some(OPEN_AFD_EVENT),
    );

    let help_card = build_settings_card(
        icons::help_svg(),
        "帮助文档",
        Some("操作步骤与常见问题解答"),
        Some(build_more_link_icon()),
        Some(OPEN_HELP_DOC_EVENT),
    );

    let qq_card = build_settings_card(
        icons::qq_group_svg(),
        "QQ交流群",
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
        icons::time_svg(),
        "构建时间",
        None,
        Some(build_value_text(&build_time)),
        None,
    );
    let build_user_row = build_settings_card(
        icons::user_svg(),
        "构建用户",
        None,
        Some(build_value_text(build_user)),
        None,
    );
    let build_branch_row = build_settings_card(
        icons::branch_svg(),
        "当前分支",
        None,
        Some(build_value_text(build_branch)),
        None,
    );
    let build_hash_row = build_settings_card(
        icons::hash_svg(),
        "当前hash",
        None,
        Some(build_value_text(&build_hash)),
        None,
    );

    root.child(afd_card)
        .child(help_card)
        .child(qq_card.margin_bottom(10))
        .child(build_title)
        .child(build_time_row)
        .child(build_user_row)
        .child(build_branch_row)
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
        "地区: {}\n时间: {}{}",
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

fn build_advanced_send_tab(state: &UiState) -> ui::Element {
    let search_label = ui::Element::new(ui::ElementType::P, Some("搜索城市"))
        .size(15)
        .margin_left(12);

    let search_input = ui::Element::new(ui::ElementType::Input, Some(&state.search_query))
        .on(ui::Event::Change, SEARCH_INPUT_CHANGE_EVENT)
        .on(ui::Event::Input, SEARCH_INPUT_SUBMIT_EVENT)
        .radius(18)
        .bg("#2A2A2A")
        .height(INPUT_HEIGHT)
        .width_full()
        .padding_left(8)
        .padding_right(8);

    let search_button = build_search_inline_button(SEARCH_BUTTON_EVENT);

    let search_row = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .width_full()
        .gap(8)
        .child(search_input)
        .child(search_button);

    let has_recent = !state.recent_locations.is_empty();
    let has_search = !state.search_query.trim().is_empty();
    let recent_container = build_recent_locations(state);
    let results_container = build_location_results(state);

    let days_card = build_days_card(state).margin_top(10);

    let hourly_card = build_settings_card(
        icons::hourly_sync_svg(),
        "同步逐小时天气数据",
        Some("开启后同步最近一周逐小时天气"),
        Some(build_switch(
            state.sync_hourly_enabled,
            HOURLY_SYNC_TOGGLE_EVENT,
        )),
        None,
    );

    let alerts_card = build_settings_card(
        icons::alerts_svg(),
        "同步天气预警数据",
        Some("开启后同步天气预警灾害信息"),
        Some(build_switch(
            state.sync_alerts_enabled,
            ALERTS_SYNC_TOGGLE_EVENT,
        )),
        None,
    )
    .margin_bottom(10);

    let send_button =
        build_icon_text_button_full("同步数据", icons::send_tab_svg(), SEND_BUTTON_EVENT)
            .bg("#0090FF26")
            .text_color("#0090FF");

    let mut root = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .gap(8);

    root = root.child(search_label).child(search_row);
    if has_recent {
        root = root.child(recent_container);
    }
    if has_search {
        root = root.child(results_container);
    }
    root.child(days_card)
        .child(hourly_card)
        .child(alerts_card)
        .child(send_button)
}

fn build_recent_locations(state: &UiState) -> ui::Element {
    if state.recent_locations.is_empty() {
        return ui::Element::new(ui::ElementType::Div, None);
    }

    let mut grid = ui::Element::new(ui::ElementType::Grid, None)
        .grid_template_columns("1fr 1fr 1fr")
        .gap(8)
        .width_full();

    for (idx, item) in state.recent_locations.iter().enumerate() {
        let label = build_location_label(item);
        let is_selected = item.id == state.selected_location_id;
        let pin_svg = if is_selected { icons::location_pin_filled_svg() } else { icons::location_pin_svg() };
        let btn = build_location_chip(
            &label,
            pin_svg,
            &format!("{}{}", SELECT_RECENT_PREFIX, idx),
            is_selected,
        );
        grid = grid.child(btn);
    }

    grid
}

fn build_location_results(state: &UiState) -> ui::Element {
    if state.search_query.trim().is_empty() {
        return ui::Element::new(ui::ElementType::Div, None);
    }

    if state.search_results.is_empty() {
        return ui::Element::new(ui::ElementType::P, Some("暂无搜索结果"))
            .size(13)
            .text_color("#888888");
    }

    let mut grid = ui::Element::new(ui::ElementType::Grid, None)
        .grid_template_columns("1fr 1fr 1fr")
        .gap(8)
        .width_full();

    for (idx, item) in state.search_results.iter().enumerate() {
        let label = build_location_label(item);
        let is_selected = item.id == state.selected_location_id;
        let pin_svg = if is_selected { icons::location_pin_filled_svg() } else { icons::location_pin_svg() };
        let btn = build_location_chip(
            &label,
            pin_svg,
            &format!("{}{}", SELECT_LOCATION_PREFIX, idx),
            is_selected,
        );
        grid = grid.child(btn);
    }

    grid
}

fn build_days_card(state: &UiState) -> ui::Element {
    let selected_text = format!("{}天", state.selected_days);
    let options = [3u32, 7, 10, 15, 30];

    let mut select = ui::Element::new(ui::ElementType::Select, Some(&selected_text))
        .on(ui::Event::Change, DAYS_DROPDOWN_EVENT)
        .radius(8)
        .padding_left(12)
        .padding_right(12)
        .bg("#2A2A2A")
        .size(14);

    for day in options.iter() {
        let option_text = format!("{}天", day);
        let option = ui::Element::new(ui::ElementType::Option, Some(&option_text));
        select = select.child(option);
    }

    build_settings_card(
        icons::calendar_svg(),
        "同步天气天数",
        None,
        Some(select),
        None,
    )
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
        psys_host::ui_v3::render(&element_id, ui_tree);
    }
}

const INPUT_HEIGHT: u32 = 40;

fn build_switch(is_on: bool, event_id: &str) -> ui::Element {
    ui::Element::new(ui::ElementType::Switch, None)
        .on(ui::Event::Change, event_id)
        .prop("checked", if is_on { "true" } else { "false" })
}

fn build_settings_card(
    icon_svg: String,
    title: &str,
    desc: Option<&str>,
    right: Option<ui::Element>,
    click_event: Option<&str>,
) -> ui::Element {
    let icon = ui::Element::new(ui::ElementType::Svg, Some(&icon_svg))
        .width(22)
        .height(22)
        .text_color("#FFFFFF");

    let icon_wrap = ui::Element::new(ui::ElementType::Div, None)
        .width(22)
        .height(22)
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
        .radius(18)
        .padding_left(12)
        .padding_right(12)
        .padding_top(10)
        .padding_bottom(10)
        .gap(10)
        .child(icon_wrap)
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
        .margin_left(12)
}

fn build_more_link_icon() -> ui::Element {
    let svg = icons::more_link_svg();
    ui::Element::new(ui::ElementType::Svg, Some(&svg))
        .width(18)
        .height(18)
        .text_color("#0088FF")
}


fn build_tab_trigger(label: &str, icon_svg: String, is_active: bool, event_id: &str) -> ui::Element {
    let icon = ui::Element::new(ui::ElementType::Svg, Some(&icon_svg))
        .width(22)
        .height(22);

    let text = ui::Element::new(ui::ElementType::Span, Some(label)).size(14);

    ui::Element::new(ui::ElementType::TabsTrigger, None)
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
        .gap(5)
        .child(icon)
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
        .width(22)
        .height(22);

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
        .gap(8)
        .child(icon)
        .child(text)
}

fn build_search_inline_button(event_id: &str) -> ui::Element {
    let icon = ui::Element::new(ui::ElementType::Svg, Some(&icons::search_svg()))
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
        .gap(6)
        .child(icon)
        .child(text)
}

fn build_location_label(item: &LocationOption) -> String {
    if item.name.trim().is_empty() {
        if !item.lon.is_empty() && !item.lat.is_empty() {
            return format!("{}, {}", item.lon, item.lat);
        }
        return "未知地区".to_string();
    }
    if item.adm1.is_empty() && item.adm2.is_empty() {
        item.name.clone()
    } else {
        format!("{} · {} {}", item.name, item.adm1, item.adm2)
            .trim()
            .to_string()
    }
}
