use serde_json;
use wit_bindgen::FutureReader;

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
            _ => {
                tracing::info!("Unhandled event type: {:?}", event_type);
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
    }
}

export!(MyPlugin);
