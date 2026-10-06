//! 自定义背景图传输协议，字段与安卓端 `ImageSyncManager.sendHeader` /
//! `sendMessageRaw` 逐字对齐。插件既按这套消息发（[`crate::bg::sync`]），也按这套消息收
//! （[`crate::bg::session`]）：
//!
//! ```text
//! header: {"type":"header","totalSize":N,"chunkSize":3072,"totalChunks":N,
//!          "width":N,"height":N,"weatherCode":"21","current":3,"total":12,"label":"阴-白天"}
//! data:   {"type":"data","index":N,"chunk":"<base64>"}
//! end:    {"type":"end"}
//! clear:  {"type":"clear_all"}
//! cancel: {"type":"cancel"}
//! ```
//!
//! 回执方向对齐安卓端监听的 `image_saved` / `clear_done` / `cancel`：发送时由
//! [`parse_ack`] 解析手环回包，接收时由 [`crate::bg::session::Reply`] 生成。
//!
//! 宿主转过来的字符串不一定就是快应用发出去的原样内容（可能包一层 `data` /
//! `payloadText`），所以解析前统一走 [`unwrap_message`] 剥壳。

/// 单张图片的分片大小，对齐安卓 `ImageSyncManager.CHUNK_SIZE`
pub const CHUNK_SIZE: usize = 3072;

/// 全部支持的自定义背景图编号及中文标签，对齐安卓端 `ImageSyncManager.WEATHER_BG_CODES`
/// 与手环端 `custom-bgs.ux` 的码表。
pub const WEATHER_BG_CODES: [(&str, &str); 12] = [
    ("21", "晴-白天"),
    ("22", "晴-夜晚"),
    ("23", "晴-日落"),
    ("11", "多云-白天"),
    ("12", "多云/阴-夜晚"),
    ("31", "阴-白天"),
    ("41", "雾霾-白天"),
    ("42", "雾霾-夜晚"),
    ("51", "雨-白天"),
    ("52", "雨-夜晚"),
    ("61", "雪-白天"),
    ("62", "雪-夜晚"),
];

/// 单张图片的分片数上限，防止畸形 header 撑爆内存。
const MAX_TOTAL_CHUNKS: usize = 4096;

/// 手环端（快应用 `image-service.js` / `connection-service.js`）回给手机端的消息。
///
/// 发送流程靠它推进：`image_saved` 表示这一张存好了可以发下一张，`clear_done`
/// 表示清除完成，`cancel` 表示手环端中止了本次传输，`ready` 是握手回执。
/// 逐条对齐安卓 `ImageSyncManager` 里 `OnMessageReceivedListener` 的判断。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ack {
    ImageSaved { weather_code: String },
    ClearDone,
    Cancelled,
    /// 快应用就绪（`{"action":"ready"}`）
    Ready,
    /// 不是手环回执
    Foreign,
}

/// 解析手环回执。安卓端是 `String(message).contains("ready")` 判握手、
/// 再按 `type` 分支，这里保持同样的宽松度，并额外兼容宿主转过来的包壳形式。
pub fn parse_ack(payload: &str) -> Ack {
    let trimmed = payload.trim();
    let Ok(raw) = serde_json::from_str::<serde_json::Value>(trimmed) else {
        // 非 JSON 消息：安卓端用 contains("ready") 判握手，这里保持一致的宽松度
        return if trimmed.contains("ready") {
            Ack::Ready
        } else {
            Ack::Foreign
        };
    };

    let ack = classify_ack(&raw);
    if ack != Ack::Foreign {
        return ack;
    }
    if let Some(inner) = unwrap_message(raw.clone(), 0) {
        let ack = classify_ack(&inner);
        if ack != Ack::Foreign {
            return ack;
        }
    }

    // 带 `type` 的协议消息上面已经排除了，能走到这里的基本是 ready 这类非协议消息；
    // 另外 base64 分片里出现 "ready" 字样是可能的，所以只对没有 type 的消息宽松判断
    let has_type = raw.get("type").and_then(|v| v.as_str()).is_some();
    if !has_type && trimmed.contains("ready") {
        return Ack::Ready;
    }
    Ack::Foreign
}

