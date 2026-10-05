//! 背景图接收状态机。
//!
//! 手机端（安卓 `ImageSyncManager`）逐条发 header → data… → end，等待手环回
//! `image_saved`；`clear_all` 回 `clear_done`；任一侧取消回 `cancel`。这里按同样的
//! 顺序驱动本地文件写入，并把进度快照留给 UI 展示。
//!
//! 状态机本身不碰宿主接口：处理函数只返回 [`Reply`]，由 [`crate::bg`] 负责发出去，
//! 这样状态转换可以单独测试。

use std::fs::File;
use std::sync::{LazyLock, Mutex};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

use super::protocol::Header;
use super::store;

/// 需要回传给手机端的确认消息
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    ImageSaved { weather_code: String },
    ClearDone,
    Cancelled,
}

impl Reply {
    pub fn to_payload(&self) -> String {
        match self {
            Reply::ImageSaved { weather_code } => serde_json::json!({
                "type": "image_saved",
                "weatherCode": weather_code,
            })
            .to_string(),
            Reply::ClearDone => r#"{"type":"clear_done"}"#.to_string(),
            Reply::Cancelled => r#"{"type":"cancel"}"#.to_string(),
        }
    }
}

/// 传输阶段，语义对齐手环端 `image-service.js` 的 `phase`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Receiving,
    Saved,
    Clearing,
    Finished,
    Cancelled,
    Failed,
}

impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Phase::Idle => "idle",
            Phase::Receiving => "receiving",
            Phase::Saved => "saved",
            Phase::Clearing => "clearing",
            Phase::Finished => "finished",
            Phase::Cancelled => "cancelled",
            Phase::Failed => "error",
        }
    }
}

/// 传输进度快照，供背景图页签渲染
#[derive(Debug, Clone)]
pub struct Progress {
    pub phase: Phase,
    pub weather_code: String,
    pub label: String,
    pub current: usize,
    pub total: usize,
    pub received: usize,
    pub total_chunks: usize,
    pub message: String,
}

impl Default for Progress {
    fn default() -> Self {
        Progress {
            phase: Phase::Idle,
            weather_code: String::new(),
            label: String::new(),
            current: 0,
            total: 0,
            received: 0,
            total_chunks: 0,
            message: "等待传输".to_string(),
        }
    }
}

struct Receiving {
    header: Header,
    received: usize,
    file: File,
}

static RECEIVING: Mutex<Option<Receiving>> = Mutex::new(None);
static PROGRESS: LazyLock<Mutex<Progress>> = LazyLock::new(|| Mutex::new(Progress::default()));

