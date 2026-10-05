//! 背景图编辑，算法逐行对齐安卓端 `ImageProcessingUtil` 与 `ImageSyncManager`：
//!
//! 1. 缩放到 ≤432×514（`calculateInSampleSize` + `scaleBitmapIfNeeded`）
//! 2. 模糊：`boxBlur` 3 次迭代，可分离滑动窗口，边界 `coerceIn` 夹取
//! 3. 压暗：alpha = `strength * 255 / 100` 的黑色覆盖
//! 4. 量化：RGB565 位截断（对齐安卓 `Bitmap.Config.RGB_565` 的 Canvas 转换）
//! 5. PNG 编码：`Compression::Best`，对齐安卓 `compress(quality = 100)`
//!
//! 与安卓端的差别只有一处：安卓在手机上处理完再发送，插件收到的是成品图，因此
//! 插件把收到的字节当作原件保存，编辑结果另存为渲染副本，滑块可反复调整而不破坏原件。

use std::io::Cursor;

use png::{Compression, Decoder, Encoder, Transformations};

/// 手环端背景图的最大尺寸，对齐安卓 `ImageSyncManager.MAX_IMAGE_WIDTH/HEIGHT`
pub const MAX_IMAGE_WIDTH: u32 = 432;
pub const MAX_IMAGE_HEIGHT: u32 = 514;

/// 模糊迭代次数，对齐安卓 `repeat(3) { boxBlur(...) }`
const BLUR_ITERATIONS: usize = 3;

/// 缩略图尺寸，用于插件 UI 预览（data URI 体积可控）
pub const THUMBNAIL_WIDTH: u32 = 160;
pub const THUMBNAIL_HEIGHT: u32 = 190;

/// PNG 文件头，用于识别输入格式
const PNG_MAGIC: [u8; 4] = [0x89, b'P', b'N', b'G'];

/// 8 位 RGBA 像素，行优先
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Rgba {
    pub fn new(width: u32, height: u32) -> Self {
        Rgba {
            width,
            height,
            pixels: vec![0u8; (width * height * 4) as usize],
        }
    }

    #[inline]
    fn index(&self, x: u32, y: u32) -> usize {
        ((y * self.width + x) * 4) as usize
    }

    #[inline]
    fn get(&self, x: u32, y: u32) -> [u8; 4] {
        let idx = self.index(x, y);
        [
            self.pixels[idx],
            self.pixels[idx + 1],
            self.pixels[idx + 2],
            self.pixels[idx + 3],
        ]
    }

    #[inline]
    fn set(&mut self, x: u32, y: u32, rgba: [u8; 4]) {
        let idx = self.index(x, y);
        self.pixels[idx..idx + 4].copy_from_slice(&rgba);
    }
}

/// 解码 PNG 或 JPEG 为 RGBA8
pub fn decode(bytes: &[u8]) -> Result<Rgba, String> {
    if bytes.starts_with(&PNG_MAGIC) {
        decode_png(bytes)
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        decode_jpeg(bytes)
    } else {
        Err("只支持 PNG 或 JPEG 图片".to_string())
    }
}

/// 解码 PNG
pub fn decode_png(bytes: &[u8]) -> Result<Rgba, String> {
    let mut decoder = Decoder::new(Cursor::new(bytes));
    // 统一到 8 位 RGB(A)：调色板展开、16 位截断、tRNS 变成 alpha 通道
    decoder.set_transformations(Transformations::EXPAND | Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("PNG 解码失败: {}", e))?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("PNG 读取失败: {}", e))?;

    let channels = info.color_type.samples();
    let width = info.width;
    let height = info.height;
    let mut out = Rgba::new(width, height);
    for i in 0..(width as usize * height as usize) {
        let src = &buf[i * channels..(i + 1) * channels];
        let rgba = match info.color_type {
            png::ColorType::Rgba => [src[0], src[1], src[2], src[3]],
            png::ColorType::Rgb => [src[0], src[1], src[2], 255],
            png::ColorType::GrayscaleAlpha => [src[0], src[0], src[0], src[1]],
            png::ColorType::Grayscale => [src[0], src[0], src[0], 255],
            png::ColorType::Indexed => [src[0], src[0], src[0], 255],
        };
        let x = (i as u32) % width;
        let y = (i as u32) / width;
        let dst = out.index(x, y);
        out.pixels[dst..dst + 4].copy_from_slice(&rgba);
    }
    Ok(out)
}