fn classify_ack(json: &serde_json::Value) -> Ack {
    if json.get("action").and_then(|v| v.as_str()) == Some("ready") {
        return Ack::Ready;
    }
    match json.get("type").and_then(|v| v.as_str()).unwrap_or("") {
        "image_saved" => Ack::ImageSaved {
            weather_code: weather_code_of(json),
        },
        "clear_done" => Ack::ClearDone,
        "cancel" => Ack::Cancelled,
        _ => Ack::Foreign,
    }
}

/// `weatherCode` 正常是字符串，快应用那边万一给了数字也要认（`21` / `"21"` 等价）
fn weather_code_of(json: &serde_json::Value) -> String {
    match json.get("weatherCode") {
        Some(serde_json::Value::String(text)) => text.trim().to_string(),
        Some(serde_json::Value::Number(number)) => number.to_string(),
        _ => String::new(),
    }
}

/// 认得出协议字段的对象：接收协议认 `type`，握手回执认 `action`
fn looks_like_message(value: &serde_json::Value) -> bool {
    value.get("type").and_then(|v| v.as_str()).is_some()
        || value.get("action").and_then(|v| v.as_str()).is_some()
}

/// 快应用侧是 `send({ data: ... })`，宿主转给插件的字符串在不同框架版本里包法不一样
/// （`{"data":{…}}`、`{"payloadText":"{…}"}` 都出现过）。这里逐层剥到认得出 `type` /
/// `action` 的那一层，最多 4 层，避免异常嵌套。
pub fn unwrap_message(value: serde_json::Value, depth: usize) -> Option<serde_json::Value> {
    if depth > 4 {
        return None;
    }
    if looks_like_message(&value) {
        return Some(value);
    }
    for key in ["data", "payloadText", "payload"] {
        let Some(inner) = value.get(key).cloned() else {
            continue;
        };
        let candidate = match inner {
            serde_json::Value::Object(_) => Some(inner),
            serde_json::Value::String(text) => {
                serde_json::from_str::<serde_json::Value>(text.trim()).ok()
            }
            _ => None,
        };
        if let Some(found) = candidate.and_then(|candidate| unwrap_message(candidate, depth + 1)) {
            return Some(found);
        }
    }
    None
}

/// 手机端主动发出的 header，字段与安卓 `ImageSyncManager.sendHeader` 逐字对齐。
pub fn header_payload(
    total_size: usize,
    total_chunks: usize,
    width: u32,
    height: u32,
    weather_code: &str,
    current: usize,
    total: usize,
    label: &str,
) -> String {
    serde_json::json!({
        "type": "header",
        "totalSize": total_size,
        "chunkSize": CHUNK_SIZE,
        "totalChunks": total_chunks,
        "width": width,
        "height": height,
        "weatherCode": weather_code,
        "current": current,
        "total": total,
        "label": label,
    })
    .to_string()
}

/// 手机端主动发出的 data 分片。`chunk` 用 `Base64.NO_WRAP` 的标准字母表，无换行。
pub fn data_payload(index: usize, chunk_base64: &str) -> String {
    serde_json::json!({
        "type": "data",
        "index": index,
        "chunk": chunk_base64,
    })
    .to_string()
}

pub fn end_payload() -> String {
    r#"{"type":"end"}"#.to_string()
}

/// 通知手环清除所有自定义背景图
pub fn clear_all_payload() -> String {
    r#"{"type":"clear_all"}"#.to_string()
}

/// 通知手环中止当前传输
pub fn cancel_payload() -> String {
    r#"{"type":"cancel"}"#.to_string()
}

/// header 里的图片元信息
#[derive(Debug, Clone)]
pub struct Header {
    pub total_size: usize,
    pub total_chunks: usize,
    pub width: u32,
    pub height: u32,
    pub weather_code: String,
    pub current: usize,
    pub total: usize,
    pub label: String,
}

/// 手机端发来的协议消息
#[derive(Debug, Clone)]
pub enum Incoming {
    Header(Header),
    Data {
        index: usize,
        chunk: String,
    },
    End,
    ClearAll,
    Cancel,
    /// 不是背景图协议的消息，交给其它处理逻辑
    Foreign,
}

/// 天气编号是否在白名单内。编号会拼进文件名，必须先校验，避免路径穿越。
pub fn is_known_code(code: &str) -> bool {
    WEATHER_BG_CODES.iter().any(|(known, _)| *known == code)
}

pub fn label_of(code: &str) -> &'static str {
    WEATHER_BG_CODES
        .iter()
        .find(|(known, _)| *known == code)
        .map(|(_, label)| *label)
        .unwrap_or("未知天气")
}

