//! 手机端（插件）把手环已配置的自定义背景图推过去，逐条对齐安卓
//! `ImageSyncManager`：
//!
//! - `sendImage`：RGB_565 → PNG → 3072 字节分片 → `header` / `data…` / `end`，
//!   随后等手环回 `image_saved`（超时 30 秒）；期间手环回 `cancel` 立即中止
//! - `syncAllImages`：按码表顺序逐张「缩放 → 模糊 → 压暗 → RGB565 → PNG」后覆盖式发送，
//!   某张失败只记一次失败继续下一张，手环取消则整轮结束
//! - `clearAllOnWatch`：`clear_all` → 等 `clear_done`（超时 30 秒）
//! - `cancelTransfer`：发 `cancel`
//!
//! 与安卓的差别只在收发方式：安卓用 `MessageApi` 的监听器收手环回执，插件这边手环
//! 消息统一走 `on_event(interconnect-message)`，所以回执先落进 `MAILBOX`，发送循环用
//! [`crate::sleep`] 轮询取件（等待期间宿主仍能把其它事件派进来）。

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

use crate::astrobox::psys_host_v4::{interconnect, register, thirdpartyapp};
use crate::sleep::sleep;

use super::protocol::{self, Ack, CHUNK_SIZE};
use super::session::{self, Phase, Progress};
use super::{QUICK_APP_PAGE, QUICK_APP_PKG, edit, store};

/// 等手环回执的超时，对齐安卓 `withTimeout(30_000)`
const ACK_TIMEOUT: Duration = Duration::from_secs(30);
/// 回执轮询间隔，越小越跟手，代价是定时器事件更密
const ACK_POLL: Duration = Duration::from_millis(50);
/// 握手：安卓端发 5 次 `start`，每次等 1 秒
const HANDSHAKE_ATTEMPTS: usize = 5;
const HANDSHAKE_WAIT: Duration = Duration::from_secs(1);
/// 每发多少片刷一次界面，避免整轮传输把宿主 UI 拖住
const REPAINT_EVERY_CHUNKS: usize = 8;

/// 手环回执邮箱。安卓端每次 `sendImage` 都新挂一个监听器，这里换成一块共享邮箱，
/// 发送循环取件时按天气编号过滤，等价于「这张图的 ack 才唤醒这张图的等待」。
#[derive(Default)]
struct Mailbox {
    image_saved: Option<String>,
    clear_done: bool,
    cancelled: bool,
    ready: bool,
}

static MAILBOX: Mutex<Mailbox> = Mutex::new(Mailbox {
    image_saved: None,
    clear_done: false,
    cancelled: false,
    ready: false,
});

/// 有一轮传输在跑
static TRANSFER_ACTIVE: AtomicBool = AtomicBool::new(false);
/// 用户按了「取消传输」
static CANCEL_REQUESTED: AtomicBool = AtomicBool::new(false);
/// 传输途中又有新的同步请求（比如刚拖完滑块），当前这轮结束后补一轮
static RESTART_REQUESTED: AtomicBool = AtomicBool::new(false);
/// 当前传输的目标设备，取消时要往它发 `cancel`
static ACTIVE_ADDR: Mutex<Option<String>> = Mutex::new(None);

fn mailbox() -> std::sync::MutexGuard<'static, Mailbox> {
    MAILBOX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn is_active() -> bool {
    TRANSFER_ACTIVE.load(Ordering::SeqCst)
}

/// 手环回执入口。返回 `true` 表示这条消息属于背景图传输（含握手 `ready`），
/// 宿主不必再当成普通快应用消息记日志。
pub fn deliver_ack(payload: &str) -> bool {
    match protocol::parse_ack(payload) {
        Ack::ImageSaved { weather_code } => {
            tracing::info!("收到手环回执 image_saved: {}", weather_code);
            mailbox().image_saved = Some(weather_code);
            true
        }
        Ack::ClearDone => {
            tracing::info!("收到手环回执 clear_done");
            mailbox().clear_done = true;
            true
        }
        Ack::Cancelled => {
            tracing::info!("手环端已取消背景图传输");
            mailbox().cancelled = true;
            true
        }
        Ack::Ready => {
            tracing::info!("收到手环 ready");
            mailbox().ready = true;
            true
        }
        Ack::Foreign => false,
    }
}

