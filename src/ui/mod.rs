pub mod api_client;
pub mod bg_page;
pub mod build;
pub mod event_handler;
pub mod icons;
pub mod state;

pub use build::render_main_ui;
pub use build::render_sync_card;
pub use event_handler::handle_interconnect_message;
pub use event_handler::on_background_transfer_committed;
pub use event_handler::refresh_background_code;
pub use event_handler::ui_event_processor;

pub const SYNC_CARD_ID: &str = "simple-weather-last-sync";
pub const SYNC_CARD_NAME: &str = "简明天气 · 上次同步";