/// 解析一条 interconnect 消息。
pub fn parse(payload: &str) -> Incoming {
    let Ok(raw) = serde_json::from_str::<serde_json::Value>(payload) else {
        tracing::warn!("背景图消息不是合法 JSON: {}", payload);
        return Incoming::Foreign;
    };
    // 宿主转过来的字符串可能包了一层壳（见 unwrap_message），先剥到协议那一层
    let Some(json) = unwrap_message(raw, 0) else {
        return Incoming::Foreign;
    };

    match json.get("type").and_then(|v| v.as_str()).unwrap_or("") {
        "header" => match parse_header(&json) {
            Ok(header) => Incoming::Header(header),
            Err(reason) => {
                tracing::warn!("忽略非法 header: {}", reason);
                Incoming::Foreign
            }
        },
        "data" => {
            let index = json.get("index").and_then(value_as_usize);
            let chunk = json.get("chunk").and_then(|v| v.as_str()).unwrap_or("");
            match index {
                Some(index) if !chunk.is_empty() => Incoming::Data {
                    index,
                    chunk: chunk.to_string(),
                },
                _ => {
                    tracing::warn!("忽略非法 data 消息: index/chunk 缺失");
                    Incoming::Foreign
                }
            }
        }
        "end" => Incoming::End,
        "clear_all" => Incoming::ClearAll,
        "cancel" => Incoming::Cancel,
        _ => Incoming::Foreign,
    }
}

fn parse_header(json: &serde_json::Value) -> Result<Header, String> {
    let weather_code = json
        .get("weatherCode")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if !is_known_code(&weather_code) {
        return Err(format!("未知天气编号: {:?}", weather_code));
    }

    let total_chunks = json
        .get("totalChunks")
        .and_then(value_as_usize)
        .ok_or_else(|| "缺少 totalChunks".to_string())?;
    if total_chunks == 0 || total_chunks > MAX_TOTAL_CHUNKS {
        return Err(format!("totalChunks 越界: {}", total_chunks));
    }

    Ok(Header {
        total_size: json
            .get("totalSize")
            .and_then(value_as_usize)
            .unwrap_or_default(),
        total_chunks,
        width: json.get("width").and_then(value_as_u32).unwrap_or_default(),
        height: json
            .get("height")
            .and_then(value_as_u32)
            .unwrap_or_default(),
        weather_code,
        current: json
            .get("current")
            .and_then(value_as_usize)
            .unwrap_or_default(),
        total: json
            .get("total")
            .and_then(value_as_usize)
            .unwrap_or_default(),
        label: json
            .get("label")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string(),
    })
}

fn value_as_usize(value: &serde_json::Value) -> Option<usize> {
    match value {
        serde_json::Value::Number(number) => number.as_u64().map(|v| v as usize),
        serde_json::Value::String(text) => text.trim().parse::<usize>().ok(),
        _ => None,
    }
}

