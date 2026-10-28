//! 纯图像处理助手：解码、缩略图裁剪。供 image_service / thumbnail_service 共用。
//! 外部 `image` crate 的调用统一收拢在此：crate 名与本地命令模块 `commands::image`
//! 同名，集中一处可避免全限定名歧义（sentrux 曾因此报出文件级假环）。
//!
//! WebP 走 libwebp 的**缩放解码**（`use_cropping` / `use_scaling` / `use_threads`）：
//! 缩略图与相似度预处理只要小图，没必要先解出整幅 RGBA 再缩小；其余格式、以及 libwebp
//! 失败（含损坏文件）时，回退到 `image` crate 的整图解码，行为与旧实现一致。

use std::path::Path;

use libwebp_sys as ffi;

/// 缩略图统一边长：200×200 方形（等价 pm sharp 的 fit: cover 输出）。
pub(crate) const THUMB_SIZE: u32 = 200;

/// 解码图像文件；错误上下文由调用方拼接。
pub(crate) fn open_image(path: &Path) -> Result<image::DynamicImage, image::ImageError> {
    image::open(path)
}

/// 只读文件头取尺寸（不解码像素）：导入记录宽高、缩放前算目标尺寸都用它。
pub(crate) fn image_dimensions(path: &Path) -> Result<(u32, u32), image::ImageError> {
    if let Some(bytes) = read_webp(path) {
        if let Some(dims) = webp_dimensions(&bytes) {
            return Ok(dims);
        }
    }
    let reader = image::ImageReader::open(path)?.with_guessed_format()?;
    reader.into_dimensions()
}

/// 解码成「短边贴满 + 居中裁剪」的方形图（缩略图 / 上传预览共用）。
pub(crate) fn cover_thumb(
    path: &Path,
    size: u32,
) -> Result<image::DynamicImage, image::ImageError> {
    // WebP：在解码器里先居中裁剪再缩放，只产出 size×size 的像素
    if let Some(bytes) = read_webp(path) {
        if let Some((w, h)) = webp_dimensions(&bytes) {
            // 小图不走进这个分支：libwebp 的 use_scaling 面向缩小，放大交回 image crate（行为与旧实现一致）
            if w >= size && h >= size {
                let side = w.min(h);
                let left = (w - side) / 2;
                let top = (h - side) / 2;
                if let Some(img) =
                    decode_webp_scaled(&bytes, Some((left, top, side, side)), Some((size, size)))
                {
                    return Ok(img);
                }
            }
        }
    }
    let img = open_image(path)?;
    Ok(img.resize_to_fill(size, size, image::imageops::FilterType::Triangle))
}

/// 解码成「长边 = target」的图（相似度检索预处理）；仅缩小、不放大。
pub(crate) fn long_side_source(
    path: &Path,
    target: u32,
) -> Result<image::DynamicImage, image::ImageError> {
    if let Some(bytes) = read_webp(path) {
        if let Some((w, h)) = webp_dimensions(&bytes) {
            if w.max(h) > target {
                let scaled = if w >= h {
                    (target, scale_side(h, target, w))
                } else {
                    (scale_side(w, target, h), target)
                };
                if let Some(img) = decode_webp_scaled(&bytes, None, Some(scaled)) {
                    return Ok(img);
                }
            }
        }
    }
    let img = open_image(path)?;
    Ok(resize_long_side(&img, target))
}

/// 缩放到「长边 = target」：仅缩小不放大，保持宽高比。
/// 相似度检索的预处理统一走这里（入库与检索必须同一策略，否则同图自比也会掉到 0.97 附近）。
pub(crate) fn resize_long_side(img: &image::DynamicImage, target: u32) -> image::DynamicImage {
    if img.width() <= target && img.height() <= target {
        return img.clone();
    }
    img.resize(target, target, image::imageops::FilterType::Lanczos3)
}

/// 编码为 JPEG 字节（相似度检索提交给 embedding 服务用；JPEG 不支持 alpha，统一转 RGB8）。
pub(crate) fn encode_jpeg(
    img: &image::DynamicImage,
    quality: u8,
) -> Result<Vec<u8>, jpeg_encoder::EncodingError> {
    let rgb = img.to_rgb8();
    let mut out = Vec::new();
    jpeg_encoder::Encoder::new(&mut out, quality).encode(
        &rgb,
        rgb.width() as u16,
        rgb.height() as u16,
        jpeg_encoder::ColorType::Rgb,
    )?;
    Ok(out)
}

