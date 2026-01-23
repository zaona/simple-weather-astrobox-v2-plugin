use std::sync::{OnceLock, RwLock};

pub struct UiState {
    pub root_element_id: Option<String>,
    pub weather_data: String,
    pub message: Option<String>,
    pub is_success: bool,
}

static UI_STATE: OnceLock<RwLock<UiState>> = OnceLock::new();

pub fn ui_state() -> &'static RwLock<UiState> {
    UI_STATE.get_or_init(|| {
        RwLock::new(UiState {
            root_element_id: None,
            weather_data: String::new(),
            message: None,
            is_success: false,
        })
    })
}

pub const INPUT_CHANGE_EVENT: &str = "input_change";
pub const SEND_BUTTON_EVENT: &str = "send_button";
pub const OPEN_WEATHER_EVENT: &str = "open_weather";
pub const OPEN_GUIDE_EVENT: &str = "open_guide";