fn value_as_u32(value: &serde_json::Value) -> Option<u32> {
    value_as_usize(value).map(|v| v as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_header() {
        let payload = r#"{"type":"header","totalSize":1024,"chunkSize":3072,"totalChunks":2,
            "width":432,"height":514,"weatherCode":"21","current":3,"total":12,"label":"晴-白天"}"#;
        match parse(payload) {
            Incoming::Header(header) => {
                assert_eq!(header.weather_code, "21");
                assert_eq!(header.total_chunks, 2);
                assert_eq!(header.total_size, 1024);
                assert_eq!(header.current, 3);
                assert_eq!(header.label, "晴-白天");
            }
            other => panic!("期望 header, 实际 {:?}", other),
        }
    }

    #[test]
    fn rejects_unknown_weather_code() {
        let payload = r#"{"type":"header","totalChunks":1,"weatherCode":"../../etc/passwd"}"#;
        assert!(matches!(parse(payload), Incoming::Foreign));
    }

    #[test]
    fn parses_data_and_commands() {
        assert!(matches!(
            parse(r#"{"type":"data","index":0,"chunk":"AAA="}"#),
            Incoming::Data { index: 0, .. }
        ));
        assert!(matches!(parse(r#"{"type":"end"}"#), Incoming::End));
        assert!(matches!(
            parse(r#"{"type":"clear_all"}"#),
            Incoming::ClearAll
        ));
        assert!(matches!(parse(r#"{"type":"cancel"}"#), Incoming::Cancel));
    }

    #[test]
    fn treats_unknown_type_as_foreign() {
        assert!(matches!(parse(r#"{"type":"whatever"}"#), Incoming::Foreign));
        assert!(matches!(parse("not json"), Incoming::Foreign));
    }

    /// 发出去的消息必须和安卓 `JSONObject` 的字段一致，手环端才认得
    #[test]
    fn builds_android_shaped_payloads() {
        let header = header_payload(2048, 1, 432, 514, "21", 3, 12, "晴-白天");
        let json: serde_json::Value = serde_json::from_str(&header).unwrap();
        assert_eq!(json["type"], "header");
        assert_eq!(json["totalSize"], 2048);
        assert_eq!(json["chunkSize"], 3072);
        assert_eq!(json["totalChunks"], 1);
        assert_eq!(json["width"], 432);
        assert_eq!(json["height"], 514);
        assert_eq!(json["weatherCode"], "21");
        assert_eq!(json["current"], 3);
        assert_eq!(json["total"], 12);
        assert_eq!(json["label"], "晴-白天");

        let data = data_payload(7, "aGVsbG8=");
        let json: serde_json::Value = serde_json::from_str(&data).unwrap();
        assert_eq!(json["type"], "data");
        assert_eq!(json["index"], 7);
        assert_eq!(json["chunk"], "aGVsbG8=");

        // 命令类消息与安卓 `sendMessageRaw` 里手写的 JSON 一致
        assert_eq!(end_payload(), r#"{"type":"end"}"#);
        assert_eq!(clear_all_payload(), r#"{"type":"clear_all"}"#);
        assert_eq!(cancel_payload(), r#"{"type":"cancel"}"#);
    }

    /// 手环回执：image_saved 要取到编号，cancel / clear_done / ready 各归各位
    #[test]
    fn parses_watch_acks() {
        assert_eq!(
            parse_ack(r#"{"type":"image_saved","weatherCode":"21"}"#),
            Ack::ImageSaved {
                weather_code: "21".to_string()
            }
        );
        assert_eq!(parse_ack(r#"{"type":"clear_done"}"#), Ack::ClearDone);
        assert_eq!(parse_ack(r#"{"type":"cancel"}"#), Ack::Cancelled);
        assert_eq!(parse_ack(r#"{"action":"ready","timestamp":1}"#), Ack::Ready);
        // 安卓端用 contains("ready") 判断握手，这里保持一致的宽松度
        assert_eq!(parse_ack("ready"), Ack::Ready);
        assert_eq!(parse_ack(r#"{"type":"header","totalChunks":1}"#), Ack::Foreign);
        assert_eq!(parse_ack("not json"), Ack::Foreign);
    }

    /// 宿主把快应用的消息转过来时可能包一层：`{"data":{…}}` / `{"payloadText":"{…}"}`。
    /// 这两种形状在参考插件（Daymatter）的日志里都出现过，必须能剥开。
    #[test]
    fn parses_wrapped_watch_acks() {
        assert_eq!(
            parse_ack(r#"{"data":{"type":"image_saved","weatherCode":"22"}}"#),
            Ack::ImageSaved {
                weather_code: "22".to_string()
            }
        );
        assert_eq!(
            parse_ack(r#"{"payloadText":"{\"type\":\"clear_done\"}"}"#),
            Ack::ClearDone
        );
        assert_eq!(
            parse_ack(r#"{"data":{"action":"ready","timestamp":1}}"#),
            Ack::Ready
        );
        assert_eq!(
            parse_ack(r#"{"data":{"data":{"type":"cancel"}}}"#),
            Ack::Cancelled
        );
        // 编号给了数字也要认
        assert_eq!(
            parse_ack(r#"{"data":{"type":"image_saved","weatherCode":62}}"#),
            Ack::ImageSaved {
                weather_code: "62".to_string()
            }
        );
        // 包壳的接收协议消息也要能认出来，交给接收状态机
        assert!(matches!(
            parse(r#"{"data":{"type":"header","totalChunks":1,"weatherCode":"21"}}"#),
            Incoming::Header(_)
        ));
        // 认不出来的还是 Foreign，不能因为剥壳把别的消息吞掉
        assert_eq!(parse_ack(r#"{"data":{"hello":1}}"#), Ack::Foreign);
        assert!(matches!(parse(r#"{"data":{"hello":1}}"#), Incoming::Foreign));
    }
}