/// 解码 JPEG，灰度按灰度展开，CMYK 等不常见色彩空间直接拒绝
pub fn decode_jpeg(bytes: &[u8]) -> Result<Rgba, String> {
    let mut decoder = jpeg_decoder::Decoder::new(Cursor::new(bytes));
    let pixels = decoder
        .decode()
        .map_err(|e| format!("JPEG 解码失败: {}", e))?;
    let info = decoder
        .info()
        .ok_or_else(|| "无法读取 JPEG 信息".to_string())?;

    let mut out = Rgba::new(info.width as u32, info.height as u32);
    match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => {
            let len = pixels.len().min(out.pixels.len());
            out.pixels[..len].copy_from_slice(&pixels[..len]);
            for px in out.pixels.chunks_exact_mut(4) {
                px[3] = 255;
            }
        }
        jpeg_decoder::PixelFormat::L8 => {
            for (i, gray) in pixels.iter().enumerate().take(info.width as usize * info.height as usize)
            {
                let dst = out.index(
                    (i as u32) % info.width as u32,
                    (i as u32) / info.width as u32,
                );
                out.pixels[dst] = *gray;
                out.pixels[dst + 1] = *gray;
                out.pixels[dst + 2] = *gray;
                out.pixels[dst + 3] = 255;
            }
        }
        other => return Err(format!("暂不支持的 JPEG 色彩格式: {:?}", other)),
    }
    Ok(out)
}

/// 编码为 PNG，对齐安卓 `compress(PNG, 100)`
pub fn encode_png(image: &Rgba) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut encoder = Encoder::new(&mut buf, image.width, image.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(Compression::Best);
        let mut writer = encoder.write_header().expect("PNG 头写入失败");
        writer
            .write_image_data(&image.pixels)
            .expect("PNG 数据写入失败");
    }
    buf
}

/// 缩放到指定上限内，比例与安卓一致：先按 2 的幂采样，再等比缩放
pub fn fit(image: &Rgba, max_width: u32, max_height: u32) -> Rgba {
    if image.width <= max_width && image.height <= max_height {
        return Rgba {
            width: image.width,
            height: image.height,
            pixels: image.pixels.clone(),
        };
    }
    let ratio =
        (max_width as f32 / image.width as f32).min(max_height as f32 / image.height as f32);
    // 安卓用 toInt() 截断
    let new_width = ((image.width as f32 * ratio) as u32).max(1);
    let new_height = ((image.height as f32 * ratio) as u32).max(1);
    scale_bilinear(image, new_width, new_height)
}

/// 双线性缩放，对齐安卓 `Bitmap.createScaledBitmap(bitmap, w, h, true)`
pub fn scale_bilinear(image: &Rgba, new_width: u32, new_height: u32) -> Rgba {
    if new_width == image.width && new_height == image.height {
        return Rgba {
            width: image.width,
            height: image.height,
            pixels: image.pixels.clone(),
        };
    }
    let mut out = Rgba::new(new_width, new_height);
    let scale_x = image.width as f32 / new_width as f32;
    let scale_y = image.height as f32 / new_height as f32;
    for y in 0..new_height {
        let fy = ((y as f32 + 0.5) * scale_y - 0.5).max(0.0);
        let y0 = (fy.floor() as u32).min(image.height - 1);
        let y1 = (y0 + 1).min(image.height - 1);
        let wy = fy - y0 as f32;
        for x in 0..new_width {
            let fx = ((x as f32 + 0.5) * scale_x - 0.5).max(0.0);
            let x0 = (fx.floor() as u32).min(image.width - 1);
            let x1 = (x0 + 1).min(image.width - 1);
            let wx = fx - x0 as f32;
            let mut rgba = [0u8; 4];
            for c in 0..4 {
                let p00 = image.get(x0, y0)[c] as f32;
                let p01 = image.get(x1, y0)[c] as f32;
                let p10 = image.get(x0, y1)[c] as f32;
                let p11 = image.get(x1, y1)[c] as f32;
                let top = p00 + (p01 - p00) * wx;
                let bottom = p10 + (p11 - p10) * wx;
                rgba[c] = (top + (bottom - top) * wy).round().clamp(0.0, 255.0) as u8;
            }
            out.set(x, y, rgba);
        }
    }
    out
}

