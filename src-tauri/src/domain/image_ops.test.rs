//! image_ops 单测：按用途解码（缩略图 cover / 相似度长边 / 头部尺寸）的行为契约。
//! 这些断言与解码实现无关（image crate 或 libwebp 缩放解码都必须满足），因此重构前后都应绿。

use super::{
    cover_thumb, image_dimensions, long_side_source, open_image, resize_long_side, THUMB_SIZE,
};
use image::GenericImageView; // DynamicImage::dimensions()
use std::path::{Path, PathBuf};

fn temp_dir(tag: &str) -> PathBuf {
    crate::infra::db::test_temp_dir(&format!("image-ops-{tag}"))
}

/// 写一张渐变 WebP（可选无损）；RGBA 输入，带 alpha 通道。
fn write_webp(path: &Path, w: u32, h: u32, lossless: bool) {
    let rgba = image::RgbaImage::from_fn(w, h, |x, y| {
        image::Rgba([
            (x * 255 / w.max(1)) as u8,
            (y * 255 / h.max(1)) as u8,
            ((x + y) * 255 / (w + h).max(1)) as u8,
            200, // 非 255：强制 libwebp 产出 ALPH（覆盖带 alpha 的解码路径）
        ])
    });
    let encoder = webp::Encoder::from_rgba(&rgba, w, h);
    let mut config = webp::WebPConfig::new().unwrap();
    config.quality = 90.0;
    config.lossless = i32::from(lossless);
    config.method = 4;
    std::fs::write(path, &*encoder.encode_advanced(&config).unwrap()).unwrap();
}

/// 每通道平均值：不同重采样器会有点差，用均值/中心像素做与实现无关的近似比较。
fn mean_rgba(img: &image::DynamicImage) -> [f64; 4] {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let mut sum = [0f64; 4];
    for p in rgba.pixels() {
        for c in 0..4 {
            sum[c] += f64::from(p.0[c]);
        }
    }
    let n = f64::from(w * h);
    [sum[0] / n, sum[1] / n, sum[2] / n, sum[3] / n]
}

fn center_rgba(img: &image::DynamicImage) -> [u8; 4] {
    let rgba = img.to_rgba8();
    rgba.get_pixel(rgba.width() / 2, rgba.height() / 2).0
}

#[test]
fn cover_thumb_is_square_and_close_to_reference() {
    let dir = temp_dir("cover");
    let path = dir.join("a.webp");
    write_webp(&path, 300, 200, false);

    let thumb = cover_thumb(&path, THUMB_SIZE).unwrap();
    assert_eq!(
        thumb.dimensions(),
        (THUMB_SIZE, THUMB_SIZE),
        "缩略图固定 200×200"
    );
    assert!(
        thumb.color().has_alpha(),
        "缩略图保留 alpha 通道（交给 JPEG 编码前）"
    );
    let decoded = image::load_from_memory(&{
        let mut out = Vec::new();
        thumb
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    })
    .unwrap();
    assert_eq!(decoded.dimensions(), (THUMB_SIZE, THUMB_SIZE));

    // 与「全量解码 + resize_to_fill」的参考实现比均值/中心像素：只允许重采样带来的微小差异
    let reference = open_image(&path).unwrap();
    let reference = reference.resize_to_fill(
        THUMB_SIZE,
        THUMB_SIZE,
        image::imageops::FilterType::Triangle,
    );
    let (a, b) = (mean_rgba(&thumb), mean_rgba(&reference));
    for c in 0..3 {
        assert!(
            (a[c] - b[c]).abs() < 6.0,
            "通道 {c} 均值偏差过大：{:?} vs {:?}",
            a,
            b
        );
    }
    let (ca, cb) = (center_rgba(&thumb), center_rgba(&reference));
    for c in 0..3 {
        assert!(
            (i32::from(ca[c]) - i32::from(cb[c])).abs() <= 12,
            "中心像素偏差过大：{:?} vs {:?}",
            ca,
            cb
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cover_thumb_handles_lossless_webp() {
    let dir = temp_dir("cover-lossless");
    let path = dir.join("lossless.webp");
    write_webp(&path, 420, 260, true);
    let thumb = cover_thumb(&path, THUMB_SIZE).unwrap();
    assert_eq!(thumb.dimensions(), (THUMB_SIZE, THUMB_SIZE));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cover_thumb_upscales_small_images_like_before() {
    // 现状：resize_to_fill 会把小图放大到 200×200；重构后必须保持
    let dir = temp_dir("cover-small");
    let path = dir.join("small.webp");
    write_webp(&path, 40, 30, false);
    let thumb = cover_thumb(&path, THUMB_SIZE).unwrap();
    assert_eq!(thumb.dimensions(), (THUMB_SIZE, THUMB_SIZE));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn long_side_source_scales_webp_keeping_aspect() {
    let dir = temp_dir("long");
    let path = dir.join("wide.webp");
    write_webp(&path, 1200, 800, false);
    let img = long_side_source(&path, 1024).unwrap();
    assert_eq!(img.width(), 1024, "长边缩到目标值");
    assert!(
        (i32::from(img.height() as u16) - 683).abs() <= 1,
        "高度按比例缩放，实际 {}",
        img.height()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn long_side_source_never_upscales() {
    let dir = temp_dir("long-small");
    let path = dir.join("small.webp");
    write_webp(&path, 300, 200, false);
    let img = long_side_source(&path, 1024).unwrap();
    assert_eq!(img.dimensions(), (300, 200), "小图不放大");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn png_and_webp_go_through_the_same_contract() {
    let dir = temp_dir("png");
    let path = dir.join("a.png");
    let img = image::RgbaImage::from_fn(300, 200, |x, y| {
        image::Rgba([(x % 255) as u8, (y % 255) as u8, 128, 255])
    });
    img.save(&path).unwrap();

    assert_eq!(image_dimensions(&path).unwrap(), (300, 200));
    assert_eq!(
        cover_thumb(&path, THUMB_SIZE).unwrap().dimensions(),
        (200, 200)
    );
    assert_eq!(
        long_side_source(&path, 1024).unwrap().dimensions(),
        (300, 200)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn image_dimensions_reads_header_without_full_decode() {
    let dir = temp_dir("dims");
    let path = dir.join("dims.webp");
    write_webp(&path, 321, 123, false);
    assert_eq!(image_dimensions(&path).unwrap(), (321, 123));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn corrupt_webp_reports_error_instead_of_panicking() {
    let dir = temp_dir("corrupt");
    let path = dir.join("bad.webp");
    // 合法 RIFF/WEBP 头 + 全 0 的 VP8 载荷：libwebp 与 image 都必须报错
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&32u32.to_le_bytes());
    bytes.extend_from_slice(b"WEBP");
    bytes.extend_from_slice(b"VP8 ");
    bytes.extend_from_slice(&20u32.to_le_bytes());
    bytes.extend_from_slice(&[0u8; 20]);
    std::fs::write(&path, &bytes).unwrap();

    assert!(
        cover_thumb(&path, THUMB_SIZE).is_err(),
        "坏文件必须返回 Err"
    );
    assert!(long_side_source(&path, 1024).is_err(), "坏文件必须返回 Err");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn resize_long_side_keeps_existing_semantics() {
    let img = image::DynamicImage::ImageRgba8(image::RgbaImage::new(1200, 800));
    assert_eq!(resize_long_side(&img, 1024).dimensions(), (1024, 683));
    let small = image::DynamicImage::ImageRgba8(image::RgbaImage::new(100, 100));
    assert_eq!(resize_long_side(&small, 1024).dimensions(), (100, 100));
}