fn receiving() -> std::sync::MutexGuard<'static, Option<Receiving>> {
    RECEIVING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn progress() -> Progress {
    PROGRESS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

/// 是否正在接收一张图片
pub fn is_receiving() -> bool {
    receiving().is_some()
}

pub fn on_header(header: Header) -> Result<Option<Reply>, String> {
    let code = header.weather_code.clone();

    // 上一张还没传完就来新图：丢弃半成品，避免两个文件句柄同时打开
    if let Some(previous) = receiving().take() {
        store::discard(&previous.header.weather_code);
        tracing::warn!(
            "上一张背景图未传完就被新 header 取代: {}",
            previous.header.weather_code
        );
    }

    let file = store::begin(&code).map_err(|e| format!("创建 {} 失败: {}", code, e))?;
    let label = if header.label.is_empty() {
        super::protocol::label_of(&code).to_string()
    } else {
        header.label.clone()
    };

    update_progress(|progress| {
        progress.phase = Phase::Receiving;
        progress.weather_code = code.clone();
        progress.label = label.clone();
        progress.current = header.current;
        progress.total = header.total;
        progress.received = 0;
        progress.total_chunks = header.total_chunks;
        progress.message = format!("接收: {} ({}/{})", label, header.current, header.total);
    });

    *receiving() = Some(Receiving {
        header,
        received: 0,
        file,
    });
    tracing::info!("开始接收背景图 {}", code);
    Ok(None)
}

pub fn on_chunk(index: usize, chunk: &str) -> Result<Option<Reply>, String> {
    let mut guard = receiving();
    let Some(current) = guard.as_mut() else {
        tracing::warn!("收到分片但没有 header 记录，丢弃 index={}", index);
        return Ok(None);
    };
    if index >= current.header.total_chunks {
        tracing::warn!(
            "分片越界: index={} totalChunks={}",
            index,
            current.header.total_chunks
        );
        return Ok(None);
    }

    let bytes = decode_chunk(chunk)?;
    store::append(&mut current.file, &bytes).map_err(|e| format!("写入分片失败: {}", e))?;
    current.received += 1;

    let received = current.received;
    let total_chunks = current.header.total_chunks;
    let weather_code = current.header.weather_code.clone();
    let label = current.header.label.clone();
    let current_index = current.header.current;
    let total = current.header.total;
    drop(guard);

    let percent = received.saturating_mul(100) / total_chunks.max(1);
    tracing::debug!(
        "背景图 {} 已保存 {} 分片, 进度 {}%",
        weather_code,
        received,
        percent
    );
    update_progress(|progress| {
        progress.received = received;
        progress.total_chunks = total_chunks;
        progress.message = format!("接收: {} ({}/{})", label, current_index, total);
    });
    Ok(None)
}

pub fn on_end() -> Result<Option<Reply>, String> {
    let Some(current) = receiving().take() else {
        tracing::warn!("收到 end 但没有接收中的图片，忽略");
        return Ok(None);
    };
    let Receiving {
        header,
        received,
        file,
    } = current;
    // 先关掉句柄，再改名半成品
    drop(file);

    if received < header.total_chunks {
        store::discard(&header.weather_code);
        fail_progress(format!(
            "图片保存失败（分片 {}/{}）",
            received, header.total_chunks
        ));
        return Err(format!("分片不完整: {}/{}", received, header.total_chunks));
    }

    let code = header.weather_code.clone();
    let label = if header.label.is_empty() {
        super::protocol::label_of(&code).to_string()
    } else {
        header.label.clone()
    };
    let bytes = store::commit(
        &code,
        &label,
        header.width,
        header.height,
        header.total_size,
    );

    match bytes {
        Ok(bytes) => {
            let done_all = current_done(&code);
            update_progress(|progress| {
                progress.phase = if done_all {
                    Phase::Finished
                } else {
                    Phase::Saved
                };
                progress.received = progress.total_chunks;
                progress.message = if done_all {
                    format!("已完成 {}/{}", progress.current, progress.total)
                } else {
                    format!("已保存: {}", label)
                };
            });
            tracing::info!("背景图已保存 {} ({} 字节)", code, bytes);
            Ok(Some(Reply::ImageSaved { weather_code: code }))
        }
        Err(reason) => {
            store::discard(&code);
            fail_progress(format!("图片保存失败: {}", reason));
            Err(reason)
        }
    }
}

pub fn on_clear_all() -> Reply {
    if let Some(current) = receiving().take() {
        store::discard(&current.header.weather_code);
    }
    update_progress(|progress| {
        progress.phase = Phase::Clearing;
        progress.message = "正在清除自定义背景图...".to_string();
        progress.weather_code.clear();
        progress.label.clear();
        progress.current = 0;
        progress.total = 0;
        progress.received = 0;
        progress.total_chunks = 0;
    });
    let removed = store::clear_all();
    tracing::info!("已清除 {} 张自定义背景图", removed);
    update_progress(|progress| {
        progress.phase = Phase::Finished;
        progress.message = "已清除所有自定义背景图".to_string();
    });
    Reply::ClearDone
}

/// 手机端或用户取消：丢掉半成品并回执 `cancel`
pub fn on_cancel(reason: &str) -> Reply {
    let discarded = receiving()
        .take()
        .map(|current| current.header.weather_code);
    if let Some(code) = discarded.as_deref() {
        store::discard(code);
        tracing::info!("背景图传输已取消: {}", code);
    }
    update_progress(|progress| {
        progress.phase = Phase::Cancelled;
        progress.message = reason.to_string();
        progress.received = 0;
        progress.total_chunks = 0;
    });
    Reply::Cancelled
}

fn current_done(code: &str) -> bool {
    let progress = progress();
    progress.total > 0 && progress.current >= progress.total && progress.weather_code == code
}

fn fail_progress(message: String) {
    tracing::warn!("背景图传输失败: {}", message);
    update_progress(|progress| {
        progress.phase = Phase::Failed;
        progress.message = message.clone();
        progress.received = 0;
        progress.total_chunks = 0;
    });
}

fn update_progress(f: impl FnOnce(&mut Progress)) {
    let mut guard = PROGRESS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    f(&mut guard);
}

/// base64 分片解码。安卓端用 `Base64.NO_WRAP`，对应标准字母表且无换行。
fn decode_chunk(chunk: &str) -> Result<Vec<u8>, String> {
    let cleaned: String = chunk.chars().filter(|c| !c.is_whitespace()).collect();
    BASE64
        .decode(cleaned.as_bytes())
        .map_err(|e| format!("base64 解码失败: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_standard_base64() {
        assert_eq!(decode_chunk("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(decode_chunk("aGVs\nbG8=").unwrap(), b"hello");
        assert!(decode_chunk("!!!").is_err());
    }

    #[test]
    fn reply_payloads_match_android_protocol() {
        assert_eq!(
            Reply::ImageSaved {
                weather_code: "21".to_string()
            }
            .to_payload(),
            r#"{"type":"image_saved","weatherCode":"21"}"#
        );
        assert_eq!(Reply::ClearDone.to_payload(), r#"{"type":"clear_done"}"#);
        assert_eq!(Reply::Cancelled.to_payload(), r#"{"type":"cancel"}"#);
    }

    #[test]
    fn chunk_without_header_is_dropped() {
        let _ = receiving().take();
        assert!(on_chunk(0, "aGVsbG8=").unwrap().is_none());
    }
}
