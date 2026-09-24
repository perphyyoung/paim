//! 纯图像处理助手：解码、缩略图裁剪。供 image_service / thumbnail_service 共用。
//! 外部 `image` crate 的调用统一收拢在此：crate 名与本地命令模块 `commands::image`
//! 同名，集中一处可避免全限定名歧义（sentrux 曾因此报出文件级假环）。

use std::path::Path;

/// 缩略图统一边长：200×200 方形（等价 pm sharp 的 fit: cover 输出）。
pub(crate) const THUMB_SIZE: u32 = 200;

/// 解码图像文件；错误上下文由调用方拼接。
pub(crate) fn open_image(path: &Path) -> Result<image::DynamicImage, image::ImageError> {
    image::open(path)
}

/// 生成 200×200 方形缩略图：短边贴满 + 居中裁剪（等价 pm sharp 的 fit: cover）。
pub(crate) fn make_center_thumb(
    img: &image::DynamicImage,
) -> Result<image::DynamicImage, image::ImageError> {
    Ok(img.resize_to_fill(
        THUMB_SIZE,
        THUMB_SIZE,
        image::imageops::FilterType::Triangle,
    ))
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