/// 压暗：alpha = `strength * 255 / 100` 的黑色覆盖，对齐安卓 `applyDarken`
pub fn apply_darken(image: &mut Rgba, strength: u32) {
    if strength == 0 {
        return;
    }
    let alpha = ((strength * 255 / 100).min(255)) as u32;
    for px in image.pixels.chunks_exact_mut(4) {
        for c in 0..3 {
            let src = px[c] as u32;
            // Canvas 合成：dst = src * (1 - a) + 0 * a，四舍五入
            px[c] = ((src * (255 - alpha) + 127) / 255).min(255) as u8;
        }
    }
}

/// 模糊：`boxBlur` 迭代 3 次，对齐安卓 `applyBlur`
pub fn apply_blur(image: &mut Rgba, radius: u32) {
    if radius == 0 {
        return;
    }
    for _ in 0..BLUR_ITERATIONS {
        box_blur(image, radius as i64);
    }
}

/// 单次盒式模糊，可分离滑动窗口，边界夹取，对齐安卓 `boxBlur`
fn box_blur(image: &mut Rgba, radius: i64) {
    let width = image.width as i64;
    let height = image.height as i64;
    if radius <= 0 || width == 0 || height == 0 {
        return;
    }
    let r = radius;
    let div = (2 * r + 1) as u32;
    let clamp_x = |v: i64| v.clamp(0, width - 1) as u32;
    let clamp_y = |v: i64| v.clamp(0, height - 1) as u32;

    // 水平方向
    for y in 0..height {
        let mut sum = [0i64; 3];
        for k in -r..=r {
            let px = image.get(clamp_x(k), y as u32);
            for c in 0..3 {
                sum[c] += px[c] as i64;
            }
        }
        let row = y as u32 * image.width;
        let mut row_buf = vec![0u8; image.width as usize * 4];
        for x in 0..width {
            for c in 0..3 {
                let avg = (sum[c] / div as i64) as u8;
                row_buf[(x as usize) * 4 + c] = avg;
            }
            row_buf[(x as usize) * 4 + 3] = 255;
            let left = image.get(clamp_x(x - r), y as u32);
            let right = image.get(clamp_x(x + r + 1), y as u32);
            for c in 0..3 {
                sum[c] += right[c] as i64 - left[c] as i64;
            }
        }
        for x in 0..width as usize {
            let dst = ((row as usize + x) * 4) as usize;
            image.pixels[dst..dst + 4].copy_from_slice(&row_buf[x * 4..x * 4 + 4]);
        }
    }

    // 垂直方向
    for x in 0..width {
        let mut sum = [0i64; 3];
        for k in -r..=r {
            let px = image.get(x as u32, clamp_y(k));
            for c in 0..3 {
                sum[c] += px[c] as i64;
            }
        }
        let mut col_buf = vec![0u8; image.height as usize * 4];
        for y in 0..height {
            for c in 0..3 {
                col_buf[(y as usize) * 4 + c] = (sum[c] / div as i64) as u8;
            }
            col_buf[(y as usize) * 4 + 3] = 255;
            let top = image.get(x as u32, clamp_y(y - r));
            let bottom = image.get(x as u32, clamp_y(y + r + 1));
            for c in 0..3 {
                sum[c] += bottom[c] as i64 - top[c] as i64;
            }
        }
        for y in 0..height as usize {
            let dst = ((y as usize * image.width as usize + x as usize) * 4) as usize;
            image.pixels[dst..dst + 4].copy_from_slice(&col_buf[y * 4..y * 4 + 4]);
        }
    }
}

