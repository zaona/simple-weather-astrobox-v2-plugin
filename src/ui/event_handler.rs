use crate::astrobox::psys_host;
use crate::astrobox::psys_host::interconnect;
use crate::astrobox::psys_host::register;
use crate::astrobox::psys_host::thirdpartyapp;
use crate::astrobox::psys_host::timer;
use crate::astrobox::psys_host::dialog;
use url::Url;
use std::io::Read;
use flate2::read::GzDecoder;
use waki::bindings::wasi::http::{outgoing_handler, types as http_types};
use waki::bindings::wasi::io::streams::StreamError;
use super::state::*;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const INPUT_CHANGE_EVENT: &str = "input_change";
pub const SEND_BUTTON_EVENT: &str = "send_button";
pub const OPEN_WEATHER_EVENT: &str = "open_weather";
pub const OPEN_GUIDE_EVENT: &str = "open_guide";
pub const TAB_PASTE_EVENT: &str = "tab_paste";
pub const TAB_SETTINGS_EVENT: &str = "tab_settings";
pub const CUSTOM_API_HOST_CHANGE_EVENT: &str = "custom_api_host_change";
pub const CUSTOM_API_KEY_CHANGE_EVENT: &str = "custom_api_key_change";
pub const API_SAVE_TEST_EVENT: &str = "api_save_test";
pub const API_RESET_EVENT: &str = "api_reset";
pub const API_HELP_EVENT: &str = "api_help";
pub const TOGGLE_SHOW_API_HOST_EVENT: &str = "toggle_show_api_host";
pub const TOGGLE_SHOW_API_KEY_EVENT: &str = "toggle_show_api_key";
pub const OPEN_SETTINGS_API_EVENT: &str = "open_settings_api";
pub const SETTINGS_BACK_EVENT: &str = "settings_back";
pub const ADV_MODE_TOGGLE_EVENT: &str = "adv_mode_toggle";
pub const OPEN_HELP_DOC_EVENT: &str = "open_help_doc";
pub const OPEN_QQ_GROUP_EVENT: &str = "open_qq_group";
pub const OPEN_AFD_EVENT: &str = "open_afd";
pub const SEARCH_INPUT_CHANGE_EVENT: &str = "search_input_change";
pub const SEARCH_BUTTON_EVENT: &str = "search_button";
pub const SEARCH_INPUT_SUBMIT_EVENT: &str = "search_input_submit";
pub const SELECT_LOCATION_PREFIX: &str = "select_location:";
pub const SELECT_RECENT_PREFIX: &str = "select_recent:";
pub const SELECT_DAYS_PREFIX: &str = "select_days:";


static LAST_READY_TS_MS: AtomicU64 = AtomicU64::new(0);
static HANDSHAKE_RUNNING: AtomicBool = AtomicBool::new(false);
static PENDING_TIMER_ID: AtomicU64 = AtomicU64::new(0);
const PENDING_SEND_TIMEOUT_MS: u64 = 1200;

struct PendingSend {
    device_addr: String,
    device_name: String,
    pkg_name: String,
    data: String,
}

static PENDING_SEND: OnceLock<Mutex<Option<PendingSend>>> = OnceLock::new();

fn pending_send() -> &'static Mutex<Option<PendingSend>> {
    PENDING_SEND.get_or_init(|| Mutex::new(None))
}

pub fn handle_interconnect_message(payload: &str) {
    tracing::info!("收到快应用消息");

    let (addr, pkg, payload_text) = extract_interconnect_fields(payload);
    tracing::info!(
        "interconnect fields: addr={:?}, pkg={:?}, payload_text_len={}",
        addr,
        pkg,
        payload_text.as_ref().map(|s| s.len()).unwrap_or(0)
    );
    let check_text = payload_text.as_deref().unwrap_or(payload);

    if check_text.contains("ready") {
        LAST_READY_TS_MS.store(now_ms(), Ordering::SeqCst);
        tracing::info!("interconnect ready detected");

        if let (Some(addr), Some(pkg)) = (addr, pkg) {
            try_send_pending(addr, pkg);
        }
    }
}

pub fn handle_timer_payload(payload: &str) {
    if payload == "pending_send_timeout" {
        tracing::info!("pending_send_timeout fired, trying to send pending");
        try_send_pending_any();
    }
}

