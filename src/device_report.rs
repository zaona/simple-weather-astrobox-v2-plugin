//! 用户设备信息上报，接口与安卓端 `DeviceReportService` 一致：
//! `POST /api/device/report`，body 为 `{ deviceId, deviceName }`。
//!
//! 插件启动时和每次同步成功后各上报一次。静默执行：没有已连接设备、拿不到宿主标识
//! 或请求失败时只记日志，不打扰用户，也不重试。

use std::sync::atomic::{AtomicBool, Ordering};

use crate::astrobox::psys_host_v4::{device, os, timer};
use crate::ui::{api_client, state};

const REPORT_PATH: &str = "/api/device/report";
const REPORT_TIMER_PAYLOAD: &str = "device_report";

static IN_FLIGHT: AtomicBool = AtomicBool::new(false);

/// 安排一次上报。放到定时器事件里执行，调用方立即返回：on-load 结束前宿主不认为插件
/// 已加载完成，同步流程也不该被上报的网络请求和首次授权弹窗拖住。
pub fn schedule_report() {
    timer::set_timeout(1, REPORT_TIMER_PAYLOAD);
}

pub fn is_report_timer_payload(payload: &str) -> bool {
    payload == REPORT_TIMER_PAYLOAD
}

pub async fn report_connected_device() {
    // 首次读取宿主标识会等待用户处理授权弹窗，期间再来的上报直接跳过，避免重复弹窗。
    if IN_FLIGHT.swap(true, Ordering::SeqCst) {
        return;
    }
    let outcome = try_report().await;
    IN_FLIGHT.store(false, Ordering::SeqCst);

    match outcome {
        Ok(Some(device_name)) => {
            tracing::info!("device reported successfully: {}", device_name);
        }
        Ok(None) => {
            tracing::info!("device report skipped: no connected device");
        }
        Err(e) => {
            tracing::warn!("device report failed: {}", e);
        }
    }
}

async fn try_report() -> Result<Option<String>, String> {
    let devices = device::get_connected_device_list().await;
    let Some(device_name) = devices
        .first()
        .map(|d| d.name.trim().to_string())
        .filter(|name| !name.is_empty())
    else {
        return Ok(None);
    };

    let device_id = os::device_id()
        .await
        .map_err(|e| format!("device-id unavailable: {}", e))?;

    let url = format!(
        "{}{}",
        state::server_api_base()?.trim_end_matches('/'),
        REPORT_PATH
    );
    let payload = serde_json::json!({
        "deviceId": device_id,
        "deviceName": device_name,
    });

    let response = api_client::post_json(&url, &payload)?;
    let code = response.get("code").and_then(|v| v.as_str());
    if code != Some("200") {
        let message = response
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error");
        return Err(format!("API Error: {}, message={}", code.unwrap_or("none"), message));
    }

    Ok(Some(device_name))
}
