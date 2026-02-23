use wit_bindgen::FutureReader;
use serde_json;

use crate::exports::astrobox::psys_plugin::{event, lifecycle};

pub mod logger;
pub mod ui;

wit_bindgen::generate!({
    path: "wit",
    world: "psys-world",
    generate_all,
});

struct MyPlugin;

impl event::Guest for MyPlugin {
    #[allow(async_fn_in_trait)]
    fn on_event(event_type: event::EventType, event_payload: _rt::String) -> FutureReader<String> {
        let (writer, reader) = wit_future::new::<String>(|| "".to_string());

        // 调试：打印所有事件类型和负载
        tracing::info!("DEBUG - event_type: {:?}, event_payload: {}", event_type, event_payload);

        // 处理所有事件，以便捕获可能的deeplink事件
        match event_type {
            event::EventType::InterconnectMessage => {
                ui::handle_interconnect_message(&event_payload);
            }
            event::EventType::Timer => {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&event_payload) {
                    if let Some(payload) = json.get("payload").and_then(|v| v.as_str()) {
                        crate::ui::event_handler::handle_timer_payload(payload);
                    } else {
                        tracing::info!("Timer event without payload field: {}", event_payload);
                    }
                } else if event_payload == "pending_send_timeout" {
                    crate::ui::event_handler::handle_timer_payload(&event_payload);
                } else {
                    tracing::info!("Timer event payload not JSON: {}", event_payload);
                }
            }
            event::EventType::DeeplinkAction => {
                // 处理deeplink数据
                tracing::info!("Received deeplink data via DeeplinkAction: {}", event_payload);
                // 将数据传递给UI状态
                ui::state::set_weather_data_from_deeplink(&event_payload);
            }
            // 尝试处理可能的其他命名变体
            _ => {
                // 检查事件负载是否包含deeplink相关数据
                if event_payload.contains("source=plugdata") {
                    tracing::info!("Received deeplink-like data in unknown event type: {:?}", event_type);
                    ui::state::set_weather_data_from_deeplink(&event_payload);
                }
            }
        }

        wit_bindgen::spawn(async move {
            let _ = writer.write("".to_string()).await;
        });

        reader
    }

    fn on_ui_event(
        event_id: _rt::String,
        event_type: event::Event,
        event_payload: _rt::String,
    ) -> wit_bindgen::rt::async_support::FutureReader<_rt::String> {
        let (writer, reader) = wit_future::new::<String>(|| "".to_string());

        ui::ui_event_processor(event_type, &event_id, &event_payload);

        wit_bindgen::spawn(async move {
            let _ = writer.write("".to_string()).await;
        });

        reader
    }

    fn on_ui_render(element_id: _rt::String) -> wit_bindgen::rt::async_support::FutureReader<()> {
        let (writer, reader) = wit_future::new::<()>(|| ());

        ui::render_main_ui(&element_id);

        wit_bindgen::spawn(async move {
            let _ = writer.write(()).await;
        });

        reader
    }

    fn on_card_render(_card_id: _rt::String) -> wit_bindgen::rt::async_support::FutureReader<()> {
        let (writer, reader) = wit_future::new::<()>(|| ());

        wit_bindgen::spawn(async move {
            let _ = writer.write(()).await;
        });

        reader
    }
}

impl lifecycle::Guest for MyPlugin {
    #[allow(async_fn_in_trait)]
    fn on_load() -> () {
        logger::init();
        tracing::info!("BUILD_ID=2026-02-24T02:55:00Z");
        tracing::info!("Simple Interconnect Plugin Loaded!");

        // 异步操作：获取设备列表并注册所有需要的服务
        wit_bindgen::spawn(async move {
            let devices = crate::astrobox::psys_host::device::get_connected_device_list().await;
            tracing::info!("on_load: found {} connected devices", devices.len());

            // 注册interconnect接收功能
            for device in &devices {
                tracing::info!("registering interconnect for device: {}", device.addr);
                let result = crate::astrobox::psys_host::register::register_interconnect_recv(
                    &device.addr,
                    "com.application.zaona.weather",
                ).await;
                tracing::info!("register interconnect result: {:?}", result);
            }
        });
        
        // 单独的异步任务：使用对话框请求用户授权注册deeplink action
        wit_bindgen::spawn(async move {
            tracing::info!("Attempting to register deeplink action directly...");
            
            // 先尝试直接注册，如果失败再显示授权对话框
            match crate::astrobox::psys_host::register::register_deeplink_action().await {
                Ok(_) => {
                    tracing::info!("DeepLink action registered successfully (no permission needed or already granted)");
                    crate::ui::state::set_deeplink_registered(true);
                }
                Err(e) => {
                    tracing::info!("Direct registration failed, showing permission dialog: {:?}", e);
                    
                    // 创建对话框按钮配置（仅保留确认按钮）
                    let confirm_btn = crate::astrobox::psys_host::dialog::DialogButton {
                        id: "confirm".to_string(),
                        primary: true,
                        content: "同意并启用".to_string(),
                    };
                    
                    // 创建对话框配置
                    let dialog_info = crate::astrobox::psys_host::dialog::DialogInfo {
                        title: "深度链接权限请求".to_string(),
                        content: "该插件需要深度链接权限来接收天气数据。请点击\"同意并启用\"以允许插件接收外部应用发送的天气信息。".to_string(),
                        buttons: vec![confirm_btn],
                    };
                    
                    // 显示对话框请求用户授权，使用Website样式以保持一致
                    let _result = crate::astrobox::psys_host::dialog::show_dialog(
                        crate::astrobox::psys_host::dialog::DialogType::Alert,
                        crate::astrobox::psys_host::dialog::DialogStyle::Website,
                        &dialog_info
                    ).await;
                    
                    // 用户点击了确认按钮（唯一的按钮）
                    tracing::info!("User confirmed deeplink permission request");
                    
                    // 用户同意后，再次尝试注册deeplink action
                    match crate::astrobox::psys_host::register::register_deeplink_action().await {
                        Ok(_) => {
                            tracing::info!("DeepLink action registered successfully with user consent");
                            crate::ui::state::set_deeplink_registered(true);
                        }
                        Err(e) => {
                            tracing::error!("Failed to register DeepLink action after user consent: {:?}", e);
                            // 注册失败时仅记录日志，不显示错误对话框
                        }
                    }
                }
            }
        });
    }
}

export!(MyPlugin);
