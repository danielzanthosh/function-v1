use crate::{ChatMessage, ProviderError};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use image::codecs::jpeg::JpegEncoder;
use image::DynamicImage;
use std::io::Cursor;

/// The maximum base64 image payload size allowed by the provider (5 MB).
pub const MAX_BASE64_IMAGE_BYTES: usize = 5 * 1024 * 1024;

/// Process all images across chat messages to ensure every base64 image payload
/// is strictly below the 5 MB limit.
pub fn process_request_messages(messages: &mut [ChatMessage]) -> Result<(), ProviderError> {
    for msg in messages {
        if let Some(ref mut imgs) = msg.images {
            for img_str in imgs {
                *img_str = process_image_string(img_str)?;
            }
        }
    }
    Ok(())
}

/// Process an image string (raw base64, data URL, or file path).
/// If the image payload as base64 is already under 5 MB, it is returned unchanged.
/// If oversized, it is automatically decoded, resized, and compressed to JPEG.
pub fn process_image_string(img_str: &str) -> Result<String, ProviderError> {
    let trimmed = img_str.trim();

    // 1. Check if input is a local file path
    let is_data_url = trimmed.starts_with("data:");
    let path = std::path::Path::new(trimmed);
    if !is_data_url && path.is_file() {
        let file_bytes = std::fs::read(path).map_err(|e| ProviderError::Api {
            code: 400,
            message: format!("Failed to read image file '{}': {}", trimmed, e),
        })?;

        let b64 = BASE64.encode(&file_bytes);
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("png")
            .to_lowercase();
        let mime = match ext.as_str() {
            "jpg" | "jpeg" => "image/jpeg",
            "webp" => "image/webp",
            "gif" => "image/gif",
            "bmp" => "image/bmp",
            _ => "image/png",
        };
        let formatted = format!("data:{};base64,{}", mime, b64);
        if formatted.len() <= MAX_BASE64_IMAGE_BYTES {
            return Ok(formatted);
        }

        // Oversized file image: load and compress
        let dyn_img = image::load_from_memory(&file_bytes).map_err(|e| ProviderError::Api {
            code: 400,
            message: format!("Failed to decode image file '{}': {}", trimmed, e),
        })?;
        return compress_dynamic_image(&dyn_img);
    }

    // 2. Check current base64 payload size
    let formatted_len = if is_data_url {
        trimmed.len()
    } else {
        // "data:image/png;base64," + raw_b64
        "data:image/png;base64,".len() + trimmed.len()
    };

    if formatted_len <= MAX_BASE64_IMAGE_BYTES {
        // Already within 5 MB limit
        return Ok(trimmed.to_string());
    }

    // 3. Oversized image -> Extract base64 payload
    let raw_b64 = if is_data_url {
        if let Some((_, payload)) = trimmed.split_once(',') {
            payload
        } else {
            trimmed
        }
    } else {
        trimmed
    };

    let decoded_bytes = BASE64.decode(raw_b64.as_bytes()).map_err(|e| ProviderError::Api {
        code: 400,
        message: format!("Failed to decode base64 image data: {}", e),
    })?;

    let dyn_img = image::load_from_memory(&decoded_bytes).map_err(|e| ProviderError::Api {
        code: 400,
        message: format!("Failed to parse image for compression: {}", e),
    })?;

    compress_dynamic_image(&dyn_img)
}