/// RGB565 位截断量化，对齐安卓把图画进 `Bitmap.Config.RGB_565`
pub fn quantize_rgb565(image: &mut Rgba) {
    for px in image.pixels.chunks_exact_mut(4) {
        px[0] &= 0xF8;
        px[1] &= 0xFC;
        px[2] &= 0xF8;
    }
}

/// 完整编辑流程：缩放 → 模糊 → 压暗 → RGB565 → PNG
pub fn process(bytes: &[u8], darken: u32, blur: u32) -> Result<Vec<u8>, String> {
    let mut image = decode(bytes)?;
    image = fit(&image, MAX_IMAGE_WIDTH, MAX_IMAGE_HEIGHT);
    apply_blur(&mut image, blur);
    apply_darken(&mut image, darken);
    quantize_rgb565(&mut image);
    Ok(encode_png(&image))
}

/// 生成缩略图，供插件 UI 的 data URI 预览使用
pub fn thumbnail(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let image = decode(bytes)?;
    let thumb = fit(&image, THUMBNAIL_WIDTH, THUMBNAIL_HEIGHT);
    Ok(encode_png(&thumb))
}

/// PNG 字节转 data URI，宿主把 IMAGE 元素渲染成 `<img src>`，只有 data URI 可用
pub fn data_uri(png_bytes: &[u8]) -> String {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD as BASE64;
    format!("data:image/png;base64,{}", BASE64.encode(png_bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(width: u32, height: u32) -> Rgba {
        let mut image = Rgba::new(width, height);
        for y in 0..height {
            for x in 0..width {
                image.set(
                    x,
                    y,
                    [
                        (x * 255 / width.max(1)) as u8,
                        (y * 255 / height.max(1)) as u8,
                        ((x + y) % 256) as u8,
                        255,
                    ],
                );
            }
        }
        image
    }

    #[test]
    fn darken_matches_alpha_overlay() {
        let mut image = Rgba::new(1, 1);
        image.set(0, 0, [255, 255, 255, 255]);
        // alpha = 50 * 255 / 100 = 127 -> (255 * 128 + 127) / 255 = 128
        apply_darken(&mut image, 50);
        assert_eq!(image.get(0, 0), [128, 128, 128, 255]);
        // 强度 100 时 alpha = 255，应压成纯黑
        apply_darken(&mut image, 100);
        assert_eq!(image.get(0, 0), [0, 0, 0, 255]);
    }

    #[test]
    fn blur_flattening_moves_pixels_toward_mean() {
        let mut image = Rgba::new(9, 1);
        image.set(0, 0, [255, 255, 255, 255]);
        image.set(8, 0, [255, 255, 255, 255]);
        for x in 1..8 {
            image.set(x, 0, [0, 0, 0, 255]);
        }
        apply_blur(&mut image, 2);
        assert!(image.get(0, 0)[0] > 0, "边缘应被邻居拉亮");
        assert!(image.get(4, 0)[0] > 0, "中心应被拉亮");
    }

    #[test]
    fn rgb565_truncates_low_bits() {
        let mut image = Rgba::new(1, 1);
        image.set(0, 0, [0xFF, 0xFF, 0xFF, 0xFF]);
        quantize_rgb565(&mut image);
        assert_eq!(image.get(0, 0), [0xF8, 0xFC, 0xF8, 0xFF]);
    }

    #[test]
    fn fit_keeps_small_images_and_caps_large_ones() {
        let small = gradient(100, 100);
        assert_eq!(fit(&small, MAX_IMAGE_WIDTH, MAX_IMAGE_HEIGHT).width, 100);

        let large = gradient(800, 1200);
        let fitted = fit(&large, MAX_IMAGE_WIDTH, MAX_IMAGE_HEIGHT);
        // ratio = min(432/800, 514/1200) = 0.4283 -> 342 x 514，安卓用 toInt() 截断
        assert_eq!(fitted.width, 342);
        assert_eq!(fitted.height, MAX_IMAGE_HEIGHT);
    }

    #[test]
    fn png_round_trip_preserves_pixels() {
        let image = gradient(32, 24);
        let encoded = encode_png(&image);
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded.width, 32);
        assert_eq!(decoded.height, 24);
        assert_eq!(decoded.pixels, image.pixels);
    }
}
