use std::sync::{OnceLock, RwLock};
use tracing::{info, warn};

const SETTINGS_FILE: &str = "api_settings.json";

pub struct UiState {
    pub root_element_id: Option<String>,
    pub weather_data: String,
    pub deeplink_registered: bool, // 跟踪深度链接是否已注册
    pub current_tab: MainTab,
    pub use_custom_api: bool,
    pub custom_api_host: String,
    pub custom_api_key: String,
    pub show_api_host: bool,
    pub show_api_key: bool,
    pub settings_loaded: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MainTab {
    PasteData,
    CustomApi,
}

static UI_STATE: OnceLock<RwLock<UiState>> = OnceLock::new();

pub fn ui_state() -> &'static RwLock<UiState> {
    UI_STATE.get_or_init(|| {
        RwLock::new(UiState {
            root_element_id: None,
            weather_data: String::new(),
            deeplink_registered: false,
            current_tab: MainTab::PasteData,
            use_custom_api: false,
            custom_api_host: String::new(),
            custom_api_key: String::new(),
            show_api_host: false,
            show_api_key: false,
            settings_loaded: false,
        })
    })
}

#[derive(serde::Serialize, serde::Deserialize)]
struct StoredApiSettings {
    use_custom_api: bool,
    custom_api_host: String,
    custom_api_key: String,
}

pub fn load_api_settings_once() {
    let should_load = {
        let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.settings_loaded {
            false
        } else {
            state.settings_loaded = true;
            true
        }
    };

    if !should_load {
        return;
    }

    match std::fs::read_to_string(SETTINGS_FILE) {
        Ok(content) => match serde_json::from_str::<StoredApiSettings>(&content) {
            Ok(stored) => {
                let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
                state.use_custom_api = stored.use_custom_api;
                state.custom_api_host = stored.custom_api_host;
                state.custom_api_key = stored.custom_api_key;
                info!("loaded api settings from disk");
            }
            Err(e) => {
                warn!("failed to parse api settings: {}", e);
            }
        },
        Err(e) => {
            warn!("api settings not loaded: {}", e);
        }
    }
}

pub fn save_api_settings(use_custom_api: bool, host: &str, key: &str) -> Result<(), String> {
    let stored = StoredApiSettings {
        use_custom_api,
        custom_api_host: host.to_string(),
        custom_api_key: key.to_string(),
    };

    let content = serde_json::to_string_pretty(&stored).map_err(|e| e.to_string())?;
    std::fs::write(SETTINGS_FILE, content).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn clear_api_settings() -> Result<(), String> {
    match std::fs::remove_file(SETTINGS_FILE) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// 从deeplink设置天气数据
pub fn set_weather_data_from_deeplink(data: &str) {
    let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
    state.weather_data = data.to_string();
    // 只更新数据，不立即刷新UI，避免锁冲突
}

/// 设置深度链接注册状态
pub fn set_deeplink_registered(registered: bool) {
    let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
    state.deeplink_registered = registered;
}

/// 获取深度链接注册状态
pub fn is_deeplink_registered() -> bool {
    let state = ui_state().read().unwrap_or_else(|poisoned| poisoned.into_inner());
    state.deeplink_registered
}
