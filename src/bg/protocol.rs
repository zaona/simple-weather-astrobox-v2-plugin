//! 手机端自定义背景图传输协议，字段与安卓端 `ImageSyncManager.sendHeader` /
//! `sendMessageRaw` 逐字对齐：
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
//! 回执由 [`crate::bg::session::Reply`] 生成，同样对齐安卓端监听的
//! `image_saved` / `clear_done` / `cancel`。

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
    let Ok(json) = serde_json::from_str::<serde_json::Value>(payload) else {
        tracing::warn!("背景图消息不是合法 JSON: {}", payload);
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
}
