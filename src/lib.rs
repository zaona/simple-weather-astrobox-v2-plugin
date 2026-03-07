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
                // deeplink到达后临时切换到粘贴同步并自动发送，完成后恢复原模式
                ui::event_handler::handle_deeplink_payload(&event_payload);
            }
            // 尝试处理可能的其他命名变体
            _ => {
                // 检查事件负载是否包含deeplink相关数据
                if event_payload.contains("source=plugdata") {
                    tracing::info!("Received deeplink-like data in unknown event type: {:?}", event_type);
                    ui::event_handler::handle_deeplink_payload(&event_payload);
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

    fn on_card_render(card_id: _rt::String) -> wit_bindgen::rt::async_support::FutureReader<()> {
        let (writer, reader) = wit_future::new::<()>(|| ());

        tracing::info!("on_card_render called: {}", card_id);
        ui::render_sync_card(&card_id);

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
        let build_time = option_env!("AB_BUILD_TIME").unwrap_or("unknown");
        let build_user = option_env!("AB_BUILD_USER").unwrap_or("unknown");
        let build_hash = option_env!("AB_BUILD_GIT_HASH").unwrap_or("unknown");
        let build_branch = option_env!("AB_BUILD_GIT_BRANCH").unwrap_or("unknown");
        tracing::info!(
            "BUILD_INFO time={} user={} branch={} hash={}",
            build_time,
            build_user,
            build_branch,
            build_hash
        );
        tracing::info!("Simple Interconnect Plugin Loaded!");

        wit_bindgen::block_on(async move {
            let result = crate::astrobox::psys_host::register::register_card(
                crate::astrobox::psys_host::register::CardType::Text,
                crate::ui::SYNC_CARD_ID,
                crate::ui::SYNC_CARD_NAME,
            )
            .await;
            tracing::info!("register card result: {:?}", result);
        });

        // 单独的异步任务：使用对话框请求用户授权注册deeplink action
        wit_bindgen::block_on(async move {
            tracing::info!("Attempting to register deeplink action directly...");

            // 先尝试直接注册，如果失败再显示授权对话框
            match crate::astrobox::psys_host::register::register_deeplink_action().await {
                Ok(_) => {
                    tracing::info!("DeepLink action registered successfully (no permission needed or already granted)");
                    crate::ui::state::set_deeplink_registered(true);
                }
                Err(e) => {
                    tracing::info!("Direct registration failed, showing permission dialog: {:?}", e);

                    let dialog_info = crate::astrobox::psys_host::dialog::DialogInfo {
                        title: "深度链接权限请求".to_string(),
                        content: "简明天气同步器插件需要深度链接权限来接收天气数据。请前往插件详情中开启该权限，启用后插件才能正常工作。".to_string(),
                        buttons: vec![],
                    };

                    let _result = crate::astrobox::psys_host::dialog::show_dialog(
                        crate::astrobox::psys_host::dialog::DialogType::Alert,
                        crate::astrobox::psys_host::dialog::DialogStyle::Website,
                        &dialog_info
                    ).await;
                }
            }
        });
    }
}

export!(MyPlugin);
