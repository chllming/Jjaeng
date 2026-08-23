/// Compile-time layout tokens — not user-overridable
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StyleTokens {
    pub spacing_4: i32,
    pub spacing_8: i32,
    pub spacing_12: i32,
    pub spacing_16: i32,
    pub spacing_20: i32,
    pub spacing_24: i32,
    pub card_radius: u16,
    pub panel_radius: u16,
    pub control_radius: u16,
    pub control_size: u16,
    pub icon_size: u16,
    pub border_width: u16,
    pub preview_default_width: i32,
    pub preview_default_height: i32,
    pub preview_min_width: i32,
    pub preview_min_height: i32,
    pub editor_initial_width: i32,
    pub editor_initial_height: i32,
    pub editor_min_width: i32,
    pub editor_min_height: i32,
    pub editor_toolbar_width: i32,
    pub motion_standard_ms: u32,
    pub motion_hover_ms: u32,
    pub toast_duration_ms: u32,
}

pub const LAYOUT_TOKENS: StyleTokens = StyleTokens {
    spacing_4: 4,
    spacing_8: 8,
    spacing_12: 12,
    spacing_16: 16,
    spacing_20: 20,
    spacing_24: 24,
    card_radius: 0,
    panel_radius: 0,
    control_radius: 0,
    control_size: 40,
    icon_size: 18,
    border_width: 1,
    preview_default_width: 252,
    preview_default_height: 142,
    preview_min_width: 252,
    preview_min_height: 142,
    editor_initial_width: 1280,
    editor_initial_height: 800,
    editor_min_width: 750,
    editor_min_height: 422,
    editor_toolbar_width: 68,
    motion_standard_ms: 220,
    motion_hover_ms: 160,
    toast_duration_ms: 2_000,
};

/// Apply Quattro's spacing scale and control dimensions to the shared layout
/// tokens while retaining the preview/editor product dimensions.
pub fn runtime_layout_tokens(
    spacing_scale: f32,
    control_height: u16,
    panel_padding: u16,
) -> StyleTokens {
    let scale = spacing_scale.clamp(0.5, 2.0);
    let px = |value: i32| ((value as f32 * scale).round() as i32).max(1);
    let radius = ((panel_padding as f32 * 0.18).round() as u16).min(16);
    StyleTokens {
        spacing_4: px(4),
        spacing_8: px(8),
        spacing_12: px(12),
        spacing_16: px(16),
        spacing_20: px(20),
        spacing_24: px(24),
        card_radius: radius,
        panel_radius: radius,
        control_radius: radius / 2,
        control_size: control_height.clamp(24, 96),
        ..LAYOUT_TOKENS
    }
}

#[cfg(test)]
mod tests {
    use super::LAYOUT_TOKENS;

    #[test]
    fn layout_tokens_keep_required_control_size() {
        assert_eq!(LAYOUT_TOKENS.control_size, 40);
    }

    #[test]
    fn layout_tokens_match_component_spec_dimensions() {
        let tokens = LAYOUT_TOKENS;
        assert_eq!(tokens.preview_min_width, 252);
        assert_eq!(tokens.preview_min_height, 142);
        assert_eq!(tokens.preview_default_width, 252);
        assert_eq!(tokens.preview_default_height, 142);
        assert_eq!(tokens.editor_initial_width, 1280);
        assert_eq!(tokens.editor_initial_height, 800);
        assert_eq!(tokens.editor_min_width, 750);
        assert_eq!(tokens.editor_min_height, 422);
        assert_eq!(tokens.editor_toolbar_width, 68);
    }

    #[test]
    fn layout_tokens_match_component_spec_motion_tokens() {
        let tokens = LAYOUT_TOKENS;
        assert_eq!(tokens.motion_standard_ms, 220);
        assert_eq!(tokens.motion_hover_ms, 160);
        assert_eq!(tokens.toast_duration_ms, 2_000);
    }
}
