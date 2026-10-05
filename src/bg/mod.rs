//! 自定义背景图接收能力，协议与安卓端 `ImageSyncManager` 完全一致。
//!
//! 手机端把处理好的 PNG 按 base64 分片发过来，插件落盘到 `bg/custom-bg-{code}.png`，
//! 再按同一套协议回执。宿主（AstroBox）承担底层传输，插件与安卓端是同等的对端，
//! 谁先发起都一样。
//!
//! 入口：[`handle_message`] 解析消息并驱动 [`session`] 状态机，返回需要回传给手机端的
//! [`session::Reply`]；[`send_reply`] 负责经 interconnect 发出去。

pub mod edit;
pub mod preset;
pub mod protocol;
pub mod session;
pub mod store;

use std::sync::Mutex;

use crate::astrobox::psys_host_v4::{device, dialog, interconnect, register, timer};

use protocol::Incoming;

/// 安卓快应用包名，图片来源与天气数据同属一个快应用
pub const QUICK_APP_PKG: &str = "com.application.zaona.weather";

const REGISTER_TIMER_PAYLOAD: &str = "bg_register";

/// 滑块改动后逐张重算的定时器 payload
const APPLY_TIMER_PAYLOAD: &str = "bg_apply";

/// 窗口尺寸轮询的定时器 payload
const RENDER_SIZE_POLL_PAYLOAD: &str = "bg_size_poll";

/// 轮询间隔：交互停止后 30 秒内每 2 秒问一次容器宽度
const RENDER_SIZE_POLL_MS: u64 = 2000;

/// 一次只重算一张，避免十二张一起跑把界面卡住
const APPLY_INTERVAL_MS: u64 = 60;

/// 分帧重算队列
#[derive(Default)]
struct ApplyQueue {
    pending: Vec<String>,
    darken: u32,
    blur: u32,
    done: usize,
    total: usize,
}

/// 选择器允许的最大图片体积，超过就直接拒绝，避免 wasm 内存被撑爆
const MAX_PICK_SIZE: usize = 12 * 1024 * 1024;

/// 最近一次成功注册的设备地址，回执时用它指定发送目标
static DEVICE_ADDR: Mutex<Option<String>> = Mutex::new(None);

static APPLY_QUEUE: Mutex<ApplyQueue> = Mutex::new(ApplyQueue {
    pending: Vec::new(),
    darken: 0,
    blur: 0,
    done: 0,
    total: 0,
});

pub fn is_apply_timer_payload(payload: &str) -> bool {
    payload == APPLY_TIMER_PAYLOAD
}

pub fn is_render_size_poll_payload(payload: &str) -> bool {
    payload == RENDER_SIZE_POLL_PAYLOAD
}

/// 排下一次窗口尺寸轮询
pub fn schedule_render_size_poll() {
    timer::set_timeout(RENDER_SIZE_POLL_MS, RENDER_SIZE_POLL_PAYLOAD);
}

/// 安排一次全量重算。已经在跑就并入当前这批参数。
pub fn schedule_apply_all(codes: Vec<String>, darken: u32, blur: u32) {
    let mut guard = APPLY_QUEUE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !guard.pending.is_empty() {
        guard.pending.clear();
    }
    guard.pending = codes;
    guard.darken = darken;
    guard.blur = blur;
    guard.done = 0;
    guard.total = guard.pending.len();
    drop(guard);
    timer::set_timeout(APPLY_INTERVAL_MS, APPLY_TIMER_PAYLOAD);
}

/// 定时器回调：重算一张。
/// 返回刚处理完的天气编号（还有剩余时为 `Some`），由调用方负责刷新 UI，
/// 这样 `bg` 模块不反向依赖 UI 层。
pub fn apply_next() -> Option<String> {
    let (code, darken, blur, remaining, total) = {
        let mut guard = APPLY_QUEUE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if guard.pending.is_empty() {
            return None;
        }
        let code = guard.pending.remove(0);
        guard.done += 1;
        (
            code,
            guard.darken,
            guard.blur,
            !guard.pending.is_empty(),
            guard.total,
        )
    };

    if let Err(e) = apply_edit(&code, darken, blur) {
        tracing::warn!("重算 {} 成品图失败: {}", code, e);
    }

    if remaining {
        timer::set_timeout(APPLY_INTERVAL_MS, APPLY_TIMER_PAYLOAD);
    } else {
        tracing::info!("全部 {} 张背景图已按当前参数重算", total);
    }

    Some(code)
}

