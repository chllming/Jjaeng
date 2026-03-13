use crate::ui::StyleTokens;
use gtk4::prelude::*;
use gtk4::ApplicationWindow;
use jjaeng_core::capture;
use jjaeng_core::preview;

use super::window_state::RuntimeWindowGeometry;

fn normalize_window_dimension(value: i32, fallback: i32, minimum: i32) -> i32 {
    let normalized = if value > 0 { value } else { fallback };
    normalized.max(minimum)
}

fn monitor_bounds() -> Vec<preview::PreviewBounds> {
    let Some(display) = gtk4::gdk::Display::default() else {
        return Vec::new();
    };
    let monitors = display.monitors();
    let mut bounds_list = Vec::new();

    for index in 0..monitors.n_items() {
        let Some(item) = monitors.item(index) else {
            continue;
        };
        let Ok(monitor) = item.downcast::<gtk4::gdk::Monitor>() else {
            continue;
        };
        let geometry = monitor.geometry();
        bounds_list.push(preview::PreviewBounds {
            x: geometry.x(),
            y: geometry.y(),
            width: geometry.width().max(1),
            height: geometry.height().max(1),
        });
    }

    bounds_list
}

pub(super) fn read_window_geometry(
    window: &ApplicationWindow,
    fallback: RuntimeWindowGeometry,
    minimum: RuntimeWindowGeometry,
) -> RuntimeWindowGeometry {
    let width = if window.width() > 0 {
        window.width()
    } else if window.default_width() > 0 {
        window.default_width()
    } else {
        fallback.width
    };
    let height = if window.height() > 0 {
        window.height()
    } else if window.default_height() > 0 {
        window.default_height()
    } else {
        fallback.height
    };

    RuntimeWindowGeometry::with_position(
        fallback.x,
        fallback.y,
        normalize_window_dimension(width, fallback.width, minimum.width),
        normalize_window_dimension(height, fallback.height, minimum.height),
    )
}

fn monitor_bounds_for_point(x: i32, y: i32) -> Option<preview::PreviewBounds> {
    let bounds_list = monitor_bounds();
    let fallback = bounds_list.first().copied();

    for bounds in bounds_list {
        let max_x = bounds.x.saturating_add(bounds.width);
        let max_y = bounds.y.saturating_add(bounds.height);
        if x >= bounds.x && x < max_x && y >= bounds.y && y < max_y {
            return Some(bounds);
        }
    }

    fallback
}

fn fallback_preview_bounds(
    source: preview::PreviewSourceArea,
    style_tokens: StyleTokens,
) -> preview::PreviewBounds {
    preview::PreviewBounds {
        x: source.x,
        y: source.y,
        width: source
            .width
            .max(style_tokens.preview_default_width)
            .max(style_tokens.preview_min_width)
            .max(1),
        height: source
            .height
            .max(style_tokens.preview_default_height)
            .max(style_tokens.preview_min_height)
            .max(1),
    }
}

fn capture_source_area(
    artifact: &capture::CaptureArtifact,
    fallback_width: i32,
    fallback_height: i32,
) -> preview::PreviewSourceArea {
    let source_width = i32::try_from(artifact.screen_width)
        .ok()
        .filter(|value| *value > 0)
        .or_else(|| {
            i32::try_from(artifact.width)
                .ok()
                .filter(|value| *value > 0)
        })
        .unwrap_or(fallback_width.max(1));
    let source_height = i32::try_from(artifact.screen_height)
        .ok()
        .filter(|value| *value > 0)
        .or_else(|| {
            i32::try_from(artifact.height)
                .ok()
                .filter(|value| *value > 0)
        })
        .unwrap_or(fallback_height.max(1));

    preview::PreviewSourceArea {
        x: artifact.screen_x,
        y: artifact.screen_y,
        width: source_width,
        height: source_height,
    }
}

pub(super) fn centered_window_geometry_for_capture(
    artifact: &capture::CaptureArtifact,
    window_geometry: RuntimeWindowGeometry,
) -> (i32, i32, i32, i32) {
    let source = capture_source_area(artifact, window_geometry.width, window_geometry.height);
    let center_x = source.x.saturating_add(source.width / 2);
    let center_y = source.y.saturating_add(source.height / 2);
    centered_window_geometry_for_point(center_x, center_y, window_geometry)
}

