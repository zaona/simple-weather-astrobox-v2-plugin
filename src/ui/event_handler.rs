use crate::astrobox::psys_host;
use crate::astrobox::psys_host::interconnect;
use crate::astrobox::psys_host::register;
use crate::astrobox::psys_host::thirdpartyapp;
use crate::astrobox::psys_host::dialog;
use waki::Client;
use super::state::*;

pub const INPUT_CHANGE_EVENT: &str = "input_change";
pub const SEND_BUTTON_EVENT: &str = "send_button";
pub const OPEN_WEATHER_EVENT: &str = "open_weather";
pub const OPEN_GUIDE_EVENT: &str = "open_guide";
pub const TAB_PASTE_EVENT: &str = "tab_paste";
pub const TAB_CUSTOM_API_EVENT: &str = "tab_custom_api";
pub const CUSTOM_API_HOST_CHANGE_EVENT: &str = "custom_api_host_change";
pub const CUSTOM_API_KEY_CHANGE_EVENT: &str = "custom_api_key_change";
pub const API_SAVE_TEST_EVENT: &str = "api_save_test";
pub const API_RESET_EVENT: &str = "api_reset";
pub const API_HELP_EVENT: &str = "api_help";
pub const TOGGLE_SHOW_API_HOST_EVENT: &str = "toggle_show_api_host";
pub const TOGGLE_SHOW_API_KEY_EVENT: &str = "toggle_show_api_key";


pub fn handle_interconnect_message(payload: &str) {
    tracing::info!("收到快应用消息: {}", payload);
}

pub fn ui_event_processor(event_type: crate::exports::astrobox::psys_plugin::event::Event, event_id: &str, event_payload: &str) {
    tracing::info!("UI Event: type={:?}, id={}, payload={}", event_type, event_id, event_payload);

    match event_id {
        INPUT_CHANGE_EVENT => {
            let parsed_value = parse_event_value(event_payload);
            tracing::info!("INPUT_CHANGE_EVENT, value={}", parsed_value);
            let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
            state.weather_data = parsed_value.clone();
            tracing::info!("state.weather_data updated to: {}", parsed_value);
        }
        CUSTOM_API_HOST_CHANGE_EVENT => {
            let parsed_value = parse_event_value(event_payload);
            let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
            state.custom_api_host = parsed_value.clone();
            tracing::info!("state.custom_api_host updated to: {}", parsed_value);
        }
        CUSTOM_API_KEY_CHANGE_EVENT => {
            let parsed_value = parse_event_value(event_payload);
            let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
            state.custom_api_key = parsed_value.clone();
            tracing::info!("state.custom_api_key updated");
        }
        SEND_BUTTON_EVENT => {
            tracing::info!("SEND_BUTTON_EVENT received");
            send_weather_data();
        }
        TAB_PASTE_EVENT => {
            let should_rerender = {
                let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
                if state.current_tab != MainTab::PasteData {
                    state.current_tab = MainTab::PasteData;
                    true
                } else {
                    false
                }
            };
            if should_rerender {
                crate::ui::build::rerender_main_ui();
            }
        }
        TAB_CUSTOM_API_EVENT => {
            let should_rerender = {
                let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
                if state.current_tab != MainTab::CustomApi {
                    state.current_tab = MainTab::CustomApi;
                    true
                } else {
                    false
                }
            };
            if should_rerender {
                crate::ui::build::rerender_main_ui();
            }
        }
        OPEN_WEATHER_EVENT => {
            tracing::info!("Handling open weather event, checking deeplink permission first...");
            
            // 首先检查状态中是否已注册深度链接
            if crate::ui::state::is_deeplink_registered() {
                tracing::info!("DeepLink action already registered in state, opening weather website");
                open_weather_website();
            } else {
                // 在打开浏览器前检查并请求深度链接权限
                wit_bindgen::block_on(async move {
                    // 检查深度链接权限状态
                    match register::register_deeplink_action().await {
                        Ok(_) => {
                            tracing::info!("DeepLink action registered, updating state and opening weather website");
                            // 更新状态为已注册
                            crate::ui::state::set_deeplink_registered(true);
                            open_weather_website();
                        },
                        Err(e) => {
                            tracing::info!("DeepLink action not registered, showing permission dialog: {:?}", e);
                            
                            // 创建对话框配置（仅保留确认按钮）
                            let confirm_btn = dialog::DialogButton {
                                id: "confirm".to_string(),
                                primary: true,
                                content: "同意并启用".to_string(),
                            };
                            
                            let dialog_info = dialog::DialogInfo {
                                title: "深度链接权限请求".to_string(),
                                content: "该插件需要深度链接权限来接收天气数据。请点击\"同意并启用\"以允许插件接收外部应用发送的天气信息。".to_string(),
                                buttons: vec![confirm_btn],
                            };
                            
                            // 显示对话框请求用户授权（使用Website样式）
                            let _result = dialog::show_dialog(
                                dialog::DialogType::Alert,
                                dialog::DialogStyle::Website,
                                &dialog_info
                            ).await;
                            
                            // 用户点击了确认按钮（唯一的按钮）
                            tracing::info!("User confirmed deeplink permission request");
                            // 用户同意后，我们假设权限已经获得，直接打开网站
                            tracing::info!("User granted permission, opening weather website");
                            open_weather_website();
                        },
                    }
                });
            }
        }
        OPEN_GUIDE_EVENT => {
            open_guide_page();
        }
        API_SAVE_TEST_EVENT => {
            save_and_test_custom_api();
        }
        API_RESET_EVENT => {
            reset_custom_api();
        }
        API_HELP_EVENT => {
            open_api_help_page();
        }
        TOGGLE_SHOW_API_HOST_EVENT => {
            let should_rerender = {
                let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
                state.show_api_host = !state.show_api_host;
                true
            };
            if should_rerender {
                crate::ui::build::rerender_main_ui();
            }
        }
        TOGGLE_SHOW_API_KEY_EVENT => {
            let should_rerender = {
                let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
                state.show_api_key = !state.show_api_key;
                true
            };
            if should_rerender {
                crate::ui::build::rerender_main_ui();
            }
        }

        _ => {}
    }
}

