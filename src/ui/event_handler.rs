use super::state::*;
use crate::astrobox::psys_host_v4::device;
use crate::astrobox::psys_host_v4::dialog;
use crate::astrobox::psys_host_v4::interconnect;
use crate::astrobox::psys_host_v4::register;
use crate::astrobox::psys_host_v4::thirdpartyapp;
use crate::astrobox::psys_host_v4::ui;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use url::Url;

pub const SEND_BUTTON_EVENT: &str = "send_button";
pub const CANCEL_SEND_EVENT: &str = "cancel_send";
pub const TAB_PASTE_EVENT: &str = "tab_paste";
pub const TAB_SETTINGS_EVENT: &str = "tab_settings";
pub const HOURLY_SYNC_TOGGLE_EVENT: &str = "hourly_sync_toggle";
pub const ALERTS_SYNC_TOGGLE_EVENT: &str = "alerts_sync_toggle";
pub const OPEN_HELP_DOC_EVENT: &str = "open_help_doc";
pub const OPEN_QQ_GROUP_EVENT: &str = "open_qq_group";
pub const OPEN_AFD_EVENT: &str = "open_afd";
pub const SEARCH_INPUT_CHANGE_EVENT: &str = "search_input_change";
pub const SEARCH_BUTTON_EVENT: &str = "search_button";
pub const CANCEL_SEARCH_EVENT: &str = "cancel_search";
pub const OPEN_LOCATION_PICKER_EVENT: &str = "open_location_picker";
pub const CLOSE_LOCATION_PICKER_EVENT: &str = "close_location_picker";
pub const SEARCH_INPUT_SUBMIT_EVENT: &str = "search_input_submit";
pub const SELECT_LOCATION_PREFIX: &str = "select_location:";
pub const SELECT_RECENT_PREFIX: &str = "select_recent:";
pub const DAYS_DROPDOWN_EVENT: &str = "days_dropdown";

const WEATHER_SYNC_HOURLY_RANGE: &str = "168h";

pub fn handle_interconnect_message(payload: &str) {
    tracing::info!("收到快应用消息: {}", payload);
}

pub fn handle_timer_payload(payload: &str) {
    tracing::info!("timer payload: {}", payload);
}

pub async fn ui_event_processor(event_type: ui::Event, event_id: &str, event_payload: &str) {
    if !is_high_frequency_input_event(event_id) {
        tracing::info!("UI Event: type={:?}, id={}", event_type, event_id);
    }

    match event_id {
        SEND_BUTTON_EVENT => {
            tracing::info!("SEND_BUTTON_EVENT received");
            send_weather_data().await;
        }
        CANCEL_SEND_EVENT => {
            cancel_send();
        }
        TAB_PASTE_EVENT => {
            let should_rerender = {
                let mut state = ui_state()
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if state.current_tab != MainTab::PasteData {
                    state.current_tab = MainTab::PasteData;
                    true
                } else {
                    false
                }
            };
            if should_rerender {
                crate::ui::build::rerender_main_ui();
                resolve_recent_locations_if_needed();
            }
        }
        TAB_SETTINGS_EVENT => {
            let should_rerender = {
                let mut state = ui_state()
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if state.current_tab != MainTab::Settings {
                    state.current_tab = MainTab::Settings;
                    true
                } else {
                    false
                }
            };
            if should_rerender {
                crate::ui::build::rerender_main_ui();
            }
        }
        OPEN_HELP_DOC_EVENT => {
            open_help_doc_page();
        }
        OPEN_QQ_GROUP_EVENT => {
            open_qq_group_page();
        }
        OPEN_AFD_EVENT => {
            open_afd_page();
        }
        HOURLY_SYNC_TOGGLE_EVENT => {
            {
                let mut state = ui_state()
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                state.sync_hourly_enabled = !state.sync_hourly_enabled;
            }
            let _ = crate::ui::state::save_all_settings();
            crate::ui::build::rerender_main_ui();
        }
        ALERTS_SYNC_TOGGLE_EVENT => {
            {
                let mut state = ui_state()
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                state.sync_alerts_enabled = !state.sync_alerts_enabled;
            }
            let _ = crate::ui::state::save_all_settings();
            crate::ui::build::rerender_main_ui();
        }
        SEARCH_INPUT_CHANGE_EVENT => {
            let parsed_value = parse_event_value(event_payload);
            let mut state = ui_state()
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.search_query = parsed_value;
        }
        SEARCH_INPUT_SUBMIT_EVENT => {
            let parsed_value = parse_event_value(event_payload);
            {
                let mut state = ui_state()
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                state.search_query = parsed_value;
            }
            if payload_has_enter(event_payload) {
                search_locations().await;
            }
        }
        SEARCH_BUTTON_EVENT => {
            search_locations().await;
        }
        CANCEL_SEARCH_EVENT => {
            let mut state = ui_state()
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.search_query.clear();
            state.search_results.clear();
            drop(state);
            crate::ui::build::rerender_main_ui();
        }
        OPEN_LOCATION_PICKER_EVENT => {
            {
                let mut state = ui_state()
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                state.show_location_picker = true;
            }
            crate::ui::build::rerender_main_ui();
        }
        CLOSE_LOCATION_PICKER_EVENT => {
            {
                let mut state = ui_state()
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                state.show_location_picker = false;
                state.search_query.clear();
                state.search_results.clear();
            }
            crate::ui::build::rerender_main_ui();
        }

        DAYS_DROPDOWN_EVENT => {
            let parsed_value = parse_event_value(event_payload);
            if let Some(day_str) = parsed_value.strip_suffix('天') {
                if let Ok(day) = day_str.trim().parse::<u32>() {
                    select_days(day);
                }
            }
        }

        _ => {}
    }

    if event_id.starts_with(SELECT_LOCATION_PREFIX) {
        if let Some(idx_str) = event_id.strip_prefix(SELECT_LOCATION_PREFIX) {
            if let Ok(idx) = idx_str.parse::<usize>() {
                select_location(idx);
            }
        }
    }
    if event_id.starts_with(SELECT_RECENT_PREFIX) {
        if let Some(idx_str) = event_id.strip_prefix(SELECT_RECENT_PREFIX) {
            if let Ok(idx) = idx_str.parse::<usize>() {
                select_recent_location(idx);
            }
        }
    }
}

