//! Function brand logo and mark components.
//!
//! Renders the official Function monochrome geometric mark and lockup
//! based on the new visual identity:
//! - Background / deep dark: #14120A
//! - Primary light / text: #F1F0EF

use gpui::prelude::*;
use gpui::{img, px, Image, ImageFormat, IntoElement};
use std::sync::Arc;

const BRAND_MARK_BYTES: &[u8] = include_bytes!("../../../../assets/brand/brand_mark.png");
const BLACK_LOGO_BYTES: &[u8] =
    include_bytes!("../../../../assets/brand/Black logo in Tranparent.png");
const BRAND_LOCKUP_BYTES: &[u8] = include_bytes!("../../../../assets/brand/brand_lockup.png");
const BRAND_LOCKUP_SMALL_BYTES: &[u8] =
    include_bytes!("../../../../assets/brand/brand_lockup_small.png");
const APP_ICON_BYTES: &[u8] = include_bytes!("../../../../assets/icon.png");

/// Render the official Function semi-circular geometric mark with dithered particles.
/// Aspect ratio is 2:1 (width:height = 2:1).
pub fn render_brand_mark(width: f32) -> impl IntoElement {
    render_brand_mark_with_mode(width, false)
}

/// Render the mark with the correct contrast for the active theme.
pub fn render_brand_mark_with_mode(width: f32, light_theme: bool) -> impl IntoElement {
    let height = (width / 2.0).round();
    let image = Arc::new(Image::from_bytes(
        ImageFormat::Png,
        if light_theme {
            BLACK_LOGO_BYTES.to_vec()
        } else {
            BRAND_MARK_BYTES.to_vec()
        },
    ));
    img(image).w(px(width)).h(px(height))
}

/// Render the official Function logo lockup (mark + FUNCTION wordmark).
/// Aspect ratio is approximately 1.51:1 (width:height).
pub fn render_brand_lockup(width: f32) -> impl IntoElement {
    let height = (width / 1.51).round();
    let image = Arc::new(Image::from_bytes(
        ImageFormat::Png,
        BRAND_LOCKUP_BYTES.to_vec(),
    ));
    img(image).w(px(width)).h(px(height))
}

/// Render the compact brand lockup (mark + FUNCTION wordmark).
pub fn render_brand_lockup_small(width: f32) -> impl IntoElement {
    let height = (width / 1.51).round();
    let image = Arc::new(Image::from_bytes(
        ImageFormat::Png,
        BRAND_LOCKUP_SMALL_BYTES.to_vec(),
    ));
    img(image).w(px(width)).h(px(height))
}

/// Render the official Function geometric brand mark for general UI use.
pub fn render_logo(size: f32) -> impl IntoElement {
    render_brand_mark(size)
}

pub fn render_logo_with_mode(size: f32, light_theme: bool) -> impl IntoElement {
    render_brand_mark_with_mode(size, light_theme)
}

/// Render the square application icon.
pub fn render_app_icon(size: f32) -> impl IntoElement {
    let image = Arc::new(Image::from_bytes(ImageFormat::Png, APP_ICON_BYTES.to_vec()));
    img(image).w(px(size)).h(px(size)).rounded_md()
}