/// 生成 size×size 的纯色小图 JPEG：供「探测服务实际返回的向量长度」用 —— 只有长度有意义，内容无关。
pub(crate) fn probe_jpeg(size: u32, quality: u8) -> Result<Vec<u8>, jpeg_encoder::EncodingError> {
    let img = image::DynamicImage::ImageRgb8(image::RgbImage::new(size, size));
    encode_jpeg(&img, quality)
}

/// 按魔数判断并读出 WebP 全文（不信扩展名：导入允许伪装扩展名的合法图）。
fn read_webp(path: &Path) -> Option<Vec<u8>> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(bytes)
    } else {
        None
    }
}

/// WebP 头部尺寸：`WebPGetFeatures` 只解析容器/码流头，不解码像素。
fn webp_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    // SAFETY: bytes 指向本次调用内有效的内存；features 是可写的 POD；ABI 版本取自同一 crate。
    unsafe {
        let mut features: ffi::WebPBitstreamFeatures = std::mem::zeroed();
        let status = ffi::WebPGetFeaturesInternal(
            bytes.as_ptr(),
            bytes.len(),
            &mut features,
            ffi::WEBP_DECODER_ABI_VERSION as i32,
        );
        if status != ffi::VP8StatusCode::VP8_STATUS_OK {
            return None;
        }
        Some((features.width as u32, features.height as u32))
    }
}

/// libwebp 缩放解码（可选裁剪）。任何失败都返回 `None`，由调用方回退到 `image` crate。
fn decode_webp_scaled(
    bytes: &[u8],
    crop: Option<(u32, u32, u32, u32)>,
    scaled: Option<(u32, u32)>,
) -> Option<image::DynamicImage> {
    // SAFETY: 输入指针在本次调用内有效；输出缓冲由 libwebp 分配，复制后立刻 `WebPFreeDecBuffer` 释放。
    unsafe {
        let mut config: ffi::WebPDecoderConfig = std::mem::zeroed();
        if ffi::WebPInitDecoderConfigInternal(&mut config, ffi::WEBP_DECODER_ABI_VERSION as i32)
            == 0
        {
            return None;
        }
        config.output.colorspace = ffi::WEBP_CSP_MODE::MODE_RGBA;
        if let Some((left, top, w, h)) = crop {
            config.options.use_cropping = 1;
            config.options.crop_left = left as i32;
            config.options.crop_top = top as i32;
            config.options.crop_width = w as i32;
            config.options.crop_height = h as i32;
        }
        if let Some((w, h)) = scaled {
            config.options.use_scaling = 1;
            config.options.scaled_width = w as i32;
            config.options.scaled_height = h as i32;
        }
        config.options.use_threads = 1;

        let status = ffi::WebPDecode(bytes.as_ptr(), bytes.len(), &mut config);
        if status != ffi::VP8StatusCode::VP8_STATUS_OK {
            return None;
        }

        let width = config.output.width as usize;
        let height = config.output.height as usize;
        let stride = config.output.u.RGBA.stride as usize;
        let rgba = config.output.u.RGBA.rgba;
        if width == 0 || height == 0 || rgba.is_null() {
            ffi::WebPFreeDecBuffer(&mut config.output);
            return None;
        }

        let row_bytes = width * 4;
        let mut data = Vec::with_capacity(row_bytes * height);
        for y in 0..height {
            let row = std::slice::from_raw_parts(rgba.add(y * stride), row_bytes);
            data.extend_from_slice(row);
        }
        ffi::WebPFreeDecBuffer(&mut config.output);
        image::RgbaImage::from_raw(width as u32, height as u32, data)
            .map(image::DynamicImage::ImageRgba8)
    }
}

/// 按长边等比缩放时的另一条边（四舍五入，至少 1px）。
fn scale_side(value: u32, target: u32, base: u32) -> u32 {
    let num = u64::from(value) * u64::from(target) + u64::from(base) / 2;
    ((num / u64::from(base)).max(1)) as u32
}

#[cfg(test)]
#[path = "image_ops.test.rs"]
mod tests;