/// 用户主动取消：通知手环，并让发送循环在下一个检查点退出。
pub async fn cancel() {
    if !is_active() {
        return;
    }
    CANCEL_REQUESTED.store(true, Ordering::SeqCst);
    if let Some(addr) = active_addr() {
        match send_raw(&addr, protocol::cancel_payload()).await {
            Ok(()) => tracing::info!("已通知手环取消背景图传输"),
            Err(e) => tracing::warn!("通知手环取消失败: {}", e),
        }
    }
}

/// 传输途中收到新的同步请求时的处理：不打断当前这轮，结束后补一轮，
/// 效果等于安卓的「覆盖传输模式」——最后一次改动一定落地。
pub fn request_restart() {
    RESTART_REQUESTED.store(true, Ordering::SeqCst);
}

/// 一轮同步的结果，语义对齐安卓 `syncAllImages` 的返回值
#[derive(Debug, Clone, Default)]
pub struct Outcome {
    pub sent: usize,
    pub failed: usize,
    pub cancelled: bool,
    /// 提前中止的原因（比如第一张就等不到手环回执）
    pub error: Option<String>,
}

impl Outcome {
    fn is_total_failure(&self) -> bool {
        self.failed > 0 && self.sent == 0
    }
}

#[derive(Debug)]
enum SendError {
    /// 手环端或用户取消
    Cancelled,
    /// 等回执超时
    Timeout(String),
    /// 发送本身失败
    Failed(String),
}

impl SendError {
    fn message(&self) -> String {
        match self {
            SendError::Cancelled => "已取消".to_string(),
            SendError::Timeout(reason) => reason.clone(),
            SendError::Failed(reason) => reason.clone(),
        }
    }
}

/// 把本地已配置的背景图覆盖式全部推给手环。
///
/// 没有任何已配置的图时返回错误，由调用方决定提示文案；
/// 传输途中再次调用只会记一次「补一轮」请求。
pub async fn sync_all() -> Result<Outcome, String> {
    if is_active() {
        request_restart();
        return Err("已有背景图传输在进行".to_string());
    }

    let codes = configured_codes();
    if codes.is_empty() {
        return Err("没有已配置的背景图".to_string());
    }

    let Some(addr) = super::resolve_device_addr().await else {
        return Err("没有连接的设备，请先在 AstroBox 连接手表".to_string());
    };

    let mut darken = super::current_edit_settings().0;
    let mut blur = super::current_edit_settings().1;
    let advanced = super::advanced_sync_mode();
    let mut codes = codes;

    // 先标记「传输中」：进度卡上的「取消传输」从这一刻起有效
    TRANSFER_ACTIVE.store(true, Ordering::SeqCst);
    CANCEL_REQUESTED.store(false, Ordering::SeqCst);
    RESTART_REQUESTED.store(false, Ordering::SeqCst);
    *ACTIVE_ADDR
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(addr.clone());
    clear_mailbox();

    // 对齐安卓：先确认手表端装了快应用，再注册接收（不然等不到 image_saved）
    session::set_progress(Progress {
        phase: Phase::Sending,
        message: "正在准备传输…".to_string(),
        ..Progress::default()
    });
    crate::ui::build::rerender_main_ui();
    if let Err(reason) = prepare(&addr).await {
        release_transfer();
        if is_cancelled() {
            finish_progress(Phase::Cancelled, "传输已取消");
            return Ok(Outcome {
                cancelled: true,
                ..Outcome::default()
            });
        }
        finish_progress(Phase::Failed, &reason);
        return Err(reason);
    }

    let outcome = loop {
        let round = run_sync(&addr, &codes, darken, blur, advanced).await;
        // 传输期间又有改动（比如刚拖完滑块），用最新参数再覆盖一轮
        if !RESTART_REQUESTED.swap(false, Ordering::SeqCst) || round.cancelled {
            break round;
        }
        tracing::info!("传输期间又有新的改动，按最新参数再同步一轮");
        CANCEL_REQUESTED.store(false, Ordering::SeqCst);
        clear_mailbox();
        let settings = super::current_edit_settings();
        darken = settings.0;
        blur = settings.1;
        codes = configured_codes();
        if codes.is_empty() {
            break round;
        }
    };

    release_transfer();

    if outcome.cancelled {
        finish_progress(Phase::Cancelled, "传输已取消");
        return Ok(outcome);
    }
    if let Some(reason) = outcome.error.clone() {
        finish_progress(Phase::Failed, &reason);
        return Err(reason);
    }
    if outcome.is_total_failure() {
        finish_progress(Phase::Failed, "所有图片发送失败");
        return Err("所有图片发送失败".to_string());
    }
    finish_progress(
        Phase::Finished,
        &format!("成功发送 {} 张背景图", outcome.sent),
    );
    Ok(outcome)
}