pub fn ui_event_processor(event_type: crate::exports::astrobox::psys_plugin::event::Event, event_id: &str, event_payload: &str) {
    if !is_high_frequency_input_event(event_id) {
        let _ = event_payload;
        tracing::info!("UI Event: type={:?}, id={}", event_type, event_id);
    }

    match event_id {
        INPUT_CHANGE_EVENT => {
            let parsed_value = parse_event_value(event_payload);
            let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
            state.weather_data = parsed_value.clone();
        }
        CUSTOM_API_HOST_CHANGE_EVENT => {
            let parsed_value = parse_event_value(event_payload);
            let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
            state.custom_api_host = parsed_value.clone();
        }
        CUSTOM_API_KEY_CHANGE_EVENT => {
            let parsed_value = parse_event_value(event_payload);
            let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
            state.custom_api_key = parsed_value.clone();
        }
        SEND_BUTTON_EVENT => {
            tracing::info!("SEND_BUTTON_EVENT received");
            send_weather_data();
        }
        TAB_PASTE_EVENT => {
            let should_rerender = {
                let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
                if state.current_tab != MainTab::PasteData {
                    state.current_tab = MainTab::PasteData;
                    true
                } else {
                    false
                }
            };
            if should_rerender {
                resolve_recent_locations_if_needed();
                crate::ui::build::rerender_main_ui();
            }
        }
        TAB_SETTINGS_EVENT => {
            let should_rerender = {
                let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
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
        OPEN_WEATHER_EVENT => {
            tracing::info!("Handling open weather event, checking deeplink permission first...");
            
            // 首先检查状态中是否已注册深度链接
            if crate::ui::state::is_deeplink_registered() {
                tracing::info!("DeepLink action already registered in state, opening weather website");
                open_weather_website();
            } else {
                // 在打开浏览器前检查并请求深度链接权限
                wit_bindgen::block_on(async move {
                    // 检查深度链接权限状态
                    match register::register_deeplink_action().await {
                        Ok(_) => {
                            tracing::info!("DeepLink action registered, updating state and opening weather website");
                            // 更新状态为已注册
                            crate::ui::state::set_deeplink_registered(true);
                            open_weather_website();
                        },
                        Err(e) => {
                            tracing::info!("DeepLink action not registered, showing permission dialog: {:?}", e);
                            
                            // 创建对话框配置（仅保留确认按钮）
                            let confirm_btn = dialog::DialogButton {
                                id: "confirm".to_string(),
                                primary: true,
                                content: "同意并启用".to_string(),
                            };
                            
                            let dialog_info = dialog::DialogInfo {
                                title: "深度链接权限请求".to_string(),
                                content: "该插件需要深度链接权限来接收天气数据。请点击\"同意并启用\"以允许插件接收外部应用发送的天气信息。".to_string(),
                                buttons: vec![confirm_btn],
                            };
                            
                            // 显示对话框请求用户授权（使用Website样式）
                            let _result = dialog::show_dialog(
                                dialog::DialogType::Alert,
                                dialog::DialogStyle::Website,
                                &dialog_info
                            ).await;
                            
                            // 用户点击了确认按钮（唯一的按钮）
                            tracing::info!("User confirmed deeplink permission request");
                            // 用户同意后，我们假设权限已经获得，直接打开网站
                            tracing::info!("User granted permission, opening weather website");
                            open_weather_website();
                        },
                    }
                });
            }
        }
        OPEN_GUIDE_EVENT => {
            open_guide_page();
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
        API_SAVE_TEST_EVENT => {
            save_and_test_custom_api();
        }
        API_RESET_EVENT => {
            reset_custom_api();
        }
        API_HELP_EVENT => {
            open_api_help_page();
        }
        OPEN_SETTINGS_API_EVENT => {
            let should_rerender = {
                let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
                state.settings_page = SettingsPage::Api;
                true
            };
            if should_rerender {
                crate::ui::build::rerender_main_ui();
            }
        }
        SETTINGS_BACK_EVENT => {
            let should_rerender = {
                let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
                state.settings_page = SettingsPage::Main;
                true
            };
            if should_rerender {
                crate::ui::build::rerender_main_ui();
            }
        }
        ADV_MODE_TOGGLE_EVENT => {
            let should_rerender = {
                let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
                state.advanced_mode = !state.advanced_mode;
                true
            };
            if should_rerender {
                let _ = crate::ui::state::save_all_settings();
                resolve_recent_locations_if_needed();
                crate::ui::build::rerender_main_ui();
            }
        }
        SEARCH_INPUT_CHANGE_EVENT => {
            let parsed_value = parse_event_value(event_payload);
            let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
            state.search_query = parsed_value;
        }
        SEARCH_INPUT_SUBMIT_EVENT => {
            let parsed_value = parse_event_value(event_payload);
            {
                let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
                state.search_query = parsed_value;
            }
            if payload_has_enter(event_payload) {
                search_locations();
            }
        }
        SEARCH_BUTTON_EVENT => {
            search_locations();
        }
        TOGGLE_SHOW_API_HOST_EVENT => {
            let should_rerender = {
                let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
                state.show_api_host = !state.show_api_host;
                true
            };
            if should_rerender {
                crate::ui::build::rerender_main_ui();
            }
        }
        TOGGLE_SHOW_API_KEY_EVENT => {
            let should_rerender = {
                let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
                state.show_api_key = !state.show_api_key;
                true
            };
            if should_rerender {
                crate::ui::build::rerender_main_ui();
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
    if event_id.starts_with(SELECT_DAYS_PREFIX) {
        if let Some(day_str) = event_id.strip_prefix(SELECT_DAYS_PREFIX) {
            if let Ok(day) = day_str.parse::<u32>() {
                select_days(day);
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
        INPUT_CHANGE_EVENT
            | SEARCH_INPUT_CHANGE_EVENT
            | SEARCH_INPUT_SUBMIT_EVENT
            | CUSTOM_API_HOST_CHANGE_EVENT
            | CUSTOM_API_KEY_CHANGE_EVENT
    )
}

fn parse_event_value(payload: &str) -> String {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(payload) {
        json.get("value")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    } else {
        tracing::warn!("parse_event_value failed to parse JSON");
        payload.to_string()
    }
}

fn send_weather_data() {
    tracing::info!("send_weather_data called");

    let advanced_mode = {
        let state = ui_state().read().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.advanced_mode
    };
    if advanced_mode {
        send_weather_data_advanced();
        return;
    }

    let weather_data = {
        let state = ui_state().read().unwrap_or_else(|poisoned| poisoned.into_inner());
        let value = state.weather_data.clone();
        value
    };

    tracing::info!("weather_data.is_empty(): {}", weather_data.is_empty());

    if weather_data.is_empty() {
        tracing::warn!("weather_data is empty, showing alert");
        show_alert("提示", "请先粘贴天气数据");
        return;
    }

    if let Err(reason) = validate_paste_weather_data(&weather_data) {
        tracing::warn!("paste weather data validation failed: {}", reason);
        show_alert(
            "提示",
            "天气数据粘贴不全，建议使用自定义api启用高级同步模式，或者使用微信输入法等可完整复制粘贴内容的输入法。",
        );
        return;
    }

    tracing::info!("weather_data has content, starting send");

    let data = weather_data.clone();

    wit_bindgen::block_on(async move {
        tracing::info!("inside block_on");

        match send_via_interconnect(&data).await {
            Ok(SendOutcome::Sent) => {
                tracing::info!("send_via_interconnect success");
                record_recent_location_from_paste(&data);
                show_alert("成功", "发送成功");
            }
            Ok(SendOutcome::Pending) => {
                tracing::info!("send_via_interconnect pending; waiting for ready");
            }
            Err(e) => {
                tracing::error!("send_via_interconnect error: {}", e);
                show_alert("失败", &format!("发送失败: {}", e));
            }
        }
    });
}

fn validate_paste_weather_data(data: &str) -> Result<(), String> {
    let trimmed = data.trim();
    if trimmed.is_empty() {
        return Err("empty payload".to_string());
    }

    let json = serde_json::from_str::<serde_json::Value>(trimmed)
        .map_err(|e| format!("invalid json: {}", e))?;
    let obj = json
        .as_object()
        .ok_or_else(|| "json root is not an object".to_string())?;

    let has_main_weather = obj
        .get("daily")
        .and_then(|v| v.as_array())
        .map(|v| !v.is_empty())
        .unwrap_or(false)
        || obj.get("now").and_then(|v| v.as_object()).is_some()
        || obj
            .get("hourly")
            .and_then(|v| v.as_array())
            .map(|v| !v.is_empty())
            .unwrap_or(false);

    let has_location_marker = obj
        .get("fxLink")
        .and_then(|v| v.as_str())
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
        || obj
            .get("location")
            .and_then(|v| v.get("fxLink"))
            .and_then(|v| v.as_str())
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false)
        || obj
            .get("location")
            .and_then(|v| v.as_str())
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);

    if has_main_weather || has_location_marker {
        Ok(())
    } else {
        Err("missing weather keys".to_string())
    }
}

fn send_weather_data_advanced() {
    let (
        api,
        location_id,
        location_name,
        location_adm1,
        location_adm2,
        location_lat,
        location_lon,
        days,
        selected_from_search,
    ) = {
        let state = ui_state().read().unwrap_or_else(|poisoned| poisoned.into_inner());
        (
            effective_api_host_key(&state),
            state.selected_location_id.clone(),
            state.selected_location_name.clone(),
            state.selected_location_adm1.clone(),
            state.selected_location_adm2.clone(),
            state.selected_location_lat.clone(),
            state.selected_location_lon.clone(),
            state.selected_days,
            state.selected_from_search,
        )
    };

    let Some((host, key)) = api else {
        show_alert("提示", "请先在设置中配置自定义API");
        return;
    };
    if location_id.is_empty() && (location_lat.is_empty() || location_lon.is_empty()) {
        show_alert("提示", "请先选择位置");
        return;
    }

    let location_param = if !location_lon.is_empty() && !location_lat.is_empty() {
        format!("{},{}", location_lon, location_lat)
    } else {
        location_id.clone()
    };
    let days_segment = days_to_api_segment(days);
    let url = format!(
        "https://{}/v7/weather/{}?location={}&key={}",
        host,
        days_segment,
        location_param,
        key
    );

    let recent_location = LocationOption {
        id: location_id,
        name: location_name,
        adm1: location_adm1,
        adm2: location_adm2,
        lat: location_lat,
        lon: location_lon,
    };

    match http_get_json(&url) {
        Ok(mut json) => {
            json["location"] = serde_json::Value::String(recent_location.name.clone());
            let payload = json.to_string();
            let recent_location = recent_location.clone();
            wit_bindgen::block_on(async move {
                match send_via_interconnect(&payload).await {
                    Ok(SendOutcome::Sent) => {
                        record_recent_location(recent_location);
                        if selected_from_search {
                            clear_search_after_sync();
                        }
                        show_alert("成功", "发送成功");
                    }
                    Ok(SendOutcome::Pending) => {
                        tracing::info!("send_via_interconnect pending; waiting for ready");
                        record_recent_location(recent_location);
                        if selected_from_search {
                            clear_search_after_sync();
                        }
                    }
                    Err(e) => show_alert("失败", &format!("发送失败: {}", e)),
                }
            });
        }
        Err(e) => {
            show_alert("失败", &format!("获取天气失败: {}", e));
        }
    }
}

fn days_to_api_segment(days: u32) -> &'static str {
    match days {
        3 => "3d",
        7 => "7d",
        10 => "10d",
        15 => "15d",
        30 => "30d",
        _ => "3d",
    }
}

enum SendOutcome {
    Sent,
    Pending,
}

async fn send_via_interconnect(data: &str) -> Result<SendOutcome, String> {
    tracing::info!("send_via_interconnect start");

    let devices = psys_host::device::get_connected_device_list().await;
    tracing::info!("get_connected_device_list returned {} devices", devices.len());

    if devices.is_empty() {
        return Err("没有连接的设备".to_string());
    }

    let first_device = devices.first()
        .ok_or("没有连接的设备")?
        .clone();
    let device_addr = first_device.addr.clone();
    let device_name = first_device.name.clone();

    tracing::info!("using device: {}", device_addr);

    let pkg_name = "com.application.zaona.weather";

    tracing::info!("checking if quick app is installed...");
    match check_quick_app_installed(&device_addr, pkg_name).await {
        Ok(false) => {
            return Err("请先安装简明天气快应用".to_string());
        }
        Err(e) => {
            tracing::warn!("failed to check app list: {:?}, assuming app exists", e);
        }
        Ok(true) => {
            tracing::info!("quick app is installed");
        }
    }

    tracing::info!("ensuring interconnect is registered for device: {}", device_addr);
    let reg_result = register::register_interconnect_recv(&device_addr, pkg_name).await;
    tracing::info!("register_interconnect_recv result: {:?}", reg_result);
    if reg_result.is_err() {
        return Err("register_interconnect_recv failed".to_string());
    }
    tracing::info!("register completed");

    tracing::info!("launching quick app before send...");
    ensure_quick_app_launched(&device_addr, pkg_name, "/index").await?;

    let last_ready = LAST_READY_TS_MS.load(Ordering::SeqCst);
    let now = now_ms();
    if last_ready > 0 && now.saturating_sub(last_ready) <= 5000 {
        let data_str = data.to_string();
        tracing::info!("ready recently seen, sending immediately");
        interconnect::send_qaic_message(&device_addr, pkg_name, &data_str)
            .await
            .map_err(|e| format!("{:?}", e))?;
        tracing::info!("send_qaic_message completed");
        let report_result = super::supabase::report_device_to_supabase(&device_addr, &device_name);
        update_last_sync_from_data(&data_str);
        if let Err(e) = report_result {
            tracing::warn!("send success but supabase report failed: {}", e);
        }
        return Ok(SendOutcome::Sent);
    }

    // Queue the payload and wait for ready via interconnect message callback.
    {
        let mut slot = pending_send().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        *slot = Some(PendingSend {
            device_addr: device_addr.clone(),
            device_name: device_name.clone(),
            pkg_name: pkg_name.to_string(),
            data: data.to_string(),
        });
    }

    schedule_pending_timeout().await;

    tracing::info!("sending start message and waiting for ready...");
    start_handshake_loop(device_addr.clone(), pkg_name.to_string());

    Ok(SendOutcome::Pending)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn extract_interconnect_fields(payload: &str) -> (Option<String>, Option<String>, Option<String>) {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(payload) {
        let addr = json.get("addr").and_then(|v| v.as_str()).map(|s| s.to_string());
        let pkg = json.get("pkgName").and_then(|v| v.as_str()).map(|s| s.to_string());
        let payload_text = json
            .get("payloadText")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        return (addr, pkg, payload_text);
    }
    (None, None, None)
}

fn update_last_sync_from_data(data: &str) {
    let location_from_data = extract_location_from_payload(data);
    let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
    state.last_sync_time_ms = now_ms();

    if let Some(loc) = location_from_data {
        if !loc.is_empty() {
            state.last_sync_location = loc;
            drop(state);
            crate::ui::render_sync_card(crate::ui::SYNC_CARD_ID);
            return;
        }
    }

    if !state.selected_location_name.is_empty() {
        state.last_sync_location = state.selected_location_name.clone();
    }
    drop(state);
    crate::ui::render_sync_card(crate::ui::SYNC_CARD_ID);
}

fn extract_location_from_payload(data: &str) -> Option<String> {
    let json = serde_json::from_str::<serde_json::Value>(data).ok()?;
    json.get("location").and_then(|v| v.as_str()).map(|s| s.to_string())
}

fn try_send_pending(addr: String, pkg: String) {
    tracing::info!("try_send_pending called: addr={}, pkg={}", addr, pkg);
    let pending = {
        let mut slot = pending_send().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        match slot.as_ref() {
            Some(item) => {
                tracing::info!(
                    "pending slot exists: addr={}, pkg={}, data_len={}",
                    item.device_addr,
                    item.pkg_name,
                    item.data.len()
                );
                if item.device_addr == addr && item.pkg_name == pkg {
                    slot.take()
                } else {
                    tracing::warn!("pending slot mismatch, skip send");
                    None
                }
            }
            None => {
                tracing::warn!("pending slot empty, nothing to send");
                None
            }
        }
    };

    let Some(pending) = pending else {
        return;
    };

    tracing::info!(
        "sending pending payload: addr={}, pkg={}, data_len={}",
        pending.device_addr,
        pending.pkg_name,
        pending.data.len()
    );
    wit_bindgen::block_on(async move {
        clear_pending_timeout().await;
        let send_result = interconnect::send_qaic_message(
            &pending.device_addr,
            &pending.pkg_name,
            &pending.data,
        )
        .await;

        match send_result {
            Ok(_) => {
                tracing::info!("pending send completed");
                let report_result =
                    super::supabase::report_device_to_supabase(&pending.device_addr, &pending.device_name);
                record_recent_location_from_paste(&pending.data);
                HANDSHAKE_RUNNING.store(false, Ordering::SeqCst);
                update_last_sync_from_data(&pending.data);
                if report_result.is_ok() {
                    show_alert("成功", "发送成功");
                } else {
                    tracing::warn!(
                        "send success but supabase report failed: {}",
                        report_result.err().unwrap_or_else(|| "未知错误".to_string())
                    );
                    show_alert("成功", "发送成功");
                }
            }
            Err(e) => {
                tracing::error!("pending send failed: {:?}", e);
                HANDSHAKE_RUNNING.store(false, Ordering::SeqCst);
                show_alert("失败", &format!("发送失败: {:?}", e));
            }
        }
    });
}

fn try_send_pending_any() {
    let pending = {
        let mut slot = pending_send().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        slot.take()
    };

    let Some(pending) = pending else {
        tracing::warn!("pending slot empty, nothing to send");
        return;
    };

    tracing::info!(
        "sending pending payload (timeout): addr={}, pkg={}, data_len={}",
        pending.device_addr,
        pending.pkg_name,
        pending.data.len()
    );

    wit_bindgen::block_on(async move {
        clear_pending_timeout().await;
        let send_result = interconnect::send_qaic_message(
            &pending.device_addr,
            &pending.pkg_name,
            &pending.data,
        )
        .await;

        match send_result {
            Ok(_) => {
                tracing::info!("pending send completed (timeout)");
                let report_result =
                    super::supabase::report_device_to_supabase(&pending.device_addr, &pending.device_name);
                record_recent_location_from_paste(&pending.data);
                HANDSHAKE_RUNNING.store(false, Ordering::SeqCst);
                update_last_sync_from_data(&pending.data);
                if report_result.is_ok() {
                    show_alert("成功", "发送成功");
                } else {
                    tracing::warn!(
                        "send success but supabase report failed: {}",
                        report_result.err().unwrap_or_else(|| "未知错误".to_string())
                    );
                    show_alert("成功", "发送成功");
                }
            }
            Err(e) => {
                tracing::error!("pending send failed (timeout): {:?}", e);
                HANDSHAKE_RUNNING.store(false, Ordering::SeqCst);
                show_alert("失败", &format!("发送失败: {:?}", e));
            }
        }
    });
}

async fn schedule_pending_timeout() {
    clear_pending_timeout().await;
    let timer_id = timer::set_timeout(1200, "pending_send_timeout").await;
    if timer_id != 0 {
        PENDING_TIMER_ID.store(timer_id, Ordering::SeqCst);
    }
}

async fn clear_pending_timeout() {
    let old = PENDING_TIMER_ID.swap(0, Ordering::SeqCst);
    if old != 0 {
        let _ = timer::clear_timer(old).await;
    }
}

fn start_handshake_loop(device_addr: String, pkg_name: String) {
    if HANDSHAKE_RUNNING.swap(true, Ordering::SeqCst) {
        tracing::info!("handshake loop already running");
        return;
    }

    wit_bindgen::spawn(async move {
        let mut last_seen = LAST_READY_TS_MS.load(Ordering::SeqCst);
        let start_ms = now_ms();
        for attempt in 0..15 {
            if !pending_exists() {
                tracing::info!("pending already sent, stopping handshake loop");
                HANDSHAKE_RUNNING.store(false, Ordering::SeqCst);
                return;
            }
            tracing::info!("handshake attempt {}", attempt + 1);
            let start_result = interconnect::send_qaic_message(&device_addr, &pkg_name, "start").await;
            tracing::info!("send start result: {:?}", start_result);

            for _ in 0..12 {
                let current = LAST_READY_TS_MS.load(Ordering::SeqCst);
                if current > last_seen {
                    tracing::info!("handshake ready received in loop");
                    HANDSHAKE_RUNNING.store(false, Ordering::SeqCst);
                    return;
                }
                last_seen = current;
                std::thread::sleep(Duration::from_millis(50));
                if now_ms().saturating_sub(start_ms) >= PENDING_SEND_TIMEOUT_MS && pending_exists() {
                    tracing::info!("pending timeout reached, sending without ready");
                    HANDSHAKE_RUNNING.store(false, Ordering::SeqCst);
                    try_send_pending_any();
                    return;
                }
            }
        }

        tracing::warn!("handshake loop exhausted without ready");
        HANDSHAKE_RUNNING.store(false, Ordering::SeqCst);
    });
}

fn pending_exists() -> bool {
    let slot = pending_send().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    slot.is_some()
}

async fn check_quick_app_installed(device_addr: &str, pkg_name: &str) -> Result<bool, String> {
    tracing::info!("checking for package: {}", pkg_name);

    match thirdpartyapp::get_thirdparty_app_list(device_addr).await {
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
            tracing::error!("failed to get app list: {:?}", e);
            Err(format!("{:?}", e))
        }
    }
}

async fn ensure_quick_app_launched(device_addr: &str, pkg_name: &str, page_name: &str) -> Result<(), String> {
    tracing::info!("ensure_quick_app_launched: pkg={}, page={}", pkg_name, page_name);

    let app_list = thirdpartyapp::get_thirdparty_app_list(device_addr)
        .await
        .map_err(|e| format!("{:?}", e))?;

    let app = match app_list.iter().find(|app| app.package_name == pkg_name) {
        Some(app) => app,
        None => {
            show_alert("未安装", "请先安装简明天气快应用");
            return Err("请先安装简明天气快应用".to_string());
        }
    };

    thirdpartyapp::launch_qa(device_addr, app, page_name)
        .await
        .map_err(|e| format!("{:?}", e))?;

    tracing::info!("quick app launched");
    Ok(())
}

fn open_weather_website() {
    tracing::info!("open_weather_website called");

    let url = "https://weather.zaona.top/weather";

    dialog::open_url(url);
    tracing::info!("opened weather website: {}", url);
}

fn open_guide_page() {
    tracing::info!("open_guide_page called");

    let url = "https://www.yuque.com/zaona/weather/plugin";

    dialog::open_url(url);
    tracing::info!("opened guide page: {}", url);
}

fn open_api_help_page() {
    tracing::info!("open_api_help_page called");

    let url = "https://www.yuque.com/zaona/weather/api";

    dialog::open_url(url);
    tracing::info!("opened api help page: {}", url);
}

fn open_help_doc_page() {
    tracing::info!("open_help_doc_page called");
    let url = "https://www.yuque.com/zaona/weather";
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

fn show_alert(title: &str, message: &str) {
    tracing::info!("show_alert: title={}, message={}", title, message);

    let title_str = title.to_string();
    let message_str = message.to_string();

    wit_bindgen::block_on(async move {
        tracing::info!("show_alert executing dialog::show_dialog (Website style)");
        let _ = dialog::show_dialog(
            dialog::DialogType::Alert,
            dialog::DialogStyle::Website,
            &dialog::DialogInfo {
                title: title_str,
                content: message_str,
                buttons: vec![dialog::DialogButton {
                    id: "ok".to_string(),
                    primary: true,
                    content: "确定".to_string(),
                }],
            },
        ).await;
        tracing::info!("alert dialog closed");
    });
}

fn save_and_test_custom_api() {
    let (host, key) = {
        let state = ui_state().read().unwrap_or_else(|poisoned| poisoned.into_inner());
        (state.custom_api_host.clone(), state.custom_api_key.clone())
    };

    match test_custom_api_connection(&host, &key) {
        Ok(_) => {
            {
                let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
                state.custom_api_host = host.trim().to_string();
                state.custom_api_key = key.trim().to_string();
                refresh_api_state(&mut state);
                auto_enable_advanced_mode_if_api_ready(&mut state);
            }
            if let Err(e) = crate::ui::state::save_all_settings() {
                show_alert("保存失败", &format!("写入配置失败: {}", e));
            } else {
                show_alert("保存成功", "配置已保存并验证通过");
            }
        }
        Err(message) => {
            show_alert("验证失败", &message);
        }
    }
}

fn reset_custom_api() {
    {
        let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.custom_api_host.clear();
        state.custom_api_key.clear();
        refresh_api_state(&mut state);
        auto_enable_advanced_mode_if_api_ready(&mut state);
    }
    if let Err(e) = crate::ui::state::save_all_settings() {
        show_alert("重置失败", &format!("写入配置失败: {}", e));
    } else {
        show_alert("重置成功", "已恢复默认配置");
    }
    crate::ui::build::rerender_main_ui();
}

fn test_custom_api_connection(host: &str, key: &str) -> Result<(), String> {
    let host = host.trim();
    let key = key.trim();

    if host.is_empty() {
        return Err("API Host 不能为空".to_string());
    }
    if key.is_empty() {
        return Err("API Key 不能为空".to_string());
    }

    let url = format!("https://{}/geo/v2/city/lookup?location=北京&key={}", host, key);
    let (status_code, body) = http_get_bytes(&url)?;
    tracing::info!(
        "test_custom_api_connection status={}, body_len={}",
        status_code,
        body.len()
    );

    if status_code != 200 {
        return match status_code {
            401 => Err("API密钥无效或已过期".to_string()),
            403 => Err("访问被拒绝，请检查API权限".to_string()),
            429 => Err("请求过于频繁，请稍后再试".to_string()),
            _ => Err(format!("API Error: {}", status_code)),
        };
    }

    let body = maybe_decompress(body)?;
    if body.is_empty() {
        return Err("Empty response".to_string());
    }
    let json: serde_json::Value =
        serde_json::from_slice(&body).map_err(|e| format!("响应解析失败: {}", e))?;
    if let Some(code) = json.get("code").and_then(|v| v.as_str()) {
        if code == "200" {
            Ok(())
        } else {
            Err(format!("API Error: {}", code))
        }
    } else {
        Ok(())
    }
}

fn search_locations() {
    let (api, query) = {
        let state = ui_state().read().unwrap_or_else(|poisoned| poisoned.into_inner());
        (effective_api_host_key(&state), state.search_query.clone())
    };

    let Some((host, key)) = api else {
        show_alert("提示", "请先在设置中配置自定义API");
        return;
    };
    if query.trim().is_empty() {
        show_alert("提示", "请输入城市名称");
        return;
    }

    let base = format!("https://{}/geo/v2/city/lookup", host);
    let url = match Url::parse_with_params(&base, &[("location", query), ("key", key)]) {
        Ok(u) => u.to_string(),
        Err(e) => {
            show_alert("错误", &format!("URL解析失败: {}", e));
            return;
        }
    };

    match http_get_json(&url) {
        Ok(json) => {
            let mut results: Vec<LocationOption> = Vec::new();
            if let Some(list) = json.get("location").and_then(|v| v.as_array()) {
                for item in list {
                    let id = item.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let adm1 = item.get("adm1").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let adm2 = item.get("adm2").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let lat = item.get("lat").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let lon = item.get("lon").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    if !id.is_empty() || (!lat.is_empty() && !lon.is_empty()) {
                        let normalized_id = if id.is_empty() {
                            format!("{},{}", lon, lat)
                        } else {
                            id
                        };
                        results.push(LocationOption {
                            id: normalized_id,
                            name,
                            adm1,
                            adm2,
                            lat,
                            lon,
                        });
                    }
                }
            }
            {
                let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
                state.search_results = results;
            }
            crate::ui::build::rerender_main_ui();
        }
        Err(e) => {
            show_alert("失败", &format!("搜索失败: {}", e));
        }
    }
}

fn select_location(idx: usize) {
    let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
    let picked = state.search_results.get(idx).cloned();
    if let Some(item) = &picked {
        state.selected_location_id = item.id.clone();
        state.selected_location_name = item.name.clone();
        state.selected_location_adm1 = item.adm1.clone();
        state.selected_location_adm2 = item.adm2.clone();
        state.selected_location_lat = item.lat.clone();
        state.selected_location_lon = item.lon.clone();
        state.selected_days = 7;
        state.selected_from_search = true;
    }
    drop(state);
    let _ = crate::ui::state::save_all_settings();
    crate::ui::build::rerender_main_ui();
}

fn select_recent_location(idx: usize) {
    let _picked = {
        let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
        let picked = state.recent_locations.get(idx).cloned();
        if let Some(item) = &picked {
            state.selected_location_id = item.id.clone();
            state.selected_location_name = item.name.clone();
            state.selected_location_adm1 = item.adm1.clone();
            state.selected_location_adm2 = item.adm2.clone();
            state.selected_location_lat = item.lat.clone();
            state.selected_location_lon = item.lon.clone();
            state.selected_days = 7;
            state.selected_from_search = false;
        }
        picked
    };
    let _ = crate::ui::state::save_all_settings();
    crate::ui::build::rerender_main_ui();
}

fn select_days(day: u32) {
    if day == 0 {
        return;
    }
    let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
    state.selected_days = day;
    drop(state);
    let _ = crate::ui::state::save_all_settings();
    crate::ui::build::rerender_main_ui();
}

fn record_recent_location(location: LocationOption) {
    const MAX_RECENT: usize = 5;
    let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
    state.selected_location_id = location.id.clone();
    state.selected_location_name = location.name.clone();
    state.selected_location_adm1 = location.adm1.clone();
    state.selected_location_adm2 = location.adm2.clone();
    state.selected_location_lat = location.lat.clone();
    state.selected_location_lon = location.lon.clone();

    state.recent_locations.retain(|item| item.id != location.id);
    state.recent_locations.insert(0, location);
    if state.recent_locations.len() > MAX_RECENT {
        state.recent_locations.truncate(MAX_RECENT);
    }
    drop(state);
    let _ = crate::ui::state::save_all_settings();
    resolve_recent_locations_if_needed();
    crate::ui::build::rerender_main_ui();
}

fn record_recent_location_from_paste(data: &str) {
    let json = match serde_json::from_str::<serde_json::Value>(data) {
        Ok(value) => value,
        Err(_) => return,
    };

    let fxlink = json
        .get("fxLink")
        .and_then(|v| v.as_str())
        .or_else(|| json.get("location").and_then(|v| v.get("fxLink")).and_then(|v| v.as_str()));
    let location_id = fxlink
        .and_then(extract_location_id_from_fxlink)
        .unwrap_or_default();
    if location_id.is_empty() {
        return;
    }

    let location_name = json
        .get("location")
        .and_then(|v| v.as_str())
        .or_else(|| json.get("location").and_then(|v| v.get("name")).and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();

    record_recent_location(LocationOption {
        id: location_id,
        name: location_name,
        adm1: String::new(),
        adm2: String::new(),
        lat: String::new(),
        lon: String::new(),
    });
}

fn extract_location_id_from_fxlink(link: &str) -> Option<String> {
    let end = link.rfind(".html")?;
    let part = &link[..end];
    let start = part.rfind('-')?;
    let id = &part[start + 1..];
    if id.chars().all(|c| c.is_ascii_digit()) && !id.is_empty() {
        Some(id.to_string())
    } else {
        None
    }
}

pub fn resolve_recent_locations_if_needed() {
    let (api, recent, selected_id, resolving, advanced_mode, current_tab) = {
        let state = ui_state().read().unwrap_or_else(|poisoned| poisoned.into_inner());
        (
            effective_api_host_key(&state),
            state.recent_locations.clone(),
            state.selected_location_id.clone(),
            state.recent_resolving,
            state.advanced_mode,
            state.current_tab,
        )
    };

    if resolving {
        return;
    }
    if !advanced_mode || current_tab != MainTab::PasteData {
        return;
    }
    let Some((host, key)) = api else {
        return;
    };

    let pending: Vec<LocationOption> = recent
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
        let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.recent_resolving = true;
    }

    let mut updates: Vec<(String, LocationOption)> = Vec::new();
    for item in pending {
        let query_id = item.id.clone();
        let base = format!("https://{}/geo/v2/city/lookup", host);
        let url = match Url::parse_with_params(&base, &[("location", query_id.clone()), ("key", key.clone())]) {
            Ok(u) => u.to_string(),
            Err(_) => continue,
        };
        if let Ok(json) = http_get_json(&url) {
            if let Some(first) = json.get("location").and_then(|v| v.as_array()).and_then(|v| v.first()) {
                let name = first.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let adm1 = first.get("adm1").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let adm2 = first.get("adm2").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let lat = first.get("lat").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let lon = first.get("lon").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let id = first.get("id").and_then(|v| v.as_str()).unwrap_or(&item.id).to_string();
                updates.push((
                    query_id,
                    LocationOption {
                    id,
                    name,
                    adm1,
                    adm2,
                    lat,
                    lon,
                    },
                ));
            }
        }
    }

    if updates.is_empty() {
        let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.recent_resolving = false;
        return;
    }

    {
        let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
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
                item.lat = update.lat.clone();
                item.lon = update.lon.clone();
            }
        }
        if let Some((_query_id, update)) = updates
            .iter()
            .find(|(query_id, item)| item.id == selected_id || *query_id == selected_id)
        {
            state.selected_location_id = update.id.clone();
            state.selected_location_name = update.name.clone();
            state.selected_location_adm1 = update.adm1.clone();
            state.selected_location_adm2 = update.adm2.clone();
            state.selected_location_lat = update.lat.clone();
            state.selected_location_lon = update.lon.clone();
        }
        state.recent_resolving = false;
    }

    let _ = crate::ui::state::save_all_settings();
    crate::ui::build::rerender_main_ui();
}
fn clear_search_after_sync() {
    let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
    state.search_query.clear();
    state.search_results.clear();
    state.selected_from_search = false;
    drop(state);
    let _ = crate::ui::state::save_all_settings();
    crate::ui::build::rerender_main_ui();
}

fn http_get_json(url: &str) -> Result<serde_json::Value, String> {
    let (status, body) = http_get_bytes(url)?;
    tracing::info!("http_get_json status={}, body_len={}", status, body.len());
    log_body_preview("http_get_json", &body);
    if status != 200 {
        return Err(format!("HTTP {}", status));
    }
    let body = maybe_decompress(body)?;
    if body.is_empty() {
        return Err("Empty response".to_string());
    }
    serde_json::from_slice(&body).map_err(|e| format!("响应解析失败: {}", e))
}

fn http_get_bytes(url: &str) -> Result<(u16, Vec<u8>), String> {
    let headers: Vec<(String, String)> = Vec::new();
    http_request_bytes("GET", url, &headers, None)
}

fn http_request_bytes(
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: Option<&[u8]>,
) -> Result<(u16, Vec<u8>), String> {
    tracing::info!("http_request_bytes method={}", method);
    let url = Url::parse(url).map_err(|e| e.to_string())?;
    let header_entries: Vec<(String, Vec<u8>)> = headers
        .iter()
        .map(|(k, v)| (k.clone(), v.as_bytes().to_vec()))
        .collect();
    let headers = http_types::Headers::from_list(&header_entries)
        .map_err(|e| format!("{:?}", e))?;
    let req = http_types::OutgoingRequest::new(headers);

    let http_method = match method {
        "POST" => http_types::Method::Post,
        "GET" => http_types::Method::Get,
        _ => return Err(format!("unsupported method: {}", method)),
    };

    req.set_method(&http_method)
        .map_err(|()| "failed to set method".to_string())?;

    let scheme = match url.scheme() {
        "https" => http_types::Scheme::Https,
        _ => http_types::Scheme::Http,
    };
    req.set_scheme(Some(&scheme))
        .map_err(|()| "failed to set scheme".to_string())?;

    let authority = url.authority();
    req.set_authority(Some(authority))
        .map_err(|()| "failed to set authority".to_string())?;

    let path = match url.query() {
        Some(q) => format!("{}?{}", url.path(), q),
        None => url.path().to_string(),
    };
    req.set_path_with_query(Some(&path))
        .map_err(|()| "failed to set path".to_string())?;

    let options = http_types::RequestOptions::new();
    let outgoing_body = req
        .body()
        .map_err(|_| "outgoing request write failed".to_string())?;
    let maybe_stream = if let Some(body) = body {
        let stream = outgoing_body
            .write()
            .map_err(|_| "open body writer failed".to_string())?;
        stream
            .blocking_write_and_flush(body)
            .map_err(|e| format!("write body failed: {:?}", e))?;
        drop(stream);
        None
    } else {
        None
    };
    http_types::OutgoingBody::finish(outgoing_body, maybe_stream)
        .map_err(|_| "finish body failed".to_string())?;

    let future_response = outgoing_handler::handle(req, Some(options))
        .map_err(|e| format!("{:?}", e))?;
    let incoming_response = match future_response.get() {
        Some(result) => result.map_err(|()| "response already taken".to_string())?,
        None => {
            let pollable = future_response.subscribe();
            pollable.block();
            future_response
                .get()
                .ok_or_else(|| "response not available".to_string())?
                .map_err(|()| "response already taken".to_string())?
        }
    }
    .map_err(|e| format!("{:?}", e))?;

    let status = incoming_response.status();
    tracing::info!("http_get_bytes status={}", status);
    let incoming_body = incoming_response
        .consume()
        .map_err(|_| "missing body".to_string())?;
    let input_stream = incoming_body
        .stream()
        .map_err(|_| "failed to open body stream".to_string())?;

    let mut body = Vec::new();
    loop {
        match input_stream.blocking_read(1024 * 64) {
            Ok(chunk) => {
                if chunk.is_empty() {
                    break;
                }
                body.extend_from_slice(&chunk);
            }
            Err(StreamError::Closed) => break,
            Err(e) => return Err(format!("read body failed: {:?}", e)),
        }
    }

    Ok((status, body))
}

fn log_body_preview(tag: &str, body: &[u8]) {
    if body.is_empty() {
        tracing::info!("{} body_preview: <empty>", tag);
        return;
    }
    let preview_len = body.len().min(400);
    let preview = String::from_utf8_lossy(&body[..preview_len]);
    tracing::info!("{} body_preview_utf8: {}", tag, preview);
}

fn maybe_decompress(body: Vec<u8>) -> Result<Vec<u8>, String> {
    if body.len() >= 2 && body[0] == 0x1f && body[1] == 0x8b {
        tracing::info!("detected gzip body, decompressing...");
        let mut decoder = GzDecoder::new(&body[..]);
        let mut out = Vec::new();
        decoder
            .read_to_end(&mut out)
            .map_err(|e| format!("gzip decompress failed: {}", e))?;
        return Ok(out);
    }
    Ok(body)
}