fn parse_event_value(payload: &str) -> String {
    tracing::info!("parse_event_value input: {}", payload);
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(payload) {
        let value = json.get("value")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        tracing::info!("parse_event_value result: '{}'", value);
        value
    } else {
        tracing::warn!("parse_event_value failed to parse JSON");
        payload.to_string()
    }
}

fn send_weather_data() {
    tracing::info!("send_weather_data called");

    let weather_data = {
        let state = ui_state().read().unwrap_or_else(|poisoned| poisoned.into_inner());
        let value = state.weather_data.clone();
        tracing::info!("read state.weather_data: '{}'", value);
        value
    };

    tracing::info!("weather_data.is_empty(): {}", weather_data.is_empty());

    if weather_data.is_empty() {
        tracing::warn!("weather_data is empty, showing alert");
        show_alert("提示", "请先粘贴天气数据");
        return;
    }

    tracing::info!("weather_data has content, starting send");

    let data = weather_data.clone();
    tracing::info!("data to send: '{}'", data);

    wit_bindgen::block_on(async move {
        tracing::info!("inside block_on");

        match send_via_interconnect(&data).await {
            Ok(_) => {
                tracing::info!("send_via_interconnect success");
                show_alert("成功", "发送成功");
            }
            Err(e) => {
                tracing::error!("send_via_interconnect error: {}", e);
                show_alert("失败", &format!("发送失败: {}", e));
            }
        }
    });
}



async fn send_via_interconnect(data: &str) -> Result<(), String> {
    tracing::info!("send_via_interconnect start, data={}", data);

    let devices = psys_host::device::get_connected_device_list().await;
    tracing::info!("get_connected_device_list returned {} devices", devices.len());

    if devices.is_empty() {
        return Err("没有连接的设备".to_string());
    }

    let device_addr = devices.first()
        .ok_or("没有连接的设备")?
        .addr.clone();

    tracing::info!("using device: {}", device_addr);

    let pkg_name = "com.application.zaona.weather";

    tracing::info!("checking if quick app is installed...");
    match check_quick_app_installed(&device_addr, pkg_name).await {
        Ok(false) => {
            return Err("请先安装简明天气快应用".to_string());
        }
        Err(e) => {
            tracing::warn!("failed to check app list: {:?}, assuming app exists", e);
        }
        Ok(true) => {
            tracing::info!("quick app is installed");
        }
    }

    tracing::info!("launching quick app before send...");
    ensure_quick_app_launched(&device_addr, pkg_name, "/index").await?;

    tracing::info!("ensuring interconnect is registered for device: {}", device_addr);
    let _ = register::register_interconnect_recv(&device_addr, pkg_name).await;
    tracing::info!("register completed");

    let data_str = data.to_string();

    tracing::info!("calling interconnect::send_qaic_message...");
    interconnect::send_qaic_message(&device_addr, pkg_name, &data_str)
        .await
        .map_err(|e| format!("{:?}", e))?;

    tracing::info!("send_qaic_message completed");

    Ok(())
}

