use std::sync::{OnceLock, RwLock};
use tracing::{info, warn};

const SETTINGS_FILE: &str = "api_settings.json";
const WEATHER_API_HOST: Option<&str> = option_env!("WEATHER_API_HOST");
const WEATHER_API_CLIENT_TYPE: Option<&str> = option_env!("WEATHER_API_CLIENT_TYPE");
const WEATHER_API_KEY: Option<&str> = option_env!("WEATHER_API_KEY");

fn default_bool_true() -> bool {
    true
}

pub struct UiState {
    pub root_element_id: Option<String>,
    pub current_tab: MainTab,
    pub settings_loaded: bool,
    pub sync_hourly_enabled: bool,
    pub sync_alerts_enabled: bool,
    pub selected_days: u32,
    pub search_query: String,
    pub search_results: Vec<CityLocation>,
    pub selected_location: Option<CityLocation>,
    pub selected_from_search: bool,
    pub recent_resolving: bool,
    pub recent_locations: Vec<CityLocation>,
    pub show_location_picker: bool,
    pub last_sync_time_ms: u64,
    pub last_sync_location: String,
}

pub fn server_api_base() -> Result<&'static str, String> {
    let host = WEATHER_API_HOST
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "WEATHER_API_HOST 未配置".to_string())?;

    Ok(host)
}

pub fn server_api_client_type() -> Result<&'static str, String> {
    let client_type = WEATHER_API_CLIENT_TYPE
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "WEATHER_API_CLIENT_TYPE 未配置".to_string())?;

    Ok(client_type)
}

pub fn server_api_key() -> Result<&'static str, String> {
    let api_key = WEATHER_API_KEY
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "WEATHER_API_KEY 未配置".to_string())?;

    Ok(api_key)
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MainTab {
    PasteData,
    Settings,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct CityLocation {
    pub id: String,
    pub name: String,
    pub adm1: String,
    pub adm2: String,
}

impl CityLocation {
    /// 显示名称，对齐 syncer-ng 的 `CityLocation.toString()`: "北京 (北京市 - 北京)"
    pub fn to_display_name(&self) -> String {
        if self.name.trim().is_empty() {
            return "未知位置".to_string();
        }
        if self.adm1.is_empty() && self.adm2.is_empty() {
            self.name.clone()
        } else {
            format!("{} ({} - {})", self.name, self.adm1, self.adm2)
                .trim()
                .to_string()
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn from_json(json: &str) -> Option<Self> {
        serde_json::from_str(json).ok()
    }
}

static UI_STATE: OnceLock<RwLock<UiState>> = OnceLock::new();

pub fn ui_state() -> &'static RwLock<UiState> {
    UI_STATE.get_or_init(|| {
        let state = UiState {
            root_element_id: None,
            current_tab: MainTab::PasteData,
            settings_loaded: false,
            sync_hourly_enabled: default_bool_true(),
            sync_alerts_enabled: default_bool_true(),
            selected_days: 7,
            search_query: String::new(),
            search_results: Vec::new(),
            selected_location: None,
            selected_from_search: false,
            recent_resolving: false,
            recent_locations: Vec::new(),
            show_location_picker: false,
            last_sync_time_ms: 0,
            last_sync_location: String::new(),
        };
        RwLock::new(state)
    })
}

#[derive(serde::Serialize, serde::Deserialize)]
struct StoredApiSettings {
    #[serde(default = "default_bool_true")]
    sync_hourly_enabled: bool,
    #[serde(default = "default_bool_true")]
    sync_alerts_enabled: bool,
    #[serde(default)]
    selected_days: u32,
    #[serde(default)]
    selected_location_name: String,
    #[serde(default)]
    selected_location_json: String,
    #[serde(default)]
    recent_locations: Vec<CityLocation>,
    #[serde(default)]
    last_sync_time_ms: u64,
    #[serde(default)]
    last_sync_location: String,
}

pub fn load_api_settings_once() {
    let should_load = {
        let mut state = ui_state()
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
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
                let mut state = ui_state()
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                state.sync_hourly_enabled = stored.sync_hourly_enabled;
                state.sync_alerts_enabled = stored.sync_alerts_enabled;
                state.selected_days = if stored.selected_days == 0 {
                    7
                } else {
                    stored.selected_days
                };
                state.selected_location =
                    CityLocation::from_json(&stored.selected_location_json);
                state.recent_locations = stored.recent_locations;
                state.last_sync_time_ms = stored.last_sync_time_ms;
                state.last_sync_location = stored.last_sync_location;
                if state.selected_location.is_none() {
                    let first = state.recent_locations.first().cloned();
                    if let Some(first) = first {
                        state.selected_location = Some(first);
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

pub fn save_all_settings() -> Result<(), String> {
    let state = ui_state()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let stored = StoredApiSettings {
        sync_hourly_enabled: state.sync_hourly_enabled,
        sync_alerts_enabled: state.sync_alerts_enabled,
        selected_days: state.selected_days,
        selected_location_name: state
            .selected_location
            .as_ref()
            .map(|l| l.to_display_name())
            .unwrap_or_default(),
        selected_location_json: state
            .selected_location
            .as_ref()
            .map(|l| l.to_json())
            .unwrap_or_default(),
        recent_locations: state.recent_locations.clone(),
        last_sync_time_ms: state.last_sync_time_ms,
        last_sync_location: state.last_sync_location.clone(),
    };

    let content = serde_json::to_string_pretty(&stored).map_err(|e| e.to_string())?;
    std::fs::write(SETTINGS_FILE, content).map_err(|e| e.to_string())?;
    Ok(())
}