/// 通知手环清除所有自定义背景图，对齐安卓 `clearAllOnWatch`
pub async fn clear_all_on_watch() -> Result<(), String> {
    if is_active() {
        return Err("已有背景图传输在进行".to_string());
    }
    let Some(addr) = super::resolve_device_addr().await else {
        return Err("没有连接的设备，请先在 AstroBox 连接手表".to_string());
    };

    session::set_progress(Progress {
        phase: Phase::Clearing,
        message: "正在请求清除…".to_string(),
        ..Progress::default()
    });
    if let Err(reason) = prepare(&addr).await {
        finish_progress(Phase::Failed, &reason);
        return Err(reason);
    }

    TRANSFER_ACTIVE.store(true, Ordering::SeqCst);
    CANCEL_REQUESTED.store(false, Ordering::SeqCst);
    *ACTIVE_ADDR
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(addr.clone());
    clear_mailbox();

    // 对齐安卓「清除自定义背景图」流程：高级同步模式开启时同样先拉起快应用并握手
    if super::advanced_sync_mode() {
        session::set_progress(Progress {
            phase: Phase::Clearing,
            message: "正在启动快应用并握手…".to_string(),
            ..Progress::default()
        });
        crate::ui::build::rerender_main_ui();
        if let Err(reason) = handshake_device(&addr).await {
            if is_cancelled() {
                release_transfer();
                finish_progress(Phase::Cancelled, "传输已取消");
                return Ok(());
            }
            // 与推送一致：握手只作探活，拿不到 ready 也继续尝试清除
            tracing::warn!("{}，继续尝试清除背景图", reason);
        }
    }

    session::set_progress(Progress {
        phase: Phase::Clearing,
        message: "正在清除自定义背景图...".to_string(),
        ..Progress::default()
    });

    let result = match send_raw(&addr, protocol::clear_all_payload()).await {
        Ok(()) => wait_for_clear_done(ACK_TIMEOUT).await,
        Err(reason) => Err(SendError::Failed(reason)),
    };

    release_transfer();

    match result {
        Ok(()) => {
            finish_progress(Phase::Finished, "已清除所有自定义背景图");
            Ok(())
        }
        Err(e) => {
            finish_progress(Phase::Failed, &e.message());
            Err(e.message())
        }
    }
}

/// 已配置的天气编号，按码表顺序（对齐安卓 `WEATHER_BG_CODES.filter`）
pub fn configured_codes() -> Vec<(String, String)> {
    let saved = store::custom_codes();
    protocol::WEATHER_BG_CODES
        .iter()
        .filter(|(code, _)| saved.iter().any(|item| item == code))
        .map(|(code, label)| (code.to_string(), label.to_string()))
        .collect()
}

