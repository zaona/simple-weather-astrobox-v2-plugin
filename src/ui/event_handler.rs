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
pub const SELECT_DAYS_PREFIX: &str = "select_days:";


static LAST_READY_TS_MS: AtomicU64 = AtomicU64::new(0);
static HANDSHAKE_RUNNING: AtomicBool = AtomicBool::new(false);
static PENDING_TIMER_ID: AtomicU64 = AtomicU64::new(0);
const PENDING_SEND_TIMEOUT_MS: u64 = 1200;

struct PendingSend {
    device_addr: String,
    pkg_name: String,
    data: String,
}

static PENDING_SEND: OnceLock<Mutex<Option<PendingSend>>> = OnceLock::new();

fn pending_send() -> &'static Mutex<Option<PendingSend>> {
    PENDING_SEND.get_or_init(|| Mutex::new(None))
}

pub fn handle_interconnect_message(payload: &str) {
    tracing::info!("收到快应用消息: {}", payload);

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
    tracing::info!("UI Event: type={:?}, id={}, payload={}", event_type, event_id, event_payload);

    match event_id {
        INPUT_CHANGE_EVENT => {
            let parsed_value = parse_event_value(event_payload);
            tracing::info!("INPUT_CHANGE_EVENT, value={}", parsed_value);
            let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
            state.weather_data = parsed_value.clone();
            tracing::info!("state.weather_data updated to: {}", parsed_value);
        }
        CUSTOM_API_HOST_CHANGE_EVENT => {
            let parsed_value = parse_event_value(event_payload);
            let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
            state.custom_api_host = parsed_value.clone();
            tracing::info!("state.custom_api_host updated to: {}", parsed_value);
        }
        CUSTOM_API_KEY_CHANGE_EVENT => {
            let parsed_value = parse_event_value(event_payload);
            let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
            state.custom_api_key = parsed_value.clone();
            tracing::info!("state.custom_api_key updated");
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

fn parse_event_value(payload: &str) -> String {
    tracing::info!("parse_event_value input: {}", payload);
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(payload) {
        let value = json.get("value")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        tracing::info!("parse_event_value result: '{}'", value);
        value
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
        tracing::info!("read state.weather_data: '{}'", value);
        value
    };

    tracing::info!("weather_data.is_empty(): {}", weather_data.is_empty());

    if weather_data.is_empty() {
        tracing::warn!("weather_data is empty, showing alert");
        show_alert("提示", "请先粘贴天气数据");
        return;
    }

    tracing::info!("weather_data has content, starting send");

    let data = weather_data.clone();
    tracing::info!("data to send: '{}'", data);

    wit_bindgen::block_on(async move {
        tracing::info!("inside block_on");

        match send_via_interconnect(&data).await {
            Ok(SendOutcome::Sent) => {
                tracing::info!("send_via_interconnect success");
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

fn send_weather_data_advanced() {
    let (host, key, location_id, location_name, days, use_custom_api) = {
        let state = ui_state().read().unwrap_or_else(|poisoned| poisoned.into_inner());
        (
            state.custom_api_host.clone(),
            state.custom_api_key.clone(),
            state.selected_location_id.clone(),
            state.selected_location_name.clone(),
            state.selected_days,
            state.use_custom_api,
        )
    };

    if !use_custom_api {
        show_alert("提示", "请先启用自定义API");
        return;
    }
    if host.trim().is_empty() || key.trim().is_empty() {
        show_alert("提示", "请先在设置中配置自定义API");
        return;
    }
    if location_id.is_empty() {
        show_alert("提示", "请先选择位置");
        return;
    }

    let days_segment = days_to_api_segment(days);
    let url = format!(
        "https://{}/v7/weather/{}?location={}&key={}",
        host.trim(),
        days_segment,
        location_id,
        key.trim()
    );
    tracing::info!("advanced weather url={}", url);

    match http_get_json(&url) {
        Ok(mut json) => {
            json["location"] = serde_json::Value::String(location_name);
            let payload = json.to_string();
            wit_bindgen::block_on(async move {
                match send_via_interconnect(&payload).await {
                    Ok(SendOutcome::Sent) => show_alert("成功", "发送成功"),
                    Ok(SendOutcome::Pending) => {
                        tracing::info!("send_via_interconnect pending; waiting for ready");
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
    tracing::info!("send_via_interconnect start, data={}", data);

    let devices = psys_host::device::get_connected_device_list().await;
    tracing::info!("get_connected_device_list returned {} devices", devices.len());

    if devices.is_empty() {
        return Err("没有连接的设备".to_string());
    }

    let device_addr = devices.first()
        .ok_or("没有连接的设备")?
        .addr.clone();

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
        update_last_sync_from_data(&data_str);
        return Ok(SendOutcome::Sent);
    }

    // Queue the payload and wait for ready via interconnect message callback.
    {
        let mut slot = pending_send().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        *slot = Some(PendingSend {
            device_addr: device_addr.clone(),
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
                HANDSHAKE_RUNNING.store(false, Ordering::SeqCst);
                update_last_sync_from_data(&pending.data);
                show_alert("成功", "发送成功");
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
                HANDSHAKE_RUNNING.store(false, Ordering::SeqCst);
                update_last_sync_from_data(&pending.data);
                show_alert("成功", "发送成功");
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
                state.use_custom_api = true;
                state.custom_api_host = host.trim().to_string();
                state.custom_api_key = key.trim().to_string();
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
        state.use_custom_api = false;
        state.custom_api_host.clear();
        state.custom_api_key.clear();
    }
    if let Err(e) = crate::ui::state::clear_api_settings() {
        show_alert("重置失败", &format!("清理配置失败: {}", e));
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
    tracing::info!("test_custom_api_connection url={}", url);
    let (status_code, body) = http_get_bytes(&url)?;
    tracing::info!(
        "test_custom_api_connection status={}, body_len={}",
        status_code,
        body.len()
    );
    log_body_preview("test_custom_api_connection", &body);

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
    let (host, key, query, use_custom_api) = {
        let state = ui_state().read().unwrap_or_else(|poisoned| poisoned.into_inner());
        (
            state.custom_api_host.clone(),
            state.custom_api_key.clone(),
            state.search_query.clone(),
            state.use_custom_api,
        )
    };

    if !use_custom_api {
        show_alert("提示", "请先启用自定义API");
        return;
    }
    if host.trim().is_empty() || key.trim().is_empty() {
        show_alert("提示", "请先在设置中配置自定义API");
        return;
    }
    if query.trim().is_empty() {
        show_alert("提示", "请输入城市名称");
        return;
    }

    let base = format!("https://{}/geo/v2/city/lookup", host.trim());
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
                    if !id.is_empty() {
                        results.push(LocationOption { id, name, adm1, adm2 });
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
    if let Some(item) = picked {
        state.selected_location_id = item.id;
        state.selected_location_name = item.name;
    }
    drop(state);
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

fn http_get_json(url: &str) -> Result<serde_json::Value, String> {
    tracing::info!("http_get_json url={}", url);
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
    tracing::info!("http_get_bytes url={}", url);
    let url = Url::parse(url).map_err(|e| e.to_string())?;
    let headers = http_types::Headers::from_list(&[])
        .map_err(|e| format!("{:?}", e))?;
    let req = http_types::OutgoingRequest::new(headers);

    req.set_method(&http_types::Method::Get)
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
    http_types::OutgoingBody::finish(outgoing_body, None)
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
                    continue;
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
    let preview_len = body.len().min(200);
    let preview = String::from_utf8_lossy(&body[..preview_len]);
    let hex_len = body.len().min(64);
    let mut hex = String::new();
    for b in &body[..hex_len] {
        hex.push_str(&format!("{:02x}", b));
    }
    tracing::info!("{} body_preview_utf8: {}", tag, preview);
    tracing::info!("{} body_preview_hex: {}", tag, hex);
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