/// Compress and downscale a DynamicImage iteratively until its base64 payload is below 5 MB.
fn compress_dynamic_image(img: &DynamicImage) -> Result<String, ProviderError> {
    let mut current_img = img.clone();
    let (orig_w, orig_h) = (img.width(), img.height());

    // Try JPEG compression at decreasing qualities first
    let qualities = [85, 75, 60, 45, 30];
    for &q in &qualities {
        let mut buffer = Vec::new();
        let mut cursor = Cursor::new(&mut buffer);
        let rgb_img = current_img.to_rgb8();
        let mut encoder = JpegEncoder::new_with_quality(&mut cursor, q);
        if encoder.encode_image(&rgb_img).is_ok() {
            let b64 = BASE64.encode(&buffer);
            let formatted = format!("data:image/jpeg;base64,{}", b64);
            if formatted.len() <= MAX_BASE64_IMAGE_BYTES {
                tracing::info!(
                    orig_width = orig_w,
                    orig_height = orig_h,
                    final_bytes = buffer.len(),
                    base64_len = formatted.len(),
                    quality = q,
                    "Successfully compressed oversized image below 5 MB limit"
                );
                return Ok(formatted);
            }
        }
    }

    // If quality reduction alone is insufficient, iteratively downscale dimensions
    let scale_factors = [0.75f32, 0.5, 0.35, 0.25, 0.15, 0.10];
    for &factor in &scale_factors {
        let new_w = ((orig_w as f32) * factor).max(1.0) as u32;
        let new_h = ((orig_h as f32) * factor).max(1.0) as u32;
        current_img = img.resize(new_w, new_h, image::imageops::FilterType::Triangle);

        for &q in &[75, 50, 30] {
            let mut buffer = Vec::new();
            let mut cursor = Cursor::new(&mut buffer);
            let rgb_img = current_img.to_rgb8();
            let mut encoder = JpegEncoder::new_with_quality(&mut cursor, q);
            if encoder.encode_image(&rgb_img).is_ok() {
                let b64 = BASE64.encode(&buffer);
                let formatted = format!("data:image/jpeg;base64,{}", b64);
                if formatted.len() <= MAX_BASE64_IMAGE_BYTES {
                    tracing::info!(
                        orig_width = orig_w,
                        orig_height = orig_h,
                        new_width = new_w,
                        new_height = new_h,
                        final_bytes = buffer.len(),
                        base64_len = formatted.len(),
                        quality = q,
                        "Successfully resized and compressed oversized image below 5 MB limit"
                    );
                    return Ok(formatted);
                }
            }
        }
    }

    Err(ProviderError::Api {
        code: 400,
        message: "Failed to compress image below 5 MB limit".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normal_image_under_limit_is_unchanged() {
        let small_img = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
        let processed = process_image_string(small_img).unwrap();
        assert_eq!(processed, small_img);
    }

    #[test]
    fn test_oversized_image_compression() {
        // Create a large synthetic image (3000x3000 raw RGB)
        let mut img_buf = image::RgbImage::new(3000, 3000);
        for (x, y, pixel) in img_buf.enumerate_pixels_mut() {
            *pixel = image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8]);
        }
        let dyn_img = DynamicImage::ImageRgb8(img_buf);

        // Encode as uncompressed PNG bytes to create oversized payload
        let mut png_bytes = Vec::new();
        let mut cursor = Cursor::new(&mut png_bytes);
        dyn_img.write_to(&mut cursor, image::ImageFormat::Png).unwrap();

        let b64_payload = BASE64.encode(&png_bytes);
        let oversized_data_url = format!("data:image/png;base64,{}", b64_payload);

        assert!(
            oversized_data_url.len() > MAX_BASE64_IMAGE_BYTES,
            "Test setup requires oversized image > 5 MB, got {}",
            oversized_data_url.len()
        );

        let processed = process_image_string(&oversized_data_url).expect("Compression should succeed");
        assert!(
            processed.len() <= MAX_BASE64_IMAGE_BYTES,
            "Compressed image must be <= 5 MB (5242880 bytes), got {}",
            processed.len()
        );
        assert!(processed.starts_with("data:image/jpeg;base64,"));
    }

    #[test]
    fn test_invalid_image_base64_error_handling() {
        let invalid_b64 = "data:image/png;base64,".to_string() + &"A".repeat(6 * 1024 * 1024);
        let result = process_image_string(&invalid_b64);
        assert!(result.is_err());
        if let Err(ProviderError::Api { message, .. }) = result {
            assert!(message.contains("Failed to") || message.contains("decode") || message.contains("parse"));
        } else {
            panic!("Expected ProviderError::Api");
        }
    }

    #[test]
    fn test_process_request_messages_mutates_images() {
        let small_img = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==".to_string();
        let mut messages = vec![
            ChatMessage::user("Hello"),
            ChatMessage::user_with_images("Look at this", vec![small_img.clone()]),
        ];

        process_request_messages(&mut messages).expect("Should succeed");
        assert_eq!(messages[1].images.as_ref().unwrap()[0], small_img);
    }

    #[test]
    fn test_oversized_rgba_screenshot_compression() {
        // Create an RGBA image (as PNG screenshots produce)
        let mut img_buf = image::RgbaImage::new(3000, 3000);
        for (x, y, pixel) in img_buf.enumerate_pixels_mut() {
            *pixel = image::Rgba([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8, 255]);
        }
        let dyn_img = DynamicImage::ImageRgba8(img_buf);

        let mut png_bytes = Vec::new();
        let mut cursor = Cursor::new(&mut png_bytes);
        dyn_img.write_to(&mut cursor, image::ImageFormat::Png).unwrap();

        let b64_payload = BASE64.encode(&png_bytes);
        let oversized_data_url = format!("data:image/png;base64,{}", b64_payload);

        let processed = process_image_string(&oversized_data_url).expect("RGBA compression must succeed");
        assert!(processed.len() <= MAX_BASE64_IMAGE_BYTES);
        assert!(processed.starts_with("data:image/jpeg;base64,"));
    }
}