/// 把失败原因写进传输状态卡片：同步还没真正开始就失败时（没连设备、没装快应用等），
/// 由定时器入口调用，用户才能看到原因。
pub fn report_failure(message: &str) {
    finish_progress(Phase::Failed, message);
}

/// 同步前的准备，对齐安卓那两步固定的前置检查：
/// 「检查应用安装」+ 注册接收（安卓挂监听器，这里订阅快应用消息）。
async fn prepare(addr: &str) -> Result<(), String> {
    match thirdpartyapp::get_thirdparty_app_list(addr.to_string()).await {
        Ok(apps) if apps.iter().any(|app| app.package_name == QUICK_APP_PKG) => {}
        Ok(_) => return Err("手表端未安装「简明天气」快应用".to_string()),
        // 安卓端拿不到应用列表会直接抛错，这里沿用天气同步的做法：查不到就先放过
        Err(e) => tracing::warn!("读取快应用列表失败，按已安装处理: {}", e),
    }
    register::register_interconnect_recv(addr.to_string(), QUICK_APP_PKG.to_string())
        .await
        .map_err(|e| format!("注册背景图接收失败: {}", e))
}

/// 结束一轮传输：清掉「进行中」与目标设备
fn release_transfer() {
    TRANSFER_ACTIVE.store(false, Ordering::SeqCst);
    *ACTIVE_ADDR
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
}

async fn run_sync(
    addr: &str,
    codes: &[(String, String)],
    darken: u32,
    blur: u32,
    advanced: bool,
) -> Outcome {
    let total = codes.len();
    let mut outcome = Outcome::default();

    if advanced {
        session::set_progress(Progress {
            phase: Phase::Sending,
            message: "正在启动快应用并握手…".to_string(),
            ..Progress::default()
        });
        crate::ui::build::rerender_main_ui();
        match handshake_device(addr).await {
            Ok(()) => tracing::info!("握手完成，开始传输背景图"),
            Err(reason) => {
                if is_cancelled() {
                    return Outcome {
                        cancelled: true,
                        ..Outcome::default()
                    };
                }
                // 握手只是探活：拿不到 ready 也别把整轮掐了，快应用可能已经在跑，
                // 真正能证明链路通的是第一张图的 image_saved
                tracing::warn!("{}，继续尝试发送背景图", reason);
            }
        }
    }

    // 对齐安卓 syncAllImages：逐张从原件重新出图，再分片发送
    for (index, (code, label)) in codes.iter().enumerate() {
        if is_cancelled() {
            outcome.cancelled = true;
            return outcome;
        }

        session::set_progress(Progress {
            phase: Phase::Sending,
            weather_code: code.clone(),
            label: label.clone(),
            current: index + 1,
            total,
            received: 0,
            total_chunks: 0,
            message: format!("发送: {} ({}/{})", label, index + 1, total),
        });
        crate::ui::build::rerender_main_ui();

        let Some(source) = store::load_source(code) else {
            tracing::warn!("{} 没有原件，跳过发送", code);
            outcome.failed += 1;
            continue;
        };
        let processed = match edit::process(&source, darken, blur) {
            Ok(bytes) => bytes,
            Err(reason) => {
                tracing::warn!("{} 出图失败，跳过发送: {}", code, reason);
                outcome.failed += 1;
                continue;
            }
        };
        let (width, height) = edit::decode(&processed)
            .map(|image| (image.width, image.height))
            .unwrap_or((0, 0));

        match send_image(
            addr,
            &processed,
            width,
            height,
            code,
            index + 1,
            total,
            label,
        )
        .await
        {
            Ok(()) => {
                outcome.sent += 1;
                tracing::info!("背景图已发送 {} ({}/{})", code, index + 1, total);
            }
            Err(SendError::Cancelled) => {
                outcome.cancelled = true;
                return outcome;
            }
            Err(e) => {
                tracing::warn!("背景图发送失败 {}: {}", code, e.message());
                outcome.failed += 1;
                // 第一张就等不到回执，说明手环那边根本没动静（没启动 / 收不到消息），
                // 接着发只会把 30 秒超时乘上十二张，这里直接收尾并给出原因
                if matches!(e, SendError::Timeout(_)) && outcome.sent == 0 {
                    outcome.error = Some(format!(
                        "手环没有回应（{}），请确认快应用已启动后重试",
                        label
                    ));
                    return outcome;
                }
            }
        }
    }

    outcome
}