fn payload_has_enter(payload: &str) -> bool {
    payload.contains("\"key\":\"Enter\"")
        || payload.contains("\"code\":\"Enter\"")
        || payload.contains("\"keyCode\":13")
        || payload.contains("\"which\":13")
        || payload.contains("Enter")
}

fn is_high_frequency_input_event(event_id: &str) -> bool {
    matches!(
        event_id,
        SEARCH_INPUT_CHANGE_EVENT | SEARCH_INPUT_SUBMIT_EVENT
    )
}

fn parse_event_value(payload: &str) -> String {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(payload) {
        if let Some(value) = extract_event_value(&json) {
            return value.trim().to_string();
        }
        tracing::warn!("parse_event_value parsed JSON but found no usable text field");
        String::new()
    } else {
        payload.trim().to_string()
    }
}

fn extract_event_value(value: &serde_json::Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    let preferred_keys = ["value", "text", "content", "label"];
    for key in preferred_keys {
        if let Some(text) = value.get(key).and_then(|v| v.as_str()) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    let nested_keys = ["detail", "target", "currentTarget", "data"];
    for key in nested_keys {
        if let Some(nested) = value.get(key) {
            if let Some(text) = extract_event_value(nested) {
                return Some(text);
            }
        }
    }

    None
}

enum SendError {
    Cancelled,
    Failed { title: &'static str, message: String },
}

impl SendError {
    fn failed(title: &'static str, message: impl Into<String>) -> Self {
        SendError::Failed {
            title,
            message: message.into(),
        }
    }
}

struct SendRequest {
    location: CityLocation,
    days: u32,
    sync_hourly_enabled: bool,
    sync_alerts_enabled: bool,
}

async fn send_weather_data() {
    let (location, days, sync_hourly_enabled, sync_alerts_enabled, selected_from_search) = {
        let state = ui_state()
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        (
            state.selected_location.clone(),
            state.selected_days,
            state.sync_hourly_enabled,
            state.sync_alerts_enabled,
            state.selected_from_search,
        )
    };

    let Some(location) = location else {
        show_alert("提示", "请先选择位置").await;
        return;
    };

    let Some(generation) = begin_send() else {
        tracing::info!("send already in progress, ignoring click");
        return;
    };

    let request = SendRequest {
        location,
        days,
        sync_hourly_enabled,
        sync_alerts_enabled,
    };
    let result = run_send(generation, request).await;

    if !finish_send(generation) {
        tracing::info!("send #{} cancelled", generation);
        return;
    }

    match result {
        Ok(()) => {
            if selected_from_search {
                clear_search_after_sync();
            }
            show_alert("成功", "发送成功").await;
        }
        Err(SendError::Cancelled) => {}
        Err(SendError::Failed { title, message }) => show_alert(title, &message).await,
    }
}

async fn run_send(generation: u64, request: SendRequest) -> Result<(), SendError> {
    let SendRequest {
        location,
        days,
        sync_hourly_enabled,
        sync_alerts_enabled,
    } = request;

    let sync_location_id = ensure_sync_location_id(&location.id)
        .map_err(|message| SendError::failed("提示", message))?;
    check_send_active(generation)?;

    let mut modules = serde_json::Map::new();
    modules.insert("daily".to_string(), serde_json::Value::String(days_to_api_segment(days).to_string()));
    if sync_hourly_enabled {
        modules.insert("hourly".to_string(), serde_json::Value::String(WEATHER_SYNC_HOURLY_RANGE.to_string()));
    }
    if sync_alerts_enabled {
        modules.insert("alerts".to_string(), serde_json::Value::Bool(true));
    }

    let payload_json = serde_json::json!({
        "locationId": sync_location_id,
        "modules": modules,
    });

    let sync_url = api_url("/api/weather/sync").map_err(|e| SendError::failed("错误", e))?;

    let mut json = super::api_client::post_json(&sync_url, &payload_json)
        .map_err(|e| SendError::failed("失败", format!("获取天气失败: {}", e)))?;
    check_send_active(generation)?;

    json["location"] = serde_json::Value::String(location.name.clone());
    let payload = json.to_string();
    mark_sync_started(&payload, &location);
    send_via_interconnect(generation, &payload).await
}

/// 开始一次发送并返回其代号；已有发送在进行时返回 `None`。
fn begin_send() -> Option<u64> {
    let generation = {
        let mut state = ui_state()
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.send_in_progress {
            return None;
        }
        state.send_generation += 1;
        state.send_in_progress = true;
        state.send_status = "正在获取天气数据…".to_string();
        state.sync_card_backup = None;
        state.send_generation
    };
    crate::ui::build::rerender_main_ui();
    Some(generation)
}

/// 结束发送并恢复按钮；这次发送已被取消（或被新的发送取代）时返回 `false`，不改动状态。
fn finish_send(generation: u64) -> bool {
    {
        let mut state = ui_state()
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.send_generation != generation {
            return false;
        }
        state.send_in_progress = false;
        state.send_status.clear();
        state.sync_card_backup = None;
    }
    crate::ui::build::rerender_main_ui();
    true
}

fn cancel_send() {
    let restored_card = {
        let mut state = ui_state()
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !state.send_in_progress {
            return;
        }
        state.send_generation += 1;
        state.send_in_progress = false;
        state.send_status.clear();
        match state.sync_card_backup.take() {
            Some((time_ms, location)) => {
                state.last_sync_time_ms = time_ms;
                state.last_sync_location = location;
                true
            }
            None => false,
        }
    };
    tracing::info!("send cancelled by user");
    if restored_card {
        crate::ui::render_sync_card(crate::ui::SYNC_CARD_ID);
    }
    crate::ui::build::rerender_main_ui();
}

fn check_send_active(generation: u64) -> Result<(), SendError> {
    let state = ui_state()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if state.send_generation == generation {
        Ok(())
    } else {
        Err(SendError::Cancelled)
    }
}

/// 更新发送进度文字，同时作为取消检查点。
fn set_send_stage(generation: u64, status: &str) -> Result<(), SendError> {
    {
        let mut state = ui_state()
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.send_generation != generation {
            return Err(SendError::Cancelled);
        }
        state.send_status = status.to_string();
    }
    crate::ui::build::rerender_main_ui();
    Ok(())
}

fn mark_sync_started(payload: &str, location: &CityLocation) {
    let location_name = if !location.name.trim().is_empty() {
        location.name.trim().to_string()
    } else {
        extract_location_from_payload(payload).unwrap_or_default()
    };

    let mut state = ui_state()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if state.sync_card_backup.is_none() {
        state.sync_card_backup = Some((state.last_sync_time_ms, state.last_sync_location.clone()));
    }
    state.last_sync_time_ms = now_ms();
    if !location_name.is_empty() {
        state.last_sync_location = location_name;
    }
    drop(state);
    crate::ui::render_sync_card(crate::ui::SYNC_CARD_ID);
}

fn ensure_sync_location_id(location_id: &str) -> Result<String, String> {
    let trimmed_id = location_id.trim();
    if !trimmed_id.is_empty() && !trimmed_id.contains(',') {
        return Ok(trimmed_id.to_string());
    }

    if trimmed_id.contains(',') {
        let location = fetch_first_location(trimmed_id)?;
        if location.id.trim().is_empty() {
            return Err("位置解析失败".to_string());
        }
        return Ok(location.id);
    }

    Err("请先选择位置".to_string())
}

fn api_url(path: &str) -> Result<String, String> {
    Ok(format!(
        "{}{}",
        server_api_base()?.trim_end_matches('/'),
        if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{}", path)
        }
    ))
}

fn days_to_api_segment(days: u32) -> &'static str {
    match days {
        3 => "3d",
        7 => "7d",
        10 => "10d",
        15 => "15d",
        30 => "30d",
        _ => "7d",
    }
}

