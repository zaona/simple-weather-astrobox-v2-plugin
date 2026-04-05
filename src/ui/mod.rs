pub mod api_client;
pub mod build;
pub mod event_handler;
pub mod state;

pub use build::render_main_ui;
pub use build::render_sync_card;
pub use event_handler::handle_interconnect_message;
pub use event_handler::ui_event_processor;

pub const SYNC_CARD_ID: &str = "simple-weather-last-sync";
pub const SYNC_CARD_NAME: &str = "简明天气 · 上次同步";
