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

fn preview_sizing_tokens(style_tokens: StyleTokens) -> preview::PreviewSizingTokens {
    preview::PreviewSizingTokens {
        default_width: style_tokens.preview_default_width,
        default_height: style_tokens.preview_default_height,
        min_width: style_tokens.preview_min_width,
        min_height: style_tokens.preview_min_height,
    }
}

fn compute_bottom_left_preview_placement_for_source(
    source: preview::PreviewSourceArea,
    bounds: preview::PreviewBounds,
    style_tokens: StyleTokens,
) -> preview::PreviewPlacement {
    let mut placement =
        preview::compute_preview_placement(source, bounds, preview_sizing_tokens(style_tokens));
    placement.geometry =
        bottom_left_preview_geometry(placement.geometry, bounds, style_tokens.spacing_24);
    placement
}

fn compute_bottom_left_preview_placement_for_anchor(
    source: preview::PreviewSourceArea,
    anchor_x: i32,
    anchor_y: i32,
    style_tokens: StyleTokens,
) -> preview::PreviewPlacement {
    let bounds = monitor_bounds_for_point(anchor_x, anchor_y)
        .unwrap_or_else(|| fallback_preview_bounds(source, style_tokens));
    compute_bottom_left_preview_placement_for_source(source, bounds, style_tokens)
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

pub(super) fn adjacent_window_geometry_for_area(
    area_x: i32,
    area_y: i32,
    area_width: i32,
    area_height: i32,
    window_geometry: RuntimeWindowGeometry,
    margin: i32,
) -> (i32, i32, i32, i32) {
    let width = window_geometry.width.max(1);
    let height = window_geometry.height.max(1);
    let center_x = area_x.saturating_add(area_width.max(1) / 2);
    let center_y = area_y.saturating_add(area_height.max(1) / 2);
    let bounds = monitor_bounds_for_point(center_x, center_y).unwrap_or(preview::PreviewBounds {
        x: area_x.saturating_sub(width / 2),
        y: area_y.saturating_sub(height / 2),
        width: area_width.max(width.saturating_mul(2)),
        height: area_height.max(height.saturating_mul(2)),
    });

    adjacent_window_geometry(
        bounds,
        area_x,
        area_y,
        area_width,
        area_height,
        window_geometry,
        margin,
    )
}

fn adjacent_window_geometry(
    bounds: preview::PreviewBounds,
    area_x: i32,
    area_y: i32,
    area_width: i32,
    area_height: i32,
    window_geometry: RuntimeWindowGeometry,
    margin: i32,
) -> (i32, i32, i32, i32) {
    let width = window_geometry.width.max(1);
    let height = window_geometry.height.max(1);
    let margin = margin.max(0);
    let center_x = area_x.saturating_add(area_width.max(1) / 2);

    let min_x = bounds.x;
    let max_x = bounds
        .x
        .saturating_add(bounds.width.saturating_sub(width))
        .max(min_x);
    let desired_x = center_x.saturating_sub(width / 2);
    let x = desired_x.clamp(min_x, max_x);

    let below_y = area_y
        .saturating_add(area_height.max(1))
        .saturating_add(margin);
    let above_y = area_y.saturating_sub(height).saturating_sub(margin);
    let min_y = bounds.y;
    let max_y = bounds
        .y
        .saturating_add(bounds.height.saturating_sub(height))
        .max(min_y);
    let y = if below_y.saturating_add(height) <= bounds.y.saturating_add(bounds.height) {
        below_y
    } else if above_y >= min_y {
        above_y
    } else {
        below_y.clamp(min_y, max_y)
    };

    (x, y, width, height)
}

fn bottom_left_preview_geometry(
    geometry: preview::PreviewWindowGeometry,
    bounds: preview::PreviewBounds,
    margin: i32,
) -> preview::PreviewWindowGeometry {
    let margin = margin.max(0);
    let width = geometry.width.max(1).min(bounds.width.max(1));
    let height = geometry.height.max(1).min(bounds.height.max(1));

    preview::PreviewWindowGeometry {
        x: bounds.x.saturating_add(margin),
        y: bounds
            .y
            .saturating_add(bounds.height.saturating_sub(height).saturating_sub(margin))
            .max(bounds.y),
        width,
        height,
    }
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
    compute_bottom_left_preview_placement_for_anchor(source, center_x, center_y, style_tokens)
}

pub(super) fn compute_media_preview_geometry_for_point(
    source_width: u32,
    source_height: u32,
    anchor_x: i32,
    anchor_y: i32,
    style_tokens: StyleTokens,
) -> preview::PreviewWindowGeometry {
    let width = i32::try_from(source_width)
        .ok()
        .filter(|value| *value > 0)
        .unwrap_or(style_tokens.preview_default_width.max(1));
    let height = i32::try_from(source_height)
        .ok()
        .filter(|value| *value > 0)
        .unwrap_or(style_tokens.preview_default_height.max(1));
    let source = preview::PreviewSourceArea {
        x: anchor_x.saturating_sub(width / 2),
        y: anchor_y.saturating_sub(height / 2),
        width,
        height,
    };

    compute_bottom_left_preview_placement_for_anchor(source, anchor_x, anchor_y, style_tokens)
        .geometry
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::LAYOUT_TOKENS;

    #[test]
    fn bottom_left_preview_geometry_anchors_preview_with_margin() {
        let bounds = preview::PreviewBounds {
            x: 100,
            y: 50,
            width: 1920,
            height: 1080,
        };
        let geometry = preview::PreviewWindowGeometry {
            x: 500,
            y: 400,
            width: 640,
            height: 360,
        };

        assert_eq!(
            bottom_left_preview_geometry(geometry, bounds, 24),
            preview::PreviewWindowGeometry {
                x: 124,
                y: 746,
                width: 640,
                height: 360,
            }
        );
    }

    #[test]
    fn compute_bottom_left_preview_placement_for_source_matches_preview_size_policy() {
        let placement = compute_bottom_left_preview_placement_for_source(
            preview::PreviewSourceArea {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            },
            preview::PreviewBounds {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            },
            LAYOUT_TOKENS,
        );

        assert_eq!(
            placement.geometry,
            preview::PreviewWindowGeometry {
                x: 24,
                y: 820,
                width: 420,
                height: 236,
            }
        );
    }

    #[test]
    fn adjacent_window_geometry_for_area_places_window_below_selection_when_possible() {
        let geometry = adjacent_window_geometry(
            preview::PreviewBounds {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            },
            500,
            300,
            420,
            180,
            RuntimeWindowGeometry::new(560, 56),
            12,
        );

        assert_eq!(geometry, (430, 492, 560, 56));
    }

    #[test]
    fn adjacent_window_geometry_for_area_places_window_above_selection_when_near_bottom() {
        let geometry = adjacent_window_geometry(
            preview::PreviewBounds {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            },
            500,
            980,
            420,
            80,
            RuntimeWindowGeometry::new(560, 56),
            12,
        );

        assert_eq!(geometry, (430, 912, 560, 56));
    }
}
