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

        match event_type {
            event::EventType::InterconnectMessage => {
                ui::handle_interconnect_message(&event_payload);
            }
            _ => {}
        }

        tracing::info!("event_type: {:?}, event_payload: {}", event_type, event_payload);

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
        tracing::info!("Simple Interconnect Plugin Loaded!");

        wit_bindgen::spawn(async {
            let devices = crate::astrobox::psys_host::device::get_connected_device_list().await;
            tracing::info!("on_load: found {} connected devices", devices.len());

            for device in &devices {
                tracing::info!("registering interconnect for device: {}", device.addr);
                let result = crate::astrobox::psys_host::register::register_interconnect_recv(
                    &device.addr,
                    "com.application.zaona.weather",
                ).await;
                tracing::info!("register result: {:?}", result);
            }
        });
    }
}

export!(MyPlugin);