async fn check_quick_app_installed(device_addr: &str, pkg_name: &str) -> Result<bool, String> {
    tracing::info!("checking for package: {}", pkg_name);

    match thirdpartyapp::get_thirdparty_app_list(device_addr).await {
        Ok(app_list) => {
            tracing::info!("found {} apps", app_list.len());
            for app in &app_list {
                tracing::info!("  - {} ({})", app.app_name, app.package_name);
            }

            let found = app_list.iter().any(|app| app.package_name == pkg_name);
            tracing::info!("app {} found: {}", pkg_name, found);
            Ok(found)
        }
        Err(e) => {
            tracing::error!("failed to get app list: {:?}", e);
            Err(format!("{:?}", e))
        }
    }
}

async fn ensure_quick_app_launched(device_addr: &str, pkg_name: &str, page_name: &str) -> Result<(), String> {
    tracing::info!("ensure_quick_app_launched: pkg={}, page={}", pkg_name, page_name);

    let app_list = thirdpartyapp::get_thirdparty_app_list(device_addr)
        .await
        .map_err(|e| format!("{:?}", e))?;

    let app = match app_list.iter().find(|app| app.package_name == pkg_name) {
        Some(app) => app,
        None => {
            show_alert("未安装", "请先安装简明天气快应用");
            return Err("请先安装简明天气快应用".to_string());
        }
    };

    thirdpartyapp::launch_qa(device_addr, app, page_name)
        .await
        .map_err(|e| format!("{:?}", e))?;

    tracing::info!("quick app launched");
    Ok(())
}

fn open_weather_website() {
    tracing::info!("open_weather_website called");

    let url = "https://weather.zaona.top/weather";

    dialog::open_url(url);
    tracing::info!("opened weather website: {}", url);
}

fn open_guide_page() {
    tracing::info!("open_guide_page called");

    let url = "https://www.yuque.com/zaona/weather/plugin";

    dialog::open_url(url);
    tracing::info!("opened guide page: {}", url);
}

fn open_api_help_page() {
    tracing::info!("open_api_help_page called");

    let url = "https://www.yuque.com/zaona/weather/api";

    dialog::open_url(url);
    tracing::info!("opened api help page: {}", url);
}

fn show_alert(title: &str, message: &str) {
    tracing::info!("show_alert: title={}, message={}", title, message);

    let title_str = title.to_string();
    let message_str = message.to_string();

    wit_bindgen::block_on(async move {
        tracing::info!("show_alert executing dialog::show_dialog (Website style)");
        let _ = dialog::show_dialog(
            dialog::DialogType::Alert,
            dialog::DialogStyle::Website,
            &dialog::DialogInfo {
                title: title_str,
                content: message_str,
                buttons: vec![dialog::DialogButton {
                    id: "ok".to_string(),
                    primary: true,
                    content: "确定".to_string(),
                }],
            },
        ).await;
        tracing::info!("alert dialog closed");
    });
}

fn save_and_test_custom_api() {
    let (host, key) = {
        let state = ui_state().read().unwrap_or_else(|poisoned| poisoned.into_inner());
        (state.custom_api_host.clone(), state.custom_api_key.clone())
    };

    match test_custom_api_connection(&host, &key) {
        Ok(_) => {
            {
                let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
                state.use_custom_api = true;
                state.custom_api_host = host.trim().to_string();
                state.custom_api_key = key.trim().to_string();
            }
            if let Err(e) = crate::ui::state::save_api_settings(true, host.trim(), key.trim()) {
                show_alert("保存失败", &format!("写入配置失败: {}", e));
            } else {
                show_alert("保存成功", "配置已保存并验证通过");
            }
        }
        Err(message) => {
            show_alert("验证失败", &message);
        }
    }
}

fn reset_custom_api() {
    {
        let mut state = ui_state().write().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.use_custom_api = false;
        state.custom_api_host.clear();
        state.custom_api_key.clear();
    }
    if let Err(e) = crate::ui::state::clear_api_settings() {
        show_alert("重置失败", &format!("清理配置失败: {}", e));
    } else {
        show_alert("重置成功", "已恢复默认配置");
    }
    crate::ui::build::rerender_main_ui();
}

fn test_custom_api_connection(host: &str, key: &str) -> Result<(), String> {
    let host = host.trim();
    let key = key.trim();

    if host.is_empty() {
        return Err("API Host 不能为空".to_string());
    }
    if key.is_empty() {
        return Err("API Key 不能为空".to_string());
    }

    let url = format!("https://{}/geo/v2/city/lookup?location=北京&key={}", host, key);
    let client = Client::new();

    let response = client.get(&url).send().map_err(|e| format!("{:?}", e))?;

    let status_code = response.status_code();
    if status_code != 200 {
        return match status_code {
            401 => Err("API密钥无效或已过期".to_string()),
            403 => Err("访问被拒绝，请检查API权限".to_string()),
            _ => Err(format!("API Error: {}", status_code)),
        };
    }

    Ok(())
}