pub(super) fn centered_window_geometry_for_point(
    anchor_x: i32,
    anchor_y: i32,
    window_geometry: RuntimeWindowGeometry,
) -> (i32, i32, i32, i32) {
    let width = window_geometry.width.max(1);
    let height = window_geometry.height.max(1);
    let bounds = monitor_bounds_for_point(anchor_x, anchor_y).unwrap_or(preview::PreviewBounds {
        x: anchor_x.saturating_sub(width / 2),
        y: anchor_y.saturating_sub(height / 2),
        width,
        height,
    });

    let x = if bounds.width > width {
        bounds.x.saturating_add((bounds.width - width) / 2)
    } else {
        bounds.x
    };
    let y = if bounds.height > height {
        bounds.y.saturating_add((bounds.height - height) / 2)
    } else {
        bounds.y
    };

    (x, y, width, height)
}

pub(super) fn bottom_centered_window_geometry_for_point(
    anchor_x: i32,
    anchor_y: i32,
    window_geometry: RuntimeWindowGeometry,
    margin: i32,
) -> (i32, i32, i32, i32) {
    let width = window_geometry.width.max(1);
    let height = window_geometry.height.max(1);
    let bounds = monitor_bounds_for_point(anchor_x, anchor_y).unwrap_or(preview::PreviewBounds {
        x: anchor_x.saturating_sub(width / 2),
        y: anchor_y.saturating_sub(height / 2),
        width,
        height,
    });
    bottom_centered_window_geometry(bounds, window_geometry, margin)
}

fn bottom_centered_window_geometry(
    bounds: preview::PreviewBounds,
    window_geometry: RuntimeWindowGeometry,
    margin: i32,
) -> (i32, i32, i32, i32) {
    let width = window_geometry.width.max(1);
    let height = window_geometry.height.max(1);
    let margin = margin.max(0);

    let x = if bounds.width > width {
        bounds.x.saturating_add((bounds.width - width) / 2)
    } else {
        bounds.x
    };
    let y = bounds
        .y
        .saturating_add(bounds.height.saturating_sub(height).saturating_sub(margin))
        .max(bounds.y);

    (x, y, width, height)
}

pub(super) fn compute_initial_preview_placement(
    artifact: &capture::CaptureArtifact,
    style_tokens: StyleTokens,
) -> preview::PreviewPlacement {
    let source = capture_source_area(
        artifact,
        style_tokens.preview_default_width,
        style_tokens.preview_default_height,
    );
    let center_x = source.x.saturating_add(source.width / 2);
    let center_y = source.y.saturating_add(source.height / 2);
    let bounds = monitor_bounds_for_point(center_x, center_y)
        .unwrap_or_else(|| fallback_preview_bounds(source, style_tokens));
    let geometry = compact_preview_geometry(bounds, style_tokens);

    preview::PreviewPlacement {
        geometry: preview::PreviewWindowGeometry {
            x: geometry.x,
            y: geometry.y,
            width: geometry.width,
            height: geometry.height,
        },
        min_width: geometry.width,
        min_height: geometry.height,
        max_width: geometry.width,
        max_height: geometry.height,
    }
}

fn compact_preview_geometry(
    bounds: preview::PreviewBounds,
    style_tokens: StyleTokens,
) -> RuntimeWindowGeometry {
    let margin = style_tokens.spacing_24.max(0);
    let available_width = bounds.width.saturating_sub(margin.saturating_mul(2)).max(1);
    let available_height = bounds
        .height
        .saturating_sub(margin.saturating_mul(2))
        .max(1);
    let width = style_tokens
        .preview_default_width
        .min(available_width)
        .max(1);
    let height = style_tokens
        .preview_default_height
        .min(available_height)
        .max(1);
    let x = bounds.x.saturating_add(margin);
    let y = bounds
        .y
        .saturating_add(bounds.height.saturating_sub(height).saturating_sub(margin));

    RuntimeWindowGeometry::with_position(x, y, width, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_preview_geometry_anchors_bottom_left_with_margin() {
        let bounds = preview::PreviewBounds {
            x: 100,
            y: 50,
            width: 1920,
            height: 1080,
        };

        let geometry = compact_preview_geometry(bounds, crate::ui::LAYOUT_TOKENS);

        assert_eq!(
            geometry,
            RuntimeWindowGeometry::with_position(124, 870, 420, 236)
        );
    }

    #[test]
    fn compact_preview_geometry_clamps_to_small_monitor_bounds() {
        let bounds = preview::PreviewBounds {
            x: 0,
            y: 0,
            width: 300,
            height: 200,
        };

        let geometry = compact_preview_geometry(bounds, crate::ui::LAYOUT_TOKENS);

        assert_eq!(
            geometry,
            RuntimeWindowGeometry::with_position(24, 24, 252, 152)
        );
    }

    #[test]
    fn bottom_centered_window_geometry_anchors_near_bottom_center() {
        let geometry = bottom_centered_window_geometry(
            preview::PreviewBounds {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            },
            RuntimeWindowGeometry::new(520, 176),
            24,
        );

        assert_eq!(geometry, (700, 880, 520, 176));
    }
}
