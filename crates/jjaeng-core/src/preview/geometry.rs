pub(super) const DEFAULT_PREVIEW_X: i32 = 24;
pub(super) const DEFAULT_PREVIEW_Y: i32 = 24;
// The preview policy is intentionally 20% larger than the original compact
// 210x118 surface while keeping the same 16:9 baseline.
pub(super) const DEFAULT_PREVIEW_WIDTH: i32 = 252;
pub(super) const DEFAULT_PREVIEW_HEIGHT: i32 = 142;
pub(super) const MIN_PREVIEW_WIDTH: i32 = 252;
pub(super) const MIN_PREVIEW_HEIGHT: i32 = 142;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreviewWindowGeometry {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}
