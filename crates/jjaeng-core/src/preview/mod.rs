mod actions;
mod geometry;
mod placement;
mod shell;

/// Lightly transparent opacity applied to every preview surface by default.
pub const DEFAULT_PREVIEW_TRANSPARENCY: f32 = 0.88;

pub use actions::{PreviewAction, PreviewActionError, PreviewEvent};
pub use geometry::PreviewWindowGeometry;
pub use placement::{
    compute_preview_placement, PreviewBounds, PreviewPlacement, PreviewSizingTokens,
    PreviewSourceArea,
};
pub use shell::PreviewWindowShell;
