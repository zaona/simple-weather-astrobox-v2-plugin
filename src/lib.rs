use crate::astrobox::psys_host_v4::{register, ui as host_ui};
use crate::exports::astrobox::psys_plugin_v4::{event, lifecycle};

pub mod logger;
pub mod sleep;
pub mod ui;

pub use sleep::sleep;

wit_bindgen::generate!({
    path: "wit",
    world: "psys-world-v4",
    generate_all,
});

struct MyPlugin;

impl event::Guest for MyPlugin {
    async fn on_event(event_type: event::EventType, event_payload: String) -> String {
        tracing::info!(
            "DEBUG - event_type: {:?}, event_payload: {}",
            event_type,
            event_payload
        );

        match event_type {
            event::EventType::InterconnectMessage => {
                ui::handle_interconnect_message(&event_payload);
            }
            event::EventType::Timer => {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&event_payload) {
                    if let Some(payload) = json.get("payload").and_then(|v| v.as_str()) {
                        if !sleep::handle_timer_payload(payload) {
                            crate::ui::event_handler::handle_timer_payload(payload);
                        }
                    } else {
                        tracing::info!("Timer event without payload field: {}", event_payload);
                    }
                } else if event_payload == "pending_send_timeout" {
                    crate::ui::event_handler::handle_timer_payload(&event_payload);
                } else {
                    tracing::info!("Timer event payload not JSON: {}", event_payload);
                }
            }
            _ => {
                tracing::info!("Unhandled event type: {:?}", event_type);
            }
        }

        String::new()
    }

    async fn on_ui_event(event_id: String, event_type: host_ui::Event, event_payload: String) -> String {
        ui::ui_event_processor(event_type, &event_id, &event_payload).await;
        String::new()
    }

    async fn on_ui_render(element_id: String) {
        ui::render_main_ui(&element_id);
    }

    async fn on_card_render(card_id: String) {
        tracing::info!("on_card_render called: {}", card_id);
        ui::render_sync_card(&card_id);
    }
}

impl lifecycle::Guest for MyPlugin {
    async fn on_load() {
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

        let result = register::register_card(
            register::CardType::Text,
            crate::ui::SYNC_CARD_ID.to_string(),
            crate::ui::SYNC_CARD_NAME.to_string(),
        )
        .await;
        tracing::info!("register card result: {:?}", result);
    }
}

export!(MyPlugin);