async fn send_via_interconnect(generation: u64, data: &str) -> Result<(), SendError> {
    let fail = |reason: String| SendError::failed("失败", format!("发送失败: {}", reason));

    tracing::info!("send_via_interconnect start");
    set_send_stage(generation, "正在连接设备…")?;

    let devices = device::get_connected_device_list().await;
    tracing::info!(
        "get_connected_device_list returned {} devices",
        devices.len()
    );
    check_send_active(generation)?;

    let Some(first_device) = devices.first() else {
        return Err(fail("没有连接的设备".to_string()));
    };
    let device_addr = first_device.addr.clone();

    tracing::info!("using device: {}", device_addr);

    let pkg_name = "com.application.zaona.weather";

    set_send_stage(generation, "正在检查快应用…")?;
    tracing::info!("checking if quick app is installed...");
    match check_quick_app_installed(&device_addr, pkg_name).await {
        Ok(false) => {
            return Err(fail("请先安装简明天气快应用".to_string()));
        }
        Err(e) => {
            tracing::warn!("failed to check app list: {}, assuming app exists", e);
        }
        Ok(true) => {
            tracing::info!("quick app is installed");
        }
    }
    check_send_active(generation)?;

    tracing::info!(
        "ensuring interconnect is registered for device: {}",
        device_addr
    );
    let reg_result =
        register::register_interconnect_recv(device_addr.clone(), pkg_name.to_string()).await;
    tracing::info!("register_interconnect_recv result: {:?}", reg_result);
    if let Err(e) = reg_result {
        return Err(fail(format!("register_interconnect_recv failed: {}", e)));
    }

    set_send_stage(generation, "正在启动快应用…")?;
    tracing::info!("launching quick app before send...");
    ensure_quick_app_launched(&device_addr, pkg_name, "/index")
        .await
        .map_err(fail)?;
    check_send_active(generation)?;

    tracing::info!("waiting 2s for quick app to be ready...");
    crate::sleep(Duration::from_secs(2)).await;

    set_send_stage(generation, "正在发送数据…")?;
    tracing::info!("sending weather data via interconnect");
    interconnect::send_qaic_message(device_addr, pkg_name.to_string(), data.to_string())
        .await
        .map_err(fail)?;

    Ok(())
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn extract_location_from_payload(data: &str) -> Option<String> {
    let json = serde_json::from_str::<serde_json::Value>(data).ok()?;
    json.get("location")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

async fn check_quick_app_installed(device_addr: &str, pkg_name: &str) -> Result<bool, String> {
    tracing::info!("checking for package: {}", pkg_name);

    match thirdpartyapp::get_thirdparty_app_list(device_addr.to_string()).await {
        Ok(app_list) => {
            tracing::info!("found {} apps", app_list.len());
            for app in &app_list {
                tracing::info!("  - {} ({})", app.app_name, app.package_name);
            }

            let found = app_list.iter().any(|app| app.package_name == pkg_name);
            tracing::info!("app {} found: {}", pkg_name, found);
            Ok(found)
        }
        Err(e) => {
            tracing::error!("failed to get app list: {}", e);
            Err(e)
        }
    }
}

async fn ensure_quick_app_launched(
    device_addr: &str,
    pkg_name: &str,
    page_name: &str,
) -> Result<(), String> {
    tracing::info!(
        "ensure_quick_app_launched: pkg={}, page={}",
        pkg_name,
        page_name
    );

    let app_list = thirdpartyapp::get_thirdparty_app_list(device_addr.to_string()).await?;

    let Some(app) = app_list.into_iter().find(|app| app.package_name == pkg_name) else {
        return Err("请先安装简明天气快应用".to_string());
    };

    thirdpartyapp::launch_qa(device_addr.to_string(), app, page_name.to_string()).await?;

    tracing::info!("quick app launched");
    Ok(())
}

fn open_help_doc_page() {
    tracing::info!("open_help_doc_page called");
    let url = "https://weather.zaona.top/";
    dialog::open_url(url);
    tracing::info!("opened help doc page: {}", url);
}

fn open_qq_group_page() {
    tracing::info!("open_qq_group_page called");
    let url = "https://qm.qq.com/q/njSLR4VNja";
    dialog::open_url(url);
    tracing::info!("opened qq group page: {}", url);
}

fn open_afd_page() {
    tracing::info!("open_afd_page called");
    let url = "https://afdian.com/a/zaona";
    dialog::open_url(url);
    tracing::info!("opened afd page: {}", url);
}

async fn show_alert(title: &str, message: &str) {
    tracing::info!("show_alert: title={}, message={}", title, message);

    tracing::info!("show_alert executing dialog::show_dialog (Website style)");
    let _ = dialog::show_dialog(
        dialog::DialogType::Alert,
        dialog::DialogStyle::Website,
        dialog::DialogInfo {
            title: title.to_string(),
            content: message.to_string(),
            buttons: vec![dialog::DialogButton {
                id: "ok".to_string(),
                primary: true,
                content: "确定".to_string(),
            }],
        },
    )
    .await;
    tracing::info!("alert dialog closed");
}

async fn search_locations() {
    let query = {
        let state = ui_state()
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.search_query.clone()
    };

    if query.trim().is_empty() {
        show_alert("提示", "请输入城市名称").await;
        return;
    }

    let base = match api_url("/api/geo/lookup") {
        Ok(url) => url,
        Err(e) => {
            show_alert("错误", &e).await;
            return;
        }
    };
    let url = match Url::parse_with_params(&base, &[("location", query.trim())]) {
        Ok(u) => u.to_string(),
        Err(e) => {
            show_alert("错误", &format!("URL解析失败: {}", e)).await;
            return;
        }
    };

    match super::api_client::get_json(&url) {
        Ok(json) => {
            let mut results: Vec<CityLocation> = Vec::new();
            if let Some(list) = json.get("location").and_then(|v| v.as_array()) {
                for item in list {
                    let id = item
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = item
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let adm1 = item
                        .get("adm1")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let adm2 = item
                        .get("adm2")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if !id.is_empty() {
                        results.push(CityLocation {
                            id,
                            name,
                            adm1,
                            adm2,
                        });
                    }
                }
            }
            {
                let mut state = ui_state()
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                state.search_results = results;
            }
            crate::ui::build::rerender_main_ui();
        }
        Err(e) => {
            show_alert("失败", &format!("搜索失败: {}", e)).await;
        }
    }
}

fn select_location(idx: usize) {
    let picked = {
        let state = ui_state()
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.search_results.get(idx).cloned()
    };

    if let Some(item) = picked {
        add_to_recent(item.clone());
        {
            let mut state = ui_state()
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.selected_location = Some(item);
            state.selected_days = 7;
            state.selected_from_search = true;
            state.show_location_picker = false;
            state.search_query.clear();
            state.search_results.clear();
        }
        let _ = crate::ui::state::save_all_settings();
        crate::ui::build::rerender_main_ui();
        resolve_recent_locations_if_needed();
    }
}

fn select_recent_location(idx: usize) {
    let picked = {
        let state = ui_state()
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.recent_locations.get(idx).cloned()
    };

    if let Some(item) = picked {
        add_to_recent(item.clone());
        {
            let mut state = ui_state()
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.selected_location = Some(item);
            state.selected_days = 7;
            state.selected_from_search = false;
            state.show_location_picker = false;
            state.search_query.clear();
            state.search_results.clear();
        }
        let _ = crate::ui::state::save_all_settings();
        crate::ui::build::rerender_main_ui();
        resolve_recent_locations_if_needed();
    }
}

fn select_days(day: u32) {
    if day == 0 {
        return;
    }
    let mut state = ui_state()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state.selected_days = day;
    drop(state);
    let _ = crate::ui::state::save_all_settings();
    crate::ui::build::rerender_main_ui();
}

/// 对齐 syncer-ng 的 `addToRecentSearches()`: 去重 → 插入头部 → 截断到 MAX_RECENT
fn add_to_recent(location: CityLocation) {
    const MAX_RECENT: usize = 10;
    let mut state = ui_state()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state.recent_locations.retain(|item| item.id != location.id);
    state.recent_locations.insert(0, location);
    if state.recent_locations.len() > MAX_RECENT {
        state.recent_locations.truncate(MAX_RECENT);
    }
    drop(state);
    let _ = crate::ui::state::save_all_settings();
}

/// 补全缺少名称/行政区的历史位置。会发同步网络请求，调用方应先渲染界面再调用。
pub fn resolve_recent_locations_if_needed() {
    let (recent, resolving, current_tab) = {
        let state = ui_state()
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        (
            state.recent_locations.clone(),
            state.recent_resolving,
            state.current_tab,
        )
    };

    if resolving {
        return;
    }
    if current_tab != MainTab::PasteData {
        return;
    }

    let pending: Vec<CityLocation> = recent
        .into_iter()
        .filter(|item| {
            !item.id.trim().is_empty()
                && (item.name.trim().is_empty()
                    || item.adm1.trim().is_empty()
                    || item.adm2.trim().is_empty())
        })
        .collect();
    if pending.is_empty() {
        return;
    }

    {
        let mut state = ui_state()
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.recent_resolving = true;
    }

    let mut updates: Vec<(String, CityLocation)> = Vec::new();
    for item in pending {
        let query_id = item.id.clone();
        if let Ok(update) = fetch_first_location(&query_id) {
            updates.push((query_id, update));
        }
    }

    if updates.is_empty() {
        let mut state = ui_state()
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.recent_resolving = false;
        return;
    }

    {
        let mut state = ui_state()
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let selected_id = state
            .selected_location
            .as_ref()
            .map(|l| l.id.clone())
            .unwrap_or_default();
        for (query_id, update) in &updates {
            if let Some(item) = state
                .recent_locations
                .iter_mut()
                .find(|item| item.id == update.id || item.id == *query_id)
            {
                item.id = update.id.clone();
                item.name = update.name.clone();
                item.adm1 = update.adm1.clone();
                item.adm2 = update.adm2.clone();
            }
        }
        if let Some((_query_id, update)) = updates
            .iter()
            .find(|(query_id, item)| item.id == selected_id || *query_id == selected_id)
        {
            state.selected_location = Some(update.clone());
        }
        state.recent_resolving = false;
    }

    let _ = crate::ui::state::save_all_settings();
    crate::ui::build::rerender_main_ui();
}

fn fetch_first_location(query: &str) -> Result<CityLocation, String> {
    let base = api_url("/api/geo/lookup")?;
    let url = Url::parse_with_params(&base, &[("location", query)])
        .map_err(|e| format!("URL解析失败: {}", e))?
        .to_string();
    let json = super::api_client::get_json(&url)?;
    let first = json
        .get("location")
        .and_then(|v| v.as_array())
        .and_then(|v| v.first())
        .ok_or_else(|| "未找到匹配位置".to_string())?;

    Ok(CityLocation {
        id: first
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        name: first
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        adm1: first
            .get("adm1")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        adm2: first
            .get("adm2")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    })
}
fn clear_search_after_sync() {
    let mut state = ui_state()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state.search_query.clear();
    state.search_results.clear();
    state.selected_from_search = false;
    drop(state);
    let _ = crate::ui::state::save_all_settings();
    crate::ui::build::rerender_main_ui();
}