fn cached_addr() -> Option<String> {
    DEVICE_ADDR
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

fn cache_addr(addr: &str) {
    let mut guard = DEVICE_ADDR
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if guard.as_deref() != Some(addr) {
        *guard = Some(addr.to_string());
    }
}

/// 注册接收要放到定时器里执行：on-load 结束前宿主不认为插件已加载完成，
/// 而读取设备列表会等待用户处理授权弹窗。
pub fn schedule_register() {
    timer::set_timeout(1, REGISTER_TIMER_PAYLOAD);
}

pub fn is_register_timer_payload(payload: &str) -> bool {
    payload == REGISTER_TIMER_PAYLOAD
}

/// 对所有已连接设备注册背景图消息接收。
pub async fn register_recv() {
    let devices = device::get_connected_device_list().await;
    if devices.is_empty() {
        tracing::info!("没有已连接设备，跳过背景图接收注册");
        return;
    }
    for dev in devices {
        match register::register_interconnect_recv(dev.addr.clone(), QUICK_APP_PKG.to_string())
            .await
        {
            Ok(_) => {
                tracing::info!("背景图接收注册成功: {}", dev.addr);
                cache_addr(&dev.addr);
            }
            Err(e) => tracing::warn!("背景图接收注册失败 {}: {}", dev.addr, e),
        }
    }
}

/// 处理一条手机端消息。返回需要回执的内容，非背景图协议的消息返回 `None`。
pub fn handle_message(payload: &str) -> Option<session::Reply> {
    match protocol::parse(payload) {
        Incoming::Header(header) => {
            if let Err(reason) = session::on_header(header) {
                tracing::warn!("header 处理失败: {}", reason);
            }
            None
        }
        Incoming::Data { index, chunk } => {
            if let Err(reason) = session::on_chunk(index, &chunk) {
                tracing::warn!("分片处理失败 index={}: {}", index, reason);
            }
            None
        }
        Incoming::End => match session::on_end() {
            Ok(reply) => {
                // 收到即出成品图：列表按成品图判定是否已配置，同时应用当前滑块
                if let Some(session::Reply::ImageSaved { weather_code }) = &reply {
                    let (darken, blur) = current_edit_settings();
                    if let Err(e) = apply_edit(weather_code, darken, blur) {
                        tracing::warn!("{} 收到后出图失败: {}", weather_code, e);
                    }
                }
                reply
            }
            Err(reason) => {
                tracing::warn!("结束处理失败: {}", reason);
                None
            }
        },
        Incoming::ClearAll => Some(session::on_clear_all()),
        Incoming::Cancel => Some(session::on_cancel("手机端已取消传输")),
        Incoming::Foreign => None,
    }
}

/// 用户在插件里主动取消：与手机端取消走同一条路径。
pub fn cancel_active() -> Option<session::Reply> {
    if session::is_receiving() {
        Some(session::on_cancel("已手动取消"))
    } else {
        None
    }
}

/// 用户主动清除全部自定义背景图。
pub fn clear_all() -> session::Reply {
    session::on_clear_all()
}

/// 已保存自定义背景图的天气编号
pub fn saved_codes() -> Vec<String> {
    store::custom_codes()
}

/// 按当前滑块值从原件重算成品图。
///
/// 每次都从 `bg/source-` 重新处理，滑块因此是可逆的：往回调不会叠加出痕迹。
pub fn apply_edit(code: &str, darken: u32, blur: u32) -> Result<(), String> {
    let source = store::load_source(code).ok_or_else(|| format!("{} 没有原件", code))?;
    let processed = edit::process(&source, darken, blur)?;
    let (width, height) = dimensions(&processed);
    store::save_processed(code, &store::label_of(code), width, height, &processed)
        .map_err(|e| format!("写入成品图失败: {}", e))
}

/// 生成预览用的 data URI。`full` 为真时出原图分辨率，否则出缩略图。
pub fn preview_data_uri(code: &str, darken: u32, blur: u32, full: bool) -> Result<String, String> {
    let source = store::load_source(code).ok_or_else(|| format!("{} 没有原件", code))?;
    let processed = edit::process(&source, darken, blur)?;
    if full {
        Ok(edit::data_uri(&processed))
    } else {
        let thumb = edit::thumbnail(&processed)?;
        Ok(edit::data_uri(&thumb))
    }
}

/// 读取成品图，供壁纸渲染直接消费
pub fn read_processed(code: &str) -> Option<Vec<u8>> {
    std::fs::read(store::saved_image_path(code)).ok()
}

/// 当前滑块设置，供收到图片后立即出图
pub fn current_edit_settings() -> (u32, u32) {
    let state = crate::ui::state::ui_state()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    (state.bg_darken, state.bg_blur)
}

/// 拉起系统图片选择器，把选中的图作为该天气编号的原件并立即按当前滑块出图。
///
/// 返回选中的文件名，用户取消时返回 `None`。
pub async fn pick_and_import(code: &str, darken: u32, blur: u32) -> Result<Option<String>, String> {
    if !protocol::is_known_code(code) {
        return Err(format!("未知天气编号: {}", code));
    }

    let picked = dialog::pick_file(
        dialog::PickConfig {
            read: true,
            copy_to: None,
        },
        dialog::FilterConfig {
            multiple: false,
            extensions: vec![
                "png".to_string(),
                "jpg".to_string(),
                "jpeg".to_string(),
                "webp".to_string(),
            ],
            default_directory: String::new(),
            default_file_name: String::new(),
        },
    )
    .await
    .map_err(|e| format!("打开图片选择器失败: {}", e))?;

    // 取消选择时宿主返回空结果
    if picked.name.is_empty() || picked.data.is_empty() {
        return Ok(None);
    }
    if picked.data.len() > MAX_PICK_SIZE {
        return Err(format!(
            "图片过大（{} MB），请选一张小于 {} MB 的",
            picked.data.len() / 1024 / 1024,
            MAX_PICK_SIZE / 1024 / 1024
        ));
    }

    let ext = store::extension_of(&picked.name);
    // 先解码校验，避免把不支持的格式存成原件
    let decoded = edit::decode(&picked.data)?;
    let (width, height) = (decoded.width, decoded.height);

    store::save_source(code, &ext, &picked.data).map_err(|e| format!("保存原件失败: {}", e))?;
    let processed = edit::process(&picked.data, darken, blur)?;
    store::save_processed(code, &picked.name, width, height, &processed)
        .map_err(|e| format!("写入成品图失败: {}", e))?;

    tracing::info!(
        "已导入 {} 的背景图 {} ({}x{})",
        code,
        picked.name,
        width,
        height
    );
    Ok(Some(picked.name))
}

/// 删除某张背景图（含原件）
pub fn remove(code: &str) {
    if protocol::is_known_code(code) {
        store::delete(code);
        tracing::info!("已删除 {} 的自定义背景图", code);
    }
}

fn dimensions(png_bytes: &[u8]) -> (u32, u32) {
    edit::decode(png_bytes)
        .map(|image| (image.width, image.height))
        .unwrap_or((0, 0))
}

/// 把回执发给手机端。
pub async fn send_reply(reply: session::Reply) {
    let payload = reply.to_payload();
    let Some(addr) = resolve_device_addr().await else {
        tracing::warn!("没有已连接设备，丢弃回执 {}", payload);
        return;
    };
    match interconnect::send_qaic_message(addr, QUICK_APP_PKG.to_string(), payload.clone()).await {
        Ok(_) => tracing::info!("回执已发送: {}", payload),
        Err(e) => tracing::warn!("回执发送失败 {}: {}", payload, e),
    }
}

async fn resolve_device_addr() -> Option<String> {
    if let Some(addr) = cached_addr() {
        return Some(addr);
    }
    let devices = device::get_connected_device_list().await;
    let addr = devices.first().map(|dev| dev.addr.clone());
    if let Some(addr) = addr.as_deref() {
        cache_addr(addr);
    }
    addr
}
