//! `.swbg` 预设包的导入 / 导出。
//!
//! 格式与安卓端 `BackgroundPresetManager` 完全一致（`.swbg` 就是 ZIP）：
//!
//! ```text
//! manifest.json      元数据、全局处理设置、预设列表
//! images/21.png      原始图片，按 weatherCode 命名
//! ```
//!
//! 导出的是**原件**加参数，导入时按参数重新出成品图，所以两边来回导不会把画质越导越差。
//! 安卓独有的 `quality` / `advancedSyncMode` 字段照常读写以保持兼容，但插件端固定
//! RGB_565 画质，这两个参数不参与处理。

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};

use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// 与安卓端 `FORMAT_VERSION` 一致
pub const FORMAT_VERSION: u32 = 1;
/// 与安卓端 `FILE_EXTENSION` 一致
pub const FILE_EXTENSION: &str = "swbg";
const MANIFEST_ENTRY: &str = "manifest.json";
const IMAGES_DIR: &str = "images/";

/// 导入结果摘要，用于弹窗告知用户
#[derive(Debug, Default, Clone)]
pub struct ImportSummary {
    pub imported: usize,
    pub skipped: usize,
    pub darken: u32,
    pub blur: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PresetSettings {
    #[serde(default)]
    darken_strength: u32,
    #[serde(default)]
    blur_radius: u32,
    #[serde(default)]
    quality: u32,
}

impl Default for PresetSettings {
    fn default() -> Self {
        PresetSettings {
            darken_strength: 0,
            blur_radius: 0,
            quality: 85,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GlobalSettings {
    #[serde(default)]
    darken_strength: u32,
    #[serde(default)]
    blur_radius: u32,
    #[serde(default)]
    quality: u32,
    #[serde(default = "default_true")]
    advanced_sync_mode: bool,
}

fn default_true() -> bool {
    true
}

impl Default for GlobalSettings {
    fn default() -> Self {
        GlobalSettings {
            darken_strength: 0,
            blur_radius: 0,
            quality: 85,
            advanced_sync_mode: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PresetEntry {
    weather_code: String,
    #[serde(default)]
    weather_label: String,
    image_file: String,
    #[serde(default = "default_format")]
    image_format: String,
    #[serde(default)]
    original_file_name: String,
    #[serde(default)]
    settings: PresetSettings,
}

fn default_format() -> String {
    "png".to_string()
}

// manifest 的字段名用 camelCase，与安卓 Gson 的 @SerializedName 对齐
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PresetManifest {
    #[serde(rename = "formatVersion")]
    format_version: u32,
    #[serde(rename = "appVersion", default)]
    app_version: String,
    #[serde(rename = "exportTimestamp", default)]
    export_timestamp: u64,
    #[serde(default)]
    metadata: BTreeMap<String, String>,
    #[serde(rename = "globalSettings")]
    global_settings: GlobalSettings,
    presets: Vec<PresetEntry>,
}

/// 把当前已配置的背景图打包成 `.swbg` 字节
pub fn export(darken: u32, blur: u32) -> Result<Vec<u8>, String> {
    let metas = super::store::metas();
    if metas.is_empty() {
        return Err("没有已配置的背景图可导出".to_string());
    }

    let mut presets = Vec::new();
    let mut images: Vec<(String, Vec<u8>)> = Vec::new();
    for meta in &metas {
        let Some(source) = super::store::load_source(&meta.code) else {
            continue;
        };
        let ext = super::store::extension_of(&meta.ext);
        let entry_name = format!("{IMAGES_DIR}{}.{}", meta.code, ext);
        images.push((entry_name.clone(), source));
        presets.push(PresetEntry {
            weather_code: meta.code.clone(),
            weather_label: super::protocol::label_of(&meta.code).to_string(),
            image_file: entry_name,
            image_format: ext,
            original_file_name: meta.label.clone(),
            settings: PresetSettings {
                darken_strength: darken,
                blur_radius: blur,
                quality: 85,
            },
        });
    }

    if presets.is_empty() {
        return Err("没有可导出的原件".to_string());
    }

    let manifest = PresetManifest {
        format_version: FORMAT_VERSION,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        export_timestamp: now_ms(),
        metadata: BTreeMap::from([(
            "generator".to_string(),
            "simple-weather-astrobox-plugin".to_string(),
        )]),
        global_settings: GlobalSettings {
            darken_strength: darken,
            blur_radius: blur,
            quality: 85,
            advanced_sync_mode: true,
        },
        presets,
    };
    let manifest_json = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;

    let mut buffer = Cursor::new(Vec::new());
    {
        let mut zip = ZipWriter::new(&mut buffer);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

        for (name, bytes) in &images {
            zip.start_file(name.clone(), options)
                .map_err(|e| format!("写入 {} 失败: {}", name, e))?;
            zip.write_all(bytes).map_err(|e| format!("写入 {} 失败: {}", name, e))?;
        }

        zip.start_file(MANIFEST_ENTRY, options)
            .map_err(|e| format!("写入 manifest 失败: {}", e))?;
        zip.write_all(&manifest_json)
            .map_err(|e| format!("写入 manifest 失败: {}", e))?;

        zip.finish().map_err(|e| format!("打包失败: {}", e))?;
    }

    Ok(buffer.into_inner())
}

/// 解析 `.swbg` 并导入，返回导入摘要。用户取消选择时返回 `Ok(None)`。
pub async fn import_from_picker() -> Result<Option<ImportSummary>, String> {
    use crate::astrobox::psys_host_v4::dialog;

    let picked = dialog::pick_file(
        dialog::PickConfig {
            read: true,
            copy_to: None,
        },
        dialog::FilterConfig {
            multiple: false,
            extensions: vec![FILE_EXTENSION.to_string()],
            default_directory: String::new(),
            default_file_name: String::new(),
        },
    )
    .await
    .map_err(|e| format!("打开文件选择器失败: {}", e))?;

    if picked.name.is_empty() || picked.data.is_empty() {
        return Ok(None);
    }
    import(&picked.data).map(Some)
}

/// 解析 `.swbg` 字节并写入 `bg/`，按包内参数出成品图
pub fn import(bytes: &[u8]) -> Result<ImportSummary, String> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("不是有效的预设包: {}", e))?;

    let manifest_json = read_entry(&mut archive, MANIFEST_ENTRY)?
        .ok_or_else(|| "预设包中未找到 manifest.json".to_string())?;
    let manifest: PresetManifest = serde_json::from_slice(&manifest_json)
        .map_err(|e| format!("解析 manifest 失败: {}", e))?;

    if manifest.format_version > FORMAT_VERSION {
        tracing::warn!(
            "预设包版本 {} 高于当前支持的 {}，按兼容方式解析",
            manifest.format_version,
            FORMAT_VERSION
        );
    }
    if manifest.presets.is_empty() {
        return Err("预设包中没有图片".to_string());
    }

    let darken = manifest.global_settings.darken_strength.min(100);
    let blur = manifest.global_settings.blur_radius.min(100);
    let mut summary = ImportSummary {
        darken,
        blur,
        ..ImportSummary::default()
    };

    for preset in &manifest.presets {
        if !super::protocol::is_known_code(&preset.weather_code) {
            summary.skipped += 1;
            continue;
        }
        let Some(image) = read_entry(&mut archive, &preset.image_file)? else {
            summary.skipped += 1;
            continue;
        };
        // 先解码校验，不支持的格式直接跳过，不写坏原件
        if super::edit::decode(&image).is_err() {
            tracing::warn!("跳过 {}：{} 无法解码", preset.weather_code, preset.image_file);
            summary.skipped += 1;
            continue;
        }

        let ext = if preset.image_format.is_empty() {
            "png".to_string()
        } else {
            preset.image_format.clone()
        };
        super::store::save_source(&preset.weather_code, &ext, &image)
            .map_err(|e| format!("保存 {} 原件失败: {}", preset.weather_code, e))?;

        let processed = super::edit::process(&image, darken, blur)?;
        let size = super::edit::decode(&processed).ok();
        let (width, height) = size
            .as_ref()
            .map(|img| (img.width, img.height))
            .unwrap_or((0, 0));
        super::store::save_processed(
            &preset.weather_code,
            &preset.original_file_name,
            width,
            height,
            &processed,
        )
        .map_err(|e| format!("写入 {} 成品图失败: {}", preset.weather_code, e))?;

        summary.imported += 1;
    }

    if summary.imported == 0 {
        return Err("未找到可导入的图片".to_string());
    }
    Ok(summary)
}

/// 通过系统保存对话框把 `.swbg` 写到用户选的位置
pub async fn export_to_disk(darken: u32, blur: u32) -> Result<(), String> {
    use crate::astrobox::psys_host_v4::dialog;

    let bytes = export(darken, blur)?;
    let default_name = format!("simple-weather-bg.{FILE_EXTENSION}");

    let session = dialog::save_file_start(dialog::FilterConfig {
        multiple: false,
        extensions: vec![FILE_EXTENSION.to_string()],
        default_directory: String::new(),
        default_file_name: default_name,
    })
    .await
    .map_err(|e| format!("打开保存对话框失败: {}", e))?;

    // 分块写入，避免一次性把整包塞进单次调用
    const WRITE_CHUNK: usize = 64 * 1024;
    for chunk in bytes.chunks(WRITE_CHUNK) {
        if let Err(e) = dialog::save_file_write_chunk(session.session_id, chunk.to_vec()).await {
            dialog::save_file_abort(session.session_id).await;
            return Err(format!("写入失败: {}", e));
        }
    }

    dialog::save_file_finish(session.session_id)
        .await
        .map_err(|e| format!("保存完成失败: {}", e))?;
    tracing::info!("预设包已导出: {}", session.name);
    Ok(())
}

fn read_entry<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
) -> Result<Option<Vec<u8>>, String> {
    let mut file = match archive.by_name(name) {
        Ok(file) => file,
        Err(_) => return Ok(None),
    };
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)
        .map_err(|e| format!("读取 {} 失败: {}", name, e))?;
    Ok(Some(buf))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}