/// 发一张图：header → data… → end，等手环 `image_saved`。对齐安卓 `sendImage`
async fn send_image(
    addr: &str,
    png: &[u8],
    width: u32,
    height: u32,
    weather_code: &str,
    current: usize,
    total: usize,
    label: &str,
) -> Result<(), SendError> {
    let total_chunks = png.len().div_ceil(CHUNK_SIZE).max(1);
    {
        let mut mailbox = mailbox();
        mailbox.image_saved = None;
        mailbox.cancelled = false;
    }

    send_raw(
        addr,
        protocol::header_payload(
            png.len(),
            total_chunks,
            width,
            height,
            weather_code,
            current,
            total,
            label,
        ),
    )
    .await
    .map_err(SendError::Failed)?;

    for index in 0..total_chunks {
        if is_cancelled() {
            return Err(SendError::Cancelled);
        }
        let start = index * CHUNK_SIZE;
        let end = (start + CHUNK_SIZE).min(png.len());
        let chunk = BASE64.encode(&png[start..end]);
        send_raw(addr, protocol::data_payload(index, &chunk))
            .await
            .map_err(SendError::Failed)?;

        session::set_progress(Progress {
            phase: Phase::Sending,
            weather_code: weather_code.to_string(),
            label: label.to_string(),
            current,
            total,
            received: index + 1,
            total_chunks,
            message: format!("发送: {} ({}/{})", label, current, total),
        });
        if (index + 1) % REPAINT_EVERY_CHUNKS == 0 || index + 1 == total_chunks {
            crate::ui::build::rerender_main_ui();
        }
    }

    send_raw(addr, protocol::end_payload())
        .await
        .map_err(SendError::Failed)?;

    wait_for_image_saved(weather_code, ACK_TIMEOUT).await
}

/// 对齐安卓 `performWatchHandshake`：拉起快应用后最多 5 次 `start`，每次等 1 秒。
/// 背景图推送与天气数据同步共用（见 [`super::handshake_device`]）。
pub async fn handshake_device(addr: &str) -> Result<(), String> {
    clear_mailbox();

    let apps = thirdpartyapp::get_thirdparty_app_list(addr.to_string())
        .await
        .map_err(|e| format!("读取快应用列表失败: {}", e))?;
    let Some(app) = apps
        .into_iter()
        .find(|app| app.package_name == QUICK_APP_PKG)
    else {
        return Err("手表端未安装「简明天气」快应用".to_string());
    };
    thirdpartyapp::launch_qa(addr.to_string(), app, QUICK_APP_PAGE.to_string())
        .await
        .map_err(|e| format!("启动快应用失败: {}", e))?;

    for attempt in 0..HANDSHAKE_ATTEMPTS {
        send_raw(addr, "start".to_string())
            .await
            .map_err(SendError::Failed)
            .map_err(|e| e.message())?;
        if wait_for_ready(HANDSHAKE_WAIT).await {
            return Ok(());
        }
        tracing::info!("握手第 {} 次未收到 ready", attempt + 1);
    }
    Err("握手失败：设备未响应".to_string())
}

async fn wait_for_ready(timeout: Duration) -> bool {
    let deadline = now_ms().saturating_add(timeout.as_millis() as u64);
    loop {
        if mailbox().ready {
            mailbox().ready = false;
            return true;
        }
        if now_ms() >= deadline || is_cancelled() {
            return false;
        }
        sleep(ACK_POLL).await;
    }
}

