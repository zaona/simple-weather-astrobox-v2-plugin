//! 自定义背景图的落盘与元信息。
//!
//! 每个天气编号存两份文件：
//!
//! ```text
//! bg/source-{code}.{ext}     原始图片（手机端发来的 PNG，或端上选择的图）
//! bg/custom-bg-{code}.png    按当前滑块处理好的成品图，预览与壁纸都用它
//! bg/source-{code}.{ext}.part 传输中的半成品，校验通过后才改名
//! bg/images.json             元信息
//! ```
//!
//! 滑块每次都从 `source-` 重算，因此往回调不会叠加出痕迹。安卓端是手机存原图 +
//! 手机存参数，手环只存成品；这里把原件留在手环，行为等价但改参数不用再传一遍图。

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const BG_DIR: &str = "bg";
const META_FILE: &str = "bg/images.json";
const FILE_PREFIX: &str = "custom-bg-";
const FILE_SUFFIX: &str = ".png";
const SOURCE_PREFIX: &str = "source-";
const PART_SUFFIX: &str = ".part";

/// PNG 文件头，用于校验分片拼出来的字节确实是图片
const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageMeta {
    pub code: String,
    pub label: String,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    pub updated_ms: u64,
    /// 原件扩展名，决定 `source-` 文件名
    #[serde(default = "default_ext")]
    pub ext: String,
}

fn default_ext() -> String {
    "png".to_string()
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct MetaFile {
    #[serde(default)]
    images: Vec<ImageMeta>,
}

fn bg_dir() -> PathBuf {
    PathBuf::from(BG_DIR)
}

/// 处理好的成品图
fn processed_path(code: &str) -> PathBuf {
    bg_dir().join(format!("{FILE_PREFIX}{code}{FILE_SUFFIX}"))
}

fn source_path(code: &str, ext: &str) -> PathBuf {
    let ext = normalize_ext(ext);
    bg_dir().join(format!("{SOURCE_PREFIX}{code}.{ext}"))
}

fn part_path(code: &str) -> PathBuf {
    source_path(code, "png").with_extension(format!("png{PART_SUFFIX}"))
}

/// 扩展名归一化，只留小写字母数字，避免拼出意外路径
fn normalize_ext(ext: &str) -> String {
    let cleaned: String = ext
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    if cleaned.is_empty() {
        "png".to_string()
    } else {
        cleaned
    }
}

/// 从文件名或选择器返回的名字里取扩展名
pub fn extension_of(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((_, ext)) => normalize_ext(ext),
        None => "png".to_string(),
    }
}

/// 成品图路径，供 UI 展示与调试使用
pub fn saved_image_path(code: &str) -> String {
    processed_path(code).to_string_lossy().to_string()
}

pub fn ensure_dir() -> std::io::Result<()> {
    fs::create_dir_all(bg_dir())
}

/// 开始接收一张图片：清掉同名半成品后新建文件
pub fn begin(code: &str) -> std::io::Result<File> {
    ensure_dir()?;
    let part = part_path(code);
    let _ = fs::remove_file(&part);
    File::create(&part)
}

/// 追加一个分片
pub fn append(file: &mut File, bytes: &[u8]) -> std::io::Result<()> {
    file.write_all(bytes)?;
    Ok(())
}

/// 校验字节数与 PNG 文件头，通过后把半成品改名为原件并记录元信息
pub fn commit(
    code: &str,
    label: &str,
    width: u32,
    height: u32,
    expected_bytes: usize,
) -> Result<u64, String> {
    let part = part_path(code);
    let bytes = fs::metadata(&part)
        .map_err(|e| format!("读取半成品大小失败: {}", e))?
        .len();
    if expected_bytes > 0 && bytes != expected_bytes as u64 {
        return Err(format!("大小不符: 期望 {} 实际 {}", expected_bytes, bytes));
    }

    let mut magic = [0u8; 8];
    {
        let mut file = File::open(&part).map_err(|e| format!("打开半成品失败: {}", e))?;
        file.read_exact(&mut magic)
            .map_err(|e| format!("读取 PNG 文件头失败: {}", e))?;
    }
    if magic != PNG_MAGIC {
        return Err("不是有效的 PNG 数据".to_string());
    }

    let target = source_path(code, "png");
    fs::rename(&part, &target).map_err(|e| format!("保存 {} 失败: {}", target.display(), e))?;
    save_meta(&ImageMeta {
        code: code.to_string(),
        label: label.to_string(),
        width,
        height,
        bytes,
        updated_ms: now_ms(),
        ext: "png".to_string(),
    });
    Ok(bytes)
}

