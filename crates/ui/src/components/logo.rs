//! Function brand logo component.
//!
//! Renders the official Function monochrome geometric icon.

use gpui::prelude::*;
use gpui::{img, px, Image, ImageFormat, IntoElement};
use std::sync::Arc;

/// Embedded icon bytes from workspace assets
const ICON_BYTES: &[u8] = include_bytes!("../../../../assets/icon.png");

/// Render the official Function geometric brand mark.
pub fn render_logo(size: f32) -> impl IntoElement {
    let image = Arc::new(Image::from_bytes(ImageFormat::Png, ICON_BYTES.to_vec()));
    img(image)
        .w(px(size))
        .h(px(size))
        .rounded_sm()
}