async fn wait_for_image_saved(weather_code: &str, timeout: Duration) -> Result<(), SendError> {
    let deadline = now_ms().saturating_add(timeout.as_millis() as u64);
    loop {
        if is_cancelled() {
            return Err(SendError::Cancelled);
        }
        {
            let mut mailbox = mailbox();
            if mailbox.image_saved.as_deref() == Some(weather_code) {
                mailbox.image_saved = None;
                return Ok(());
            }
        }
        if now_ms() >= deadline {
            return Err(SendError::Timeout(format!(
                "等待手环确认超时（{}）",
                weather_code
            )));
        }
        sleep(ACK_POLL).await;
    }
}

async fn wait_for_clear_done(timeout: Duration) -> Result<(), SendError> {
    let deadline = now_ms().saturating_add(timeout.as_millis() as u64);
    loop {
        if is_cancelled() {
            return Err(SendError::Cancelled);
        }
        if mailbox().clear_done {
            mailbox().clear_done = false;
            return Ok(());
        }
        if now_ms() >= deadline {
            return Err(SendError::Timeout("等待手环确认清除超时".to_string()));
        }
        sleep(ACK_POLL).await;
    }
}

fn is_cancelled() -> bool {
    if CANCEL_REQUESTED.load(Ordering::SeqCst) || mailbox().cancelled {
        CANCEL_REQUESTED.store(true, Ordering::SeqCst);
        return true;
    }
    false
}

fn clear_mailbox() {
    let mut mailbox = mailbox();
    *mailbox = Mailbox::default();
}

fn active_addr() -> Option<String> {
    ACTIVE_ADDR
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

async fn send_raw(addr: &str, payload: String) -> Result<(), String> {
    interconnect::send_qaic_message(addr.to_string(), QUICK_APP_PKG.to_string(), payload)
        .await
        .map_err(|e| e.to_string())
}

fn finish_progress(phase: Phase, message: &str) {
    let mut progress = session::progress();
    progress.phase = phase;
    progress.message = message.to_string();
    if phase != Phase::Failed {
        progress.received = progress.total_chunks;
    } else {
        progress.received = 0;
        progress.total_chunks = 0;
    }
    session::set_progress(progress);
    crate::ui::build::rerender_main_ui();
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 发送用的分片数算法必须和安卓一致：`(size + CHUNK_SIZE - 1) / CHUNK_SIZE`
    #[test]
    fn chunk_count_matches_android() {
        for (size, want) in [
            (0usize, 1usize),
            (1, 1),
            (CHUNK_SIZE, 1),
            (CHUNK_SIZE + 1, 2),
            (CHUNK_SIZE * 3, 3),
            (CHUNK_SIZE * 3 + 5, 4),
        ] {
            assert_eq!(size.div_ceil(CHUNK_SIZE).max(1), want, "size={}", size);
        }
    }

    /// 回执邮箱只认自己那一张图的 image_saved，别的编号不能提前唤醒
    #[test]
    fn mailbox_filters_by_weather_code() {
        clear_mailbox();
        assert!(deliver_ack(r#"{"type":"image_saved","weatherCode":"22"}"#));
        {
            let mailbox = mailbox();
            assert_ne!(mailbox.image_saved.as_deref(), Some("21"));
        }
        assert!(deliver_ack(r#"{"type":"image_saved","weatherCode":"21"}"#));
        {
            let mut mailbox = mailbox();
            assert_eq!(mailbox.image_saved.take().as_deref(), Some("21"));
        }
        clear_mailbox();
    }

    /// 接收协议的消息不能当成回执吞掉，否则双向都对不上
    #[test]
    fn receive_protocol_is_not_an_ack() {
        assert!(!deliver_ack(
            r#"{"type":"header","totalChunks":2,"weatherCode":"21"}"#
        ));
        assert!(!deliver_ack(
            r#"{"type":"data","index":0,"chunk":"aGVsbG8="}"#
        ));
        assert!(!deliver_ack(r#"{"type":"end"}"#));
        assert!(deliver_ack(r#"{"type":"clear_done"}"#));
        clear_mailbox();
    }
}