/// 丢弃半成品
pub fn discard(code: &str) {
    let _ = fs::remove_file(part_path(code));
}

/// 保存端上选择的原件
pub fn save_source(code: &str, ext: &str, bytes: &[u8]) -> std::io::Result<()> {
    ensure_dir()?;
    fs::write(source_path(code, ext), bytes)
}

/// 读取原件。早期版本只存了成品图，这里回退读成品图，保证升级后滑块依然可用。
pub fn load_source(code: &str) -> Option<Vec<u8>> {
    let ext = metas()
        .into_iter()
        .find(|meta| meta.code == code)
        .map(|meta| meta.ext)
        .unwrap_or_else(|| "png".to_string());
    fs::read(source_path(code, &ext))
        .ok()
        .or_else(|| fs::read(processed_path(code)).ok())
}

/// 写入按当前滑块处理好的成品图
pub fn save_processed(
    code: &str,
    label: &str,
    width: u32,
    height: u32,
    bytes: &[u8],
) -> std::io::Result<()> {
    ensure_dir()?;
    fs::write(processed_path(code), bytes)?;
    let ext = metas()
        .into_iter()
        .find(|meta| meta.code == code)
        .map(|meta| meta.ext)
        .unwrap_or_else(|| "png".to_string());
    save_meta(&ImageMeta {
        code: code.to_string(),
        label: label.to_string(),
        width,
        height,
        bytes: bytes.len() as u64,
        updated_ms: now_ms(),
        ext,
    });
    Ok(())
}

/// 删除某张已保存的背景图（含原件）
pub fn delete(code: &str) {
    discard(code);
    let _ = fs::remove_file(processed_path(code));
    for ext in ["png", "jpg", "jpeg", "webp"] {
        let _ = fs::remove_file(source_path(code, ext));
    }
    let mut meta = load_meta();
    meta.images.retain(|item| item.code != code);
    store_meta(&meta);
}

/// 删除全部自定义背景图，返回删除的编号数
pub fn clear_all() -> usize {
    let codes = custom_codes();
    for code in &codes {
        delete(code);
    }
    // 兜底清掉传输中断留下的半成品与没有元信息的孤立原件
    if let Ok(entries) = fs::read_dir(bg_dir()) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let is_part = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.ends_with(PART_SUFFIX))
                .unwrap_or(false);
            let is_source = name.starts_with(SOURCE_PREFIX);
            if is_part || is_source {
                let _ = fs::remove_file(path);
            }
        }
    }
    codes.len()
}

/// 扫描目录，返回已保存自定义背景图的天气编号
pub fn custom_codes() -> Vec<String> {
    let mut codes = Vec::new();
    let Ok(entries) = fs::read_dir(bg_dir()) else {
        return codes;
    };
    for entry in entries.flatten() {
        if let Some(code) = code_from_processed_path(&entry.path()) {
            codes.push(code);
        }
    }
    codes.sort();
    codes
}

/// 已保存图片的元信息，按更新时间从新到旧
pub fn metas() -> Vec<ImageMeta> {
    let mut metas = load_meta().images;
    metas.sort_by(|a, b| b.updated_ms.cmp(&a.updated_ms));
    metas
}

/// 某个天气编号的显示名：有自定义图用元信息里的名字，否则用码表默认名
pub fn label_of(code: &str) -> String {
    metas()
        .into_iter()
        .find(|meta| meta.code == code)
        .map(|meta| meta.label)
        .filter(|label| !label.trim().is_empty())
        .unwrap_or_else(|| super::protocol::label_of(code).to_string())
}

fn code_from_processed_path(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    let code = name.strip_prefix(FILE_PREFIX)?.strip_suffix(FILE_SUFFIX)?.to_string();
    if super::protocol::is_known_code(&code) {
        Some(code)
    } else {
        None
    }
}

fn load_meta() -> MetaFile {
    fs::read_to_string(META_FILE)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

fn store_meta(meta: &MetaFile) {
    if meta.images.is_empty() {
        let _ = fs::remove_file(META_FILE);
        return;
    }
    if let Ok(content) = serde_json::to_string_pretty(meta) {
        let _ = fs::write(META_FILE, content);
    }
}

fn save_meta(entry: &ImageMeta) {
    let mut meta = load_meta();
    meta.images.retain(|item| item.code != entry.code);
    meta.images.push(entry.clone());
    store_meta(&meta);
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// 供 UI 判断某张图是否已存在的轻量集合
pub fn custom_code_set() -> HashSet<String> {
    custom_codes().into_iter().collect()
}