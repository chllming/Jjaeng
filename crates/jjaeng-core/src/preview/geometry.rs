pub(super) const DEFAULT_PREVIEW_X: i32 = 24;
pub(super) const DEFAULT_PREVIEW_Y: i32 = 24;
pub(super) const DEFAULT_PREVIEW_WIDTH: i32 = 210;
pub(super) const DEFAULT_PREVIEW_HEIGHT: i32 = 118;
pub(super) const MIN_PREVIEW_WIDTH: i32 = 210;
pub(super) const MIN_PREVIEW_HEIGHT: i32 = 118;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreviewWindowGeometry {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}
