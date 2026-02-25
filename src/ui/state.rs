use std::sync::{OnceLock, RwLock};
use tracing::{info, warn};

const SETTINGS_FILE: &str = "api_settings.json";

pub struct UiState {
    pub root_element_id: Option<String>,
    pub weather_data: String,
    pub deeplink_registered: bool, // 跟踪深度链接是否已注册
    pub current_tab: MainTab,
    pub settings_page: SettingsPage,
    pub use_custom_api: bool,
    pub custom_api_host: String,
    pub custom_api_key: String,
    pub show_api_host: bool,
    pub show_api_key: bool,
    pub settings_loaded: bool,
    pub advanced_mode: bool,
    pub selected_days: u32,
    pub search_query: String,
    pub search_results: Vec<LocationOption>,
    pub selected_location_id: String,
    pub selected_location_name: String,
    pub selected_location_adm1: String,
    pub selected_location_adm2: String,
    pub selected_location_lat: String,
    pub selected_location_lon: String,
    pub selected_from_search: bool,
    pub recent_resolving: bool,
    pub recent_locations: Vec<LocationOption>,
    pub last_sync_time_ms: u64,
    pub last_sync_location: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MainTab {
    PasteData,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SettingsPage {
    Main,
    Api,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct LocationOption {
    pub id: String,
    pub name: String,
    pub adm1: String,
    pub adm2: String,
    #[serde(default)]
    pub lat: String,
    #[serde(default)]
    pub lon: String,
}

static UI_STATE: OnceLock<RwLock<UiState>> = OnceLock::new();

pub fn ui_state() -> &'static RwLock<UiState> {
    UI_STATE.get_or_init(|| {
        RwLock::new(UiState {
            root_element_id: None,
            weather_data: String::new(),
            deeplink_registered: false,
            current_tab: MainTab::PasteData,
            settings_page: SettingsPage::Main,
            use_custom_api: false,
            custom_api_host: String::new(),
            custom_api_key: String::new(),
            show_api_host: false,
            show_api_key: false,
            settings_loaded: false,
            advanced_mode: false,
            selected_days: 7,
            search_query: String::new(),
            search_results: Vec::new(),
            selected_location_id: String::new(),
            selected_location_name: String::new(),
            selected_location_adm1: String::new(),
            selected_location_adm2: String::new(),
            selected_location_lat: String::new(),
            selected_location_lon: String::new(),
            selected_from_search: false,
            recent_resolving: false,
            recent_locations: Vec::new(),
            last_sync_time_ms: 0,
            last_sync_location: String::new(),
        })
    })
}

#[derive(serde::Serialize, serde::Deserialize)]
struct StoredApiSettings {
    use_custom_api: bool,
    custom_api_host: String,
    custom_api_key: String,
    #[serde(default)]
    advanced_mode: bool,
    #[serde(default)]
    selected_days: u32,
    #[serde(default)]
    selected_location_id: String,
    #[serde(default)]
    selected_location_name: String,
    #[serde(default)]
    selected_location_adm1: String,
    #[serde(default)]
    selected_location_adm2: String,
    #[serde(default)]
    selected_location_lat: String,
    #[serde(default)]
    selected_location_lon: String,
    #[serde(default)]
    recent_locations: Vec<LocationOption>,
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
                state.advanced_mode = stored.advanced_mode;
                state.selected_days = if stored.selected_days == 0 { 7 } else { stored.selected_days };
                state.selected_location_id = stored.selected_location_id;
                state.selected_location_name = stored.selected_location_name;
                state.selected_location_adm1 = stored.selected_location_adm1;
                state.selected_location_adm2 = stored.selected_location_adm2;
                state.selected_location_lat = stored.selected_location_lat;
                state.selected_location_lon = stored.selected_location_lon;
                state.recent_locations = stored.recent_locations;
                if state.selected_location_id.is_empty() {
                    let first = state.recent_locations.first().cloned();
                    if let Some(first) = first {
                        state.selected_location_id = first.id;
                        state.selected_location_name = first.name;
                        state.selected_location_adm1 = first.adm1;
                        state.selected_location_adm2 = first.adm2;
                        state.selected_location_lat = first.lat;
                        state.selected_location_lon = first.lon;
                    }
                }
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
        advanced_mode: false,
        selected_days: 7,
        selected_location_id: String::new(),
        selected_location_name: String::new(),
        selected_location_adm1: String::new(),
        selected_location_adm2: String::new(),
        selected_location_lat: String::new(),
        selected_location_lon: String::new(),
        recent_locations: Vec::new(),
    };

    let content = serde_json::to_string_pretty(&stored).map_err(|e| e.to_string())?;
    std::fs::write(SETTINGS_FILE, content).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn save_all_settings() -> Result<(), String> {
    let state = ui_state().read().unwrap_or_else(|poisoned| poisoned.into_inner());
    let stored = StoredApiSettings {
        use_custom_api: state.use_custom_api,
        custom_api_host: state.custom_api_host.clone(),
        custom_api_key: state.custom_api_key.clone(),
        advanced_mode: state.advanced_mode,
        selected_days: state.selected_days,
        selected_location_id: state.selected_location_id.clone(),
        selected_location_name: state.selected_location_name.clone(),
        selected_location_adm1: state.selected_location_adm1.clone(),
        selected_location_adm2: state.selected_location_adm2.clone(),
        selected_location_lat: state.selected_location_lat.clone(),
        selected_location_lon: state.selected_location_lon.clone(),
        recent_locations: state.recent_locations.clone(),
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
