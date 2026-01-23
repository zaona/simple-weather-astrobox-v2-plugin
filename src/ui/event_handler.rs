use crate::astrobox::psys_host;
use crate::astrobox::psys_host::interconnect;
use crate::astrobox::psys_host::register;
use super::state::*;
use super::build::build_main_ui;

pub fn handle_interconnect_message(payload: &str) {
    tracing::info!("收到快应用消息: {}", payload);
}

pub fn ui_event_processor(event_type: crate::exports::astrobox::psys_plugin::event::Event, event_id: &str, event_payload: &str) {
    tracing::info!("UI Event: type={:?}, id={}, payload={}", event_type, event_id, event_payload);

    match event_id {
        INPUT_CHANGE_EVENT => {
            let parsed_value = parse_event_value(event_payload);
            tracing::info!("INPUT_CHANGE_EVENT, value={}", parsed_value);
            let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
            state.input_value = parsed_value;
            state.message = None;
        }
        SEND_BUTTON_EVENT => {
            send_message();
        }
        _ => {}
    }
}

fn parse_event_value(payload: &str) -> String {
    tracing::info!("parse_event_value input: {}", payload);
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(payload) {
        let value = json.get("value")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        tracing::info!("parse_event_value result: {}", value);
        value
    } else {
        tracing::warn!("parse_event_value failed to parse JSON");
        payload.to_string()
    }
}

fn send_message() {
    tracing::info!("send_message called");

    let input_value = {
        let state = ui_state().read().unwrap_or_else(|poisoned| poisoned.into_inner());
        let value = state.input_value.clone();
        tracing::info!("current input_value: {}", value);
        value
    };

    if input_value.is_empty() {
        tracing::warn!("input_value is empty");
        show_message("请输入内容", false);
        return;
    }

    show_message("正在发送...", false);

    let data = input_value.clone();
    tracing::info!("starting async send, data={}", data);

    wit_bindgen::block_on(async move {
        tracing::info!("inside block_on");

        match send_via_interconnect(&data).await {
            Ok(_) => {
                tracing::info!("send_via_interconnect success");
                show_message("发送成功！", true);
                let root_id = {
                    let state = ui_state().read().unwrap_or_else(|poisoned| poisoned.into_inner());
                    state.root_element_id.clone()
                };
                if let Some(id) = root_id {
                    let ui = build_main_ui();
                    psys_host::ui::render(&id, ui);
                }
            }
            Err(e) => {
                tracing::error!("send_via_interconnect error: {}", e);
                show_message(&format!("发送失败: {}", e), false);
            }
        }
    });
}

async fn send_via_interconnect(data: &str) -> Result<(), String> {
    tracing::info!("send_via_interconnect start, data={}", data);

    let devices = psys_host::device::get_connected_device_list().await;
    tracing::info!("get_connected_device_list returned {} devices", devices.len());

    if devices.is_empty() {
        return Err("没有连接的设备".to_string());
    }

    let device_addr = devices.first()
        .ok_or("没有连接的设备")?
        .addr.clone();

    tracing::info!("ensuring interconnect is registered for device: {}", device_addr);

    let pkg_name = "com.application.zaona.weather";
    let _ = register::register_interconnect_recv(&device_addr, pkg_name).await;
    tracing::info!("register completed");

    let data_str = data.to_string();

    tracing::info!("calling interconnect::send_qaic_message...");
    interconnect::send_qaic_message(&device_addr, pkg_name, &data_str)
        .await
        .map_err(|e| format!("{:?}", e))?;

    tracing::info!("send_qaic_message completed");

    Ok(())
}

fn show_message(msg: &str, is_success: bool) {
    tracing::info!("show_message: {} (success={})", msg, is_success);
    let root_id = {
        let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.message = Some(msg.to_string());
        state.is_success = is_success;
        state.root_element_id.clone()
    };

    if let Some(id) = root_id {
        let ui = build_main_ui();
        psys_host::ui::render(&id, ui);
    }
}
