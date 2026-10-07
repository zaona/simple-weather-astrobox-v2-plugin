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
//! 包里只放处理参数（压暗 / 模糊）：`quality` 两端都只存不用（出图固定 RGB_565），
//! `advancedSyncMode` 属于本机设置——这两个字段都不再进预设包；早期版本导出的包里带过
//! 它们，serde 默认忽略未知字段，导入照样能读。

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
/// 导出对话框的默认文件名，与安卓端 `CreateDocument` 的初始名一致
const DEFAULT_EXPORT_NAME: &str = "weather_backgrounds.swbg";

/// 导入结果摘要，用于弹窗告知用户
#[derive(Debug, Clone)]
pub struct ImportSummary {
    pub imported: usize,
    pub skipped: usize,
    pub darken: u32,
    pub blur: u32,
}

impl Default for ImportSummary {
    fn default() -> Self {
        ImportSummary {
            imported: 0,
            skipped: 0,
            darken: 0,
            blur: 0,
        }
    }
}

// 字段名与安卓端 Gson 的 @SerializedName 逐字对齐（camelCase）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PresetSettings {
    #[serde(default)]
    darken_strength: u32,
    #[serde(default)]
    blur_radius: u32,
}

impl Default for PresetSettings {
    fn default() -> Self {
        PresetSettings {
            darken_strength: 0,
            blur_radius: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GlobalSettings {
    #[serde(default)]
    darken_strength: u32,
    #[serde(default)]
    blur_radius: u32,
}

impl Default for GlobalSettings {
    fn default() -> Self {
        GlobalSettings {
            darken_strength: 0,
            blur_radius: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PresetManifest {
    format_version: u32,
    #[serde(default)]
    app_version: String,
    #[serde(default)]
    export_timestamp: u64,
    #[serde(default)]
    metadata: BTreeMap<String, String>,
    global_settings: GlobalSettings,
    presets: Vec<PresetEntry>,
}

/// 把当前已配置的背景图打包成 `.swbg` 字节。
/// 只写处理参数（压暗 / 模糊）：`quality`、`advancedSyncMode` 都不随包走。
pub fn export(darken: u32, blur: u32) -> Result<Vec<u8>, String> {
    let metas = super::store::metas();

    // 与安卓一致：按码表顺序导出已配置的编号，而不是按落盘顺序
    let configured: Vec<&str> = super::protocol::WEATHER_BG_CODES
        .iter()
        .map(|(code, _)| *code)
        .filter(|code| metas.iter().any(|meta| meta.code == *code))
        .collect();
    if configured.is_empty() {
        return Err("没有已配置的背景图可导出".to_string());
    }

    let global_settings = GlobalSettings {
        darken_strength: darken,
        blur_radius: blur,
    };

    let mut presets = Vec::new();
    let mut images: Vec<(String, Vec<u8>)> = Vec::new();
    for code in configured {
        let label = super::protocol::label_of(code);
        // 安卓端读不到原图会直接让整次导出失败，这里保持一致
        let Some(source) = super::store::load_source(code) else {
            return Err(format!("无法读取图片: {}", label));
        };
        let meta = metas
            .iter()
            .find(|meta| meta.code == code)
            .expect("configured 来自 metas");
        let ext = super::store::extension_of(&meta.ext);
        let entry_name = format!("{IMAGES_DIR}{}.{}", code, ext);
        let original_file_name = if meta.label.trim().is_empty() {
            format!("{}.{}", code, ext)
        } else {
            meta.label.clone()
        };
        images.push((entry_name.clone(), source));
        presets.push(PresetEntry {
            weather_code: code.to_string(),
            weather_label: label.to_string(),
            image_file: entry_name,
            image_format: ext,
            original_file_name,
            settings: PresetSettings {
                darken_strength: darken,
                blur_radius: blur,
            },
        });
    }

    let manifest = PresetManifest {
        format_version: FORMAT_VERSION,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        export_timestamp: now_ms(),
        metadata: BTreeMap::new(),
        global_settings,
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

/// 拉起系统文件选择器挑一个 `.swbg`，返回文件名与字节。用户取消时返回 `Ok(None)`。
///
/// 分成「挑包」和「导入」两步，是为了和安卓一样在真正写入前先让用户确认。
pub async fn pick_package() -> Result<Option<(String, Vec<u8>)>, String> {
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
    // 与安卓一致：按文件名再挡一次非 .swbg
    if !picked
        .name
        .to_ascii_lowercase()
        .ends_with(&format!(".{FILE_EXTENSION}"))
    {
        return Err("请选择 .swbg 格式的预设包文件".to_string());
    }
    Ok(Some((picked.name, picked.data)))
}

/// 只读解析包，返回包内的预设数量，对齐安卓的 `peekImportInfo`
pub fn peek_count(bytes: &[u8]) -> Result<usize, String> {
    load_manifest(bytes).map(|(_, manifest)| manifest.presets.len())
}

/// 打开并校验 manifest，返回归档与 manifest 供后续读取
fn load_manifest(bytes: &[u8]) -> Result<(ZipArchive<Cursor<&[u8]>>, PresetManifest), String> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("不是有效的预设包: {}", e))?;

    let manifest_json = read_entry(&mut archive, MANIFEST_ENTRY)?
        .ok_or_else(|| "预设包中未找到 manifest.json".to_string())?;
    let manifest = parse_manifest(&manifest_json)?;

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
    Ok((archive, manifest))
}

/// 解析 `.swbg` 字节并写入 `bg/`，按包内参数出成品图
pub fn import(bytes: &[u8]) -> Result<ImportSummary, String> {
    let (mut archive, manifest) = load_manifest(bytes)?;

    let global = &manifest.global_settings;
    let darken = global.darken_strength.min(100);
    let blur = global.blur_radius.min(100);
    let mut summary = ImportSummary {
        darken,
        blur,
        ..ImportSummary::default()
    };

    // 与安卓一致：以 ZIP 里 `images/` 下的实际条目为准，编号取自条目文件名，
    // 而不是按 manifest 里的 imageFile 去找条目
    let entry_names: Vec<String> = archive
        .file_names()
        .filter_map(|name| name.strip_prefix(IMAGES_DIR).map(str::to_string))
        .collect();
    // 安卓只导入 manifest 里列出的编号
    let preset_codes: Vec<&str> = manifest
        .presets
        .iter()
        .map(|preset| preset.weather_code.as_str())
        .collect();

    for entry_name in entry_names {
        let code = entry_name.split('.').next().unwrap_or("");
        if !preset_codes.contains(&code) {
            continue;
        }
        // 编号会拼进落盘文件名，仍然只认码表里的编号，挡住路径穿越
        if !super::protocol::is_known_code(code) {
            tracing::warn!("跳过 {}：未知天气编号", entry_name);
            summary.skipped += 1;
            continue;
        }

        let Some(image) = read_entry(&mut archive, &format!("{IMAGES_DIR}{entry_name}"))? else {
            summary.skipped += 1;
            continue;
        };
        // 先解码校验，不支持的格式直接跳过，不写坏原件
        if super::edit::decode(&image).is_err() {
            tracing::warn!("跳过 {}：无法解码", entry_name);
            summary.skipped += 1;
            continue;
        }

        let ext = super::store::extension_of(&entry_name);
        super::store::save_source(code, &ext, &image)
            .map_err(|e| format!("保存 {} 原件失败: {}", code, e))?;

        let processed = super::edit::process(&image, darken, blur)?;
        let size = super::edit::decode(&processed).ok();
        let (width, height) = size
            .as_ref()
            .map(|img| (img.width, img.height))
            .unwrap_or((0, 0));
        // 显示名优先用 manifest 里的 originalFileName，与安卓导出的包对得上
        let label = manifest
            .presets
            .iter()
            .find(|preset| preset.weather_code == code)
            .map(|preset| preset.original_file_name.as_str())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(&entry_name);
        super::store::save_processed(code, label, width, height, &processed)
            .map_err(|e| format!("写入 {} 成品图失败: {}", code, e))?;

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

    let session = dialog::save_file_start(dialog::FilterConfig {
        multiple: false,
        extensions: vec![FILE_EXTENSION.to_string()],
        default_directory: String::new(),
        default_file_name: DEFAULT_EXPORT_NAME.to_string(),
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

fn parse_manifest(bytes: &[u8]) -> Result<PresetManifest, String> {
    serde_json::from_slice(bytes).map_err(|e| format!("解析 manifest 失败: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 安卓端 `BackgroundPresetManager` 导出的是 camelCase 字段，两端字段名必须逐字对上。
    /// 这个样张保留了老版本才会写的 `quality` / `advancedSyncMode`，用来一起验证兼容性。
    #[test]
    fn parses_android_camel_case_manifest() {
        let json = r#"{
            "appVersion": "2.4.4",
            "exportTimestamp": 1791184200000,
            "formatVersion": 1,
            "globalSettings": {
                "advancedSyncMode": true,
                "blurRadius": 6,
                "darkenStrength": 28,
                "quality": 28
            },
            "metadata": {},
            "presets": [
                {
                    "imageFile": "images/21.png",
                    "imageFormat": "png",
                    "originalFileName": "21.png",
                    "settings": { "blurRadius": 6, "darkenStrength": 28, "quality": 28 },
                    "weatherCode": "21",
                    "weatherLabel": "晴-白天"
                }
            ]
        }"#;

        let manifest = parse_manifest(json.as_bytes()).expect("安卓端预设包应能解析");
        assert_eq!(manifest.format_version, FORMAT_VERSION);
        assert_eq!(manifest.global_settings.darken_strength, 28);
        assert_eq!(manifest.global_settings.blur_radius, 6);
        assert_eq!(manifest.presets.len(), 1);

        let preset = &manifest.presets[0];
        assert_eq!(preset.weather_code, "21");
        assert_eq!(preset.weather_label, "晴-白天");
        assert_eq!(preset.image_file, "images/21.png");
        assert_eq!(preset.image_format, "png");
        assert_eq!(preset.original_file_name, "21.png");
        assert_eq!(preset.settings.darken_strength, 28);
        assert_eq!(preset.settings.blur_radius, 6);
    }

    /// 只认安卓那套 camelCase，插件自己早期导出的 snake_case 包不再兼容
    #[test]
    fn rejects_snake_case_manifest() {
        let json = r#"{
            "formatVersion": 1,
            "globalSettings": { "darken_strength": 12, "blur_radius": 3 },
            "presets": [
                {
                    "weather_code": "62",
                    "image_file": "images/62.png"
                }
            ]
        }"#;

        assert!(parse_manifest(json.as_bytes()).is_err());
    }

    /// 导出的字段名必须与安卓 Gson 的 @SerializedName 一致，否则对方导不回去；
    /// 且 `quality` / `advancedSyncMode` 都不能出现在包里（一个是死参数，一个是本机设置）
    #[test]
    fn exports_camel_case_fields() {
        let manifest = PresetManifest {
            format_version: FORMAT_VERSION,
            app_version: "test".to_string(),
            export_timestamp: 0,
            metadata: BTreeMap::new(),
            global_settings: GlobalSettings::default(),
            presets: vec![PresetEntry {
                weather_code: "21".to_string(),
                weather_label: "晴-白天".to_string(),
                image_file: "images/21.png".to_string(),
                image_format: "png".to_string(),
                original_file_name: "21.png".to_string(),
                settings: PresetSettings::default(),
            }],
        };
        let json = serde_json::to_string(&manifest).unwrap();

        for key in [
            "\"formatVersion\"",
            "\"globalSettings\"",
            "\"darkenStrength\"",
            "\"blurRadius\"",
            "\"weatherCode\"",
            "\"weatherLabel\"",
            "\"imageFile\"",
            "\"imageFormat\"",
            "\"originalFileName\"",
        ] {
            assert!(json.contains(key), "导出缺少字段 {}", key);
        }
        for key in [
            "\"darken_strength\"",
            "\"blur_radius\"",
            "\"weather_code\"",
            "\"image_file\"",
            "\"original_file_name\"",
            "\"advanced_sync_mode\"",
            // 本机设置不进预设包，导出必须没有它
            "\"advancedSyncMode\"",
            // 出图固定 RGB_565，这个参数没有意义，也不进预设包
            "\"quality\"",
        ] {
            assert!(!json.contains(key), "导出不应包含字段 {}", key);
        }
    }

    /// 早期版本（安卓端和插件端都算）导出的包里带 `quality` 和 `advancedSyncMode`，
    /// 现在虽然不写了，但读老包必须照常成功、直接忽略这两个字段
    #[test]
    fn legacy_extra_settings_are_ignored() {
        let json = r#"{
            "formatVersion": 1,
            "globalSettings": {
                "darkenStrength": 12,
                "blurRadius": 3,
                "quality": 85,
                "advancedSyncMode": false
            },
            "presets": [
                {
                    "weatherCode": "21",
                    "imageFile": "images/21.png",
                    "settings": { "darkenStrength": 12, "blurRadius": 3, "quality": 85 }
                }
            ]
        }"#;

        let manifest =
            parse_manifest(json.as_bytes()).expect("带 quality / advancedSyncMode 的老包应能解析");
        assert_eq!(manifest.global_settings.darken_strength, 12);
        assert_eq!(manifest.global_settings.blur_radius, 3);
        assert_eq!(manifest.presets.len(), 1);
        assert_eq!(manifest.presets[0].settings.darken_strength, 12);
        assert_eq!(manifest.presets[0].settings.blur_radius, 3);
    }

}
