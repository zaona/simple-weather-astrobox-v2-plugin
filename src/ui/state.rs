use std::sync::{OnceLock, RwLock};

pub struct UiState {
    pub root_element_id: Option<String>,
    pub weather_data: String,
    pub deeplink_registered: bool, // 跟踪深度链接是否已注册
}

static UI_STATE: OnceLock<RwLock<UiState>> = OnceLock::new();

pub fn ui_state() -> &'static RwLock<UiState> {
    UI_STATE.get_or_init(|| {
        RwLock::new(UiState {
            root_element_id: None,
            weather_data: String::new(),
            deeplink_registered: false,
        })
    })
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