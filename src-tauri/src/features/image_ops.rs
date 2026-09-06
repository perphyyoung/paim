//! 纯图像处理助手：解码、缩略图裁剪。供 image_service / thumbnail_service 共用。
//! 外部 `image` crate 的调用统一收拢在此：crate 名与本地命令模块 `features::image`
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
