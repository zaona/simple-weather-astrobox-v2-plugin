//! 自定义背景图能力，协议与安卓端 `ImageSyncManager` 完全一致。
//!
//! 插件是安卓端同等的发送端：端上选好图（或导入 `.swbg`）后，[`sync`] 把成品 PNG 按
//! base64 分片推给手环快应用，手环回 `image_saved` / `clear_done` / `cancel`。
//! 另一个方向（手环推图过来）由 [`session`] 的接收状态机处理，两个方向共用同一套
//! 分片协议与同一份落盘布局，谁先发起都一样。
//!
//! 入口：[`handle_message`] 先把快应用消息按回执喂给发送流程，再按接收协议驱动
//! [`session`]；[`send_reply`] 负责把接收协议的回执经 interconnect 发出去。

pub mod edit;
pub mod preset;
pub mod protocol;
pub mod session;
pub mod store;
pub mod sync;

use std::sync::Mutex;

use crate::astrobox::psys_host_v4::{device, dialog, interconnect, register, timer};

use protocol::Incoming;

/// 安卓快应用包名，图片来源与天气数据同属一个快应用
pub const QUICK_APP_PKG: &str = "com.application.zaona.weather";

/// 拉起快应用时用的入口页，与天气同步保持一致
pub const QUICK_APP_PAGE: &str = "/index";

const REGISTER_TIMER_PAYLOAD: &str = "bg_register";

/// 滑块改动后逐张重算的定时器 payload
const APPLY_TIMER_PAYLOAD: &str = "bg_apply";

/// 安排一次「把手环要的背景图推过去」的定时器 payload
const PUSH_TIMER_PAYLOAD: &str = "bg_push";

/// 推送放到定时器里发起：UI 事件立即返回，宿主随后单独调度这轮传输
const PUSH_START_DELAY_MS: u64 = 1;

/// 一次只重算一张，避免十二张一起跑把界面卡住
const APPLY_INTERVAL_MS: u64 = 60;

/// 已经安排过一次推送，重复请求不再叠加定时器
static PUSH_SCHEDULED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

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

pub fn is_push_timer_payload(payload: &str) -> bool {
    payload == PUSH_TIMER_PAYLOAD
}

/// 安排一次「把已配置的背景图覆盖式推给手环」，对应背景图页那颗「发送到手表」。
///
/// 语义与安卓 `syncAllImages` 的覆盖传输模式一致：每次发送都把当前全部已配置图
/// 重新发一遍。传输中来的请求（说明页/连点等）不打断当前这轮，只记一笔，
/// 当前这轮结束后补一轮。
pub fn schedule_push() {
    if sync::is_active() {
        sync::request_restart();
        return;
    }
    if PUSH_SCHEDULED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    timer::set_timeout(PUSH_START_DELAY_MS, PUSH_TIMER_PAYLOAD);
}

/// 定时器回调：跑一轮推送
pub async fn run_scheduled_push() {
    PUSH_SCHEDULED.store(false, std::sync::atomic::Ordering::SeqCst);
    match sync::sync_all().await {
        Ok(_) => {}
        // 已经有一轮在跑：请求已经记在 sync 里，等它结束后补一轮，不必打扰用户
        Err(reason) if reason == "已有背景图传输在进行" => {}
        // 没有已配置的图是正常状态（安卓端这时弹的是「确认清除」），不写进状态卡
        Err(reason) if reason == "没有已配置的背景图" => {
            tracing::info!("没有已配置的背景图，跳过推送");
        }
        Err(reason) => {
            tracing::warn!("背景图推送未完成: {}", reason);
            sync::report_failure(&reason);
        }
    }
    crate::ui::build::rerender_main_ui();
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

/// 一条快应用消息的处理结果
pub enum Handled {
    /// 接收协议产生的回执，需要发回快应用
    Reply(session::Reply),
    /// 手环回执（image_saved / clear_done / cancel / ready），已经交给发送流程
    Ack,
    /// 不是背景图协议的消息，交给原来的日志处理
    Ignored,
}

/// 处理一条快应用消息。
///
/// 手环回执先交给发送流程（插件是安卓 `ImageSyncManager` 的对端，回执就是它要等的
/// `image_saved` / `clear_done` / `cancel`）；剩下的再按接收协议走，保证两个方向
/// 共用一套消息通道时互不干扰。
pub fn handle_message(payload: &str) -> Handled {
    if sync::deliver_ack(payload) {
        return Handled::Ack;
    }

    match protocol::parse(payload) {
        Incoming::Header(header) => {
            if let Err(reason) = session::on_header(header) {
                tracing::warn!("header 处理失败: {}", reason);
            }
            Handled::Ignored
        }
        Incoming::Data { index, chunk } => {
            if let Err(reason) = session::on_chunk(index, &chunk) {
                tracing::warn!("分片处理失败 index={}: {}", index, reason);
            }
            Handled::Ignored
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
                match reply {
                    Some(reply) => Handled::Reply(reply),
                    None => Handled::Ignored,
                }
            }
            Err(reason) => {
                tracing::warn!("结束处理失败: {}", reason);
                Handled::Ignored
            }
        },
        Incoming::ClearAll => Handled::Reply(session::on_clear_all()),
        Incoming::Cancel => Handled::Ignored,
        Incoming::Foreign => Handled::Ignored,
    }
}

/// 用户在插件里主动取消「取消传输」按钮。
///
/// 正在推图时通知手环并中止本地这轮（对齐安卓 `cancelTransfer`）；正在收图时
/// 丢弃半成品并回执 `cancel`。
pub async fn cancel_active() {
    if sync::is_active() {
        sync::cancel().await;
        return;
    }
    if session::is_receiving() {
        let reply = session::on_cancel("已手动取消");
        send_reply(reply).await;
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

/// 生成缩略图 data URI：1:1 填满的正方形（等比放大到铺满后居中裁剪），不做拉伸。
pub fn preview_data_uri(code: &str, darken: u32, blur: u32) -> Result<String, String> {
    let source = store::load_source(code).ok_or_else(|| format!("{} 没有原件", code))?;
    let processed = edit::process(&source, darken, blur)?;
    Ok(edit::data_uri(&edit::square_cover(
        &processed,
        edit::THUMBNAIL_SIZE,
    )?))
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

/// 对齐安卓 `advanced_sync_mode`：开启时推送前先拉起快应用并握手
pub fn advanced_sync_mode() -> bool {
    let state = crate::ui::state::ui_state()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    state.bg_advanced_sync_mode
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
