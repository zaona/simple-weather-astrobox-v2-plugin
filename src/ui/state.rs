use std::collections::HashMap;
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
    pub send_in_progress: bool,
    /// 每次开始或取消发送都会递增；进行中的发送发现代号变了就放弃后续步骤。
    pub send_generation: u64,
    pub send_status: String,
    /// 发送开始时提前刷新了同步卡片，取消时用它还原 (last_sync_time_ms, last_sync_location)。
    pub sync_card_backup: Option<(u64, String)>,
    /// 压暗强度 0-100，对齐安卓 `bg_darken_strength`
    pub bg_darken: u32,
    /// 模糊半径 0-100，对齐安卓 `bg_blur_radius`
    pub bg_blur: u32,
    /// 已保存自定义背景图的天气编号
    pub bg_codes: Vec<String>,
    /// 说明页是否打开
    pub bg_guide: bool,
    /// 灯箱当前打开的天气编号
    pub bg_lightbox: Option<String>,
    /// 灯箱里的原图 data URI
    pub bg_preview_uri: String,
    /// 每个天气编号的缩略图 data URI，生成一次后复用，避免每次重绘都重编码
    pub bg_thumbs: HashMap<String, String>,
    /// 插件 UI 渲染区宽度（CSS 像素），0 表示还没查到。用于桌面/移动两套版式。
    pub render_width: u32,
    pub render_height: u32,
    /// 最近一次交互时间戳，用于决定要不要继续轮询窗口尺寸
    pub last_ui_touch_ms: u64,
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

/// 卡片网格每列的最小宽度，与 bg_page 里的 grid columns 保持一致
pub const CARD_MIN_COLUMN: u32 = 188;
/// 卡片网格的列间距
pub const CARD_COLUMN_GAP: u32 = 10;
/// 排得下两列卡片才用卡片版式，否则退回列表
pub const CARDS_MIN_WIDTH: u32 = CARD_MIN_COLUMN * 2 + CARD_COLUMN_GAP;

impl UiState {
    /// 容器排得下两列卡片就走卡片版式，否则列表。宽度未知（0）时按列表处理。
    pub fn is_compact_layout(&self) -> bool {
        self.render_width < CARDS_MIN_WIDTH
    }
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
            send_in_progress: false,
            send_generation: 0,
            send_status: String::new(),
            sync_card_backup: None,
            bg_darken: 0,
            bg_blur: 0,
            bg_codes: Vec::new(),
            bg_guide: false,
            bg_lightbox: None,
            bg_preview_uri: String::new(),
            bg_thumbs: HashMap::new(),
            render_width: 0,
            render_height: 0,
            last_ui_touch_ms: 0,
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
    #[serde(default)]
    bg_darken: u32,
    #[serde(default)]
    bg_blur: u32,
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
                state.bg_darken = stored.bg_darken.min(100);
                state.bg_blur = stored.bg_blur.min(100);
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
        bg_darken: state.bg_darken,
        bg_blur: state.bg_blur,
    };

    let content = serde_json::to_string_pretty(&stored).map_err(|e| e.to_string())?;
    std::fs::write(SETTINGS_FILE, content).map_err(|e| e.to_string())?;
    Ok(())
}
