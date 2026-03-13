use crate::ui::{ColorTokens, StyleTokens};
use gtk4::CssProvider;
use jjaeng_core::theme::load_omarchy_menu_style;

fn css_font_family(font_family: &str) -> String {
    let trimmed = font_family.trim();
    if trimmed.is_empty() {
        return "monospace".into();
    }
    if trimmed.contains(',') || trimmed.starts_with('"') || trimmed.starts_with('\'') {
        return trimmed.to_string();
    }
    if trimmed.chars().any(char::is_whitespace) {
        return format!("\"{}\"", trimmed.replace('"', "\\\""));
    }
    trimmed.to_string()
}

fn scaled_font_size(base: u16, delta: i16, minimum: u16) -> u16 {
    let adjusted = i32::from(base).saturating_add(i32::from(delta));
    adjusted.max(i32::from(minimum)) as u16
}

pub(super) fn install_runtime_css(tokens: StyleTokens, colors: &ColorTokens, motion_enabled: bool) {
    let omarchy_menu_style = load_omarchy_menu_style();
    let close_icon_size = tokens.icon_size.saturating_add(2);
    let pin_icon_size = tokens.icon_size.saturating_add(1);
    let motion_standard_ms = if motion_enabled {
        tokens.motion_standard_ms
    } else {
        0
    };
    let motion_hover_ms = if motion_enabled {
        tokens.motion_hover_ms
    } else {
        0
    };
    let history_font_family = css_font_family(&omarchy_menu_style.font_family);
    let history_base_font_size = omarchy_menu_style.base_font_size_px.max(14);
    let history_meta_font_size = scaled_font_size(history_base_font_size, -6, 11);
    let history_button_font_size = scaled_font_size(history_base_font_size, -7, 11);
    let history_border_width = omarchy_menu_style.surface_border_width_px.max(1);
    let history_inner_border_width = history_border_width.saturating_sub(1).max(1);
    let history_surface_alpha = omarchy_menu_style.surface_background_alpha.clamp(0.0, 1.0);
    let history_tile_alpha = (history_surface_alpha + 0.03).min(0.98);
    let history_overlay_top_alpha = (history_surface_alpha * 0.82).clamp(0.44, 0.90);
    let history_overlay_mid_alpha = (history_surface_alpha * 0.58).clamp(0.28, 0.72);
    let css = format!(
        "
window.jjaeng-root {{
  background: transparent;
  color: {text_color};
}}
.jjaeng-root,
.jjaeng-root label,
.jjaeng-root button,
.jjaeng-root entry,
.jjaeng-root text,
.jjaeng-root combobox,
.jjaeng-root dropdown,
.jjaeng-root popover,
.jjaeng-root scale {{
  font-family: {history_font_family};
  color: {text_color};
}}
.jjaeng-root button {{
  font-size: {history_button_font_size}px;
}}
.jjaeng-root entry,
.jjaeng-root text,
.jjaeng-root combobox,
.jjaeng-root dropdown {{
  font-size: {history_meta_font_size}px;
}}
.jjaeng-root button:hover,
.jjaeng-root button:active {{
  color: {text_color};
}}
.jjaeng-root button image {{
  -gtk-icon-style: symbolic;
  color: inherit;
}}
.jjaeng-root button:hover image,
.jjaeng-root button:active image {{
  color: inherit;
}}
tooltip {{
  border-radius: 0;
  border: {history_border_width}px solid alpha({text_color}, 0.18);
  background: alpha({canvas_background}, {history_surface_alpha:.2});
  color: {text_color};
  box-shadow: none;
  font-family: {history_font_family};
}}
tooltip label {{
  color: {text_color};
  font-family: {history_font_family};
}}
popover {{
  border-radius: 0;
  border: {history_border_width}px solid alpha({text_color}, 0.18);
  background: alpha({canvas_background}, {history_surface_alpha:.2});
  box-shadow: none;
}}
popover contents,
popover listview,
popover row,
popover modelbutton {{
  border-radius: 0;
  background: transparent;
  color: {text_color};
  font-family: {history_font_family};
}}
popover row:hover,
popover row:selected,
popover modelbutton:hover,
popover modelbutton:active {{
  background: alpha({canvas_background}, {history_tile_alpha:.2});
}}
scrollbar slider {{
  border-radius: 0;
  border: {history_inner_border_width}px solid alpha({text_color}, 0.16);
  background: alpha({canvas_background}, {history_tile_alpha:.2});
  box-shadow: none;
}}
scrollbar slider:hover {{
  border-color: {focus_ring_color};
}}
.preview-surface,
.editor-surface {{
  border-radius: 0;
  border: {history_border_width}px solid alpha({text_color}, 0.82);
  background: alpha({canvas_background}, {history_surface_alpha:.2});
  padding: 0;
  transition: opacity {motion_standard_ms}ms cubic-bezier(0.4, 0, 0.2, 1);
  box-shadow: none;
}}
window.floating-preview-window,
window.floating-editor-window {{
  background: transparent;
}}
.transparent-bg {{
  background: transparent;
}}
/* ── Shared elevated panels ── */
.editor-toolbar,
.editor-action-group,
.preview-action-group {{
  border-radius: 0;
  border: {history_border_width}px solid alpha({text_color}, 0.18);
  background: alpha({canvas_background}, {history_surface_alpha:.2});
  padding: {spacing_4}px;
  box-shadow: none;
}}
.launchpad-root button,
.history-root button,
.recording-prompt-surface button,
button.preview-quick-action {{
  min-height: 32px;
  padding: 0 {spacing_12}px;
  border-radius: 0;
  border: {history_inner_border_width}px solid alpha({text_color}, 0.16);
  background: alpha({canvas_background}, {history_surface_alpha:.2});
  box-shadow: none;
  transition: color {motion_hover_ms}ms cubic-bezier(0.4, 0, 0.2, 1),
              border-color {motion_hover_ms}ms cubic-bezier(0.4, 0, 0.2, 1),
              background {motion_hover_ms}ms cubic-bezier(0.4, 0, 0.2, 1);
}}
.launchpad-root button:hover,
.launchpad-root button:active,
.history-root button:hover,
.history-root button:active,
.recording-prompt-surface button:hover,
.recording-prompt-surface button:active,
button.preview-quick-action:hover,
button.preview-quick-action:active {{
  border-color: {focus_ring_color};
  background: alpha({canvas_background}, {history_tile_alpha:.2});
  box-shadow: none;
}}
.launchpad-root entry,
.launchpad-root text,
.launchpad-root combobox button,
.launchpad-root dropdown > button,
.recording-prompt-surface entry,
.recording-prompt-surface text,
.recording-prompt-surface combobox button,
.recording-prompt-surface dropdown > button {{
  border-radius: 0;
  border: {history_inner_border_width}px solid alpha({text_color}, 0.18);
  background: alpha({canvas_background}, {history_surface_alpha:.2});
  box-shadow: none;
  color: {text_color};
}}
.jjaeng-root entry:focus,
.jjaeng-root text:focus,
.jjaeng-root combobox button:focus,
.jjaeng-root dropdown > button:focus {{
  border-color: {focus_ring_color};
  box-shadow: none;
}}
.icon-button {{
  border-radius: 0;
  min-width: {control_size}px;
  min-height: {control_size}px;
  padding: 0;
  border: {history_inner_border_width}px solid alpha({text_color}, 0.16);
  background: alpha({canvas_background}, {history_surface_alpha:.2});
  box-shadow: none;
  transition: color {motion_hover_ms}ms cubic-bezier(0.4, 0, 0.2, 1),
              border-color {motion_hover_ms}ms cubic-bezier(0.4, 0, 0.2, 1),
              background {motion_hover_ms}ms cubic-bezier(0.4, 0, 0.2, 1);
}}
.icon-button:hover {{
  color: {text_color};
  background: alpha({canvas_background}, {history_tile_alpha:.2});
  border-color: {focus_ring_color};
  box-shadow: none;
}}
.icon-button:active {{
  color: {text_color};
  background: alpha({canvas_background}, {history_tile_alpha:.2});
  border-color: {focus_ring_color};
  box-shadow: none;
  transition: color 60ms ease, border-color 60ms ease, background 60ms ease;
}}
.icon-button:disabled {{
  opacity: 0.38;
  box-shadow: none;
}}
.icon-button:disabled:hover,
.icon-button:disabled:active {{
  box-shadow: none;
}}
.editor-toolbar button.tool-active {{
  background-image: linear-gradient(
                      rgba(0, 0, 0, 0.24),
                      rgba(0, 0, 0, 0.24)
                    ),
                    {accent_gradient};
  background-origin: border-box;
  color: {accent_text_color};
  border-color: transparent;
  box-shadow: none;
}}
.editor-toolbar button.tool-active image,
.editor-toolbar button.tool-active:hover image,
.editor-toolbar button.tool-active:active image {{
  color: {accent_text_color};
}}
.editor-toolbar button.tool-active:hover,
.editor-toolbar button.tool-active:active {{
  color: {accent_text_color};
}}

/* ── Close button base + hover ── */
button.editor-close-button {{
  border-radius: 0;
  padding: 0;
  border: {history_inner_border_width}px solid rgba(255, 80, 80, 0.24);
  background: rgba(255, 80, 80, 0.10);
  color: rgba(255, 80, 80, 0.85);
}}
button.editor-close-button:hover {{
  background: rgba(255, 80, 80, 0.22);
  border-color: rgba(255, 80, 80, 0.4);
  color: rgba(255, 60, 60, 1.0);
  box-shadow: none;
}}
button.preview-close-button {{
  border-radius: 0;
  padding: 0;
  border: {history_inner_border_width}px solid rgba(255, 80, 80, 0.32);
  background: rgba(255, 80, 80, 0.24);
  color: rgba(255, 64, 64, 1.0);
  box-shadow: none;
}}
button.preview-close-button:hover,
button.preview-close-button:active {{
  background: rgba(255, 80, 80, 0.36);
  border-color: rgba(255, 80, 80, 0.58);
  color: rgba(255, 45, 45, 1.0);
  box-shadow: none;
}}
button.editor-close-button image,
button.preview-close-button image {{
  -gtk-icon-size: {close_icon_size}px;
}}

/* ── Editor bottom controls ── */
.editor-bottom-controls {{
  padding: 0;
}}
.editor-options-bar,
.preview-bottom-controls {{
  border-radius: 0;
  border: {history_border_width}px solid alpha({text_color}, 0.18);
  background: alpha({canvas_background}, {history_surface_alpha:.2});
  padding: {spacing_8}px {spacing_12}px;
  box-shadow: none;
}}
.editor-options-header {{
  min-height: {control_size}px;
}}
.editor-options-collapsed .editor-options-header {{
  min-height: 30px;
}}
.editor-options-collapsed-row {{
  min-height: 30px;
}}
button.editor-options-toggle {{
  min-width: 30px;
  min-height: 30px;
  border-radius: 0;
  padding: 0;
  border: {history_inner_border_width}px solid alpha({text_color}, 0.16);
  background: alpha({canvas_background}, {history_surface_alpha:.2});
  box-shadow: none;
}}
button.editor-options-toggle image {{
  -gtk-icon-size: 16px;
}}
button.editor-options-toggle:hover {{
  border-color: {focus_ring_color};
  background: alpha({canvas_background}, {history_tile_alpha:.2});
  box-shadow: none;
}}
.editor-options-collapsed {{
  padding: {spacing_8}px {spacing_8}px;
}}
.stroke-options-title {{
  font-size: 11px;
  opacity: 0.86;
  margin-left: 2px;
}}
button.stroke-chip-button {{
  border-radius: 0;
  min-width: 30px;
  min-height: 30px;
  padding: 0;
  border: {history_inner_border_width}px solid alpha({text_color}, 0.16);
  background: alpha({canvas_background}, {history_surface_alpha:.2});
  box-shadow: none;
  transition: border-color {motion_hover_ms}ms cubic-bezier(0.4, 0, 0.2, 1),
              background {motion_hover_ms}ms cubic-bezier(0.4, 0, 0.2, 1);
}}
button.stroke-chip-button:hover {{
  border-color: {focus_ring_color};
  background: alpha({canvas_background}, {history_tile_alpha:.2});
  box-shadow: none;
}}
button.stroke-chip-active {{
  border-color: {focus_ring_color};
  background: alpha({canvas_background}, {history_tile_alpha:.2});
  box-shadow: none;
}}
/* ── Shared slider style ── */
.accent-slider trough {{
  min-height: 4px;
  border-radius: 0;
  border: {history_inner_border_width}px solid alpha({text_color}, 0.14);
  background: alpha({canvas_background}, {history_surface_alpha:.2});
}}
.accent-slider highlight {{
  border-radius: 0;
  background-image: {accent_gradient};
  background-origin: border-box;
  box-shadow: none;
}}
.accent-slider slider {{
  min-width: 16px;
  min-height: 16px;
  border-radius: 0;
  border: {history_inner_border_width}px solid alpha({text_color}, 0.18);
  background-image: {accent_gradient};
  background-origin: border-box;
  box-shadow: none;
  transition: border-color {motion_hover_ms}ms cubic-bezier(0.4, 0, 0.2, 1);
}}
.accent-slider slider:hover {{
  border-color: {focus_ring_color};
  box-shadow: none;
}}
.editor-zoom-slider {{
  min-width: 160px;
}}
.editor-canvas {{
  border-radius: 0;
  border: {history_border_width}px solid alpha({text_color}, 0.18);
  background: {canvas_background};
}}

/* ── Preview controls revealer ── */
.preview-controls-revealer {{
  transition: opacity {motion_hover_ms}ms cubic-bezier(0.4, 0, 0.2, 1);
}}
.preview-top-controls {{
  padding: 0;
  border-radius: 0;
}}
button.preview-quick-action {{
  min-height: 30px;
  min-width: 76px;
  padding: 0 {spacing_8}px;
  border-radius: 0;
  font-size: {history_button_font_size}px;
  font-weight: 600;
}}
button.preview-quick-action .preview-shortcut-hint {{
  opacity: 0.68;
  font-size: {history_meta_font_size}px;
}}
button.preview-quick-action.suggested-action {{
  background-image: linear-gradient(
                      rgba(0, 0, 0, 0.20),
                      rgba(0, 0, 0, 0.20)
                    ),
                    {accent_gradient};
  color: {accent_text_color};
  border-color: transparent;
  box-shadow: none;
}}
button.preview-quick-action.suggested-action:hover,
button.preview-quick-action.suggested-action:active {{
  background-image: linear-gradient(
                      rgba(0, 0, 0, 0.20),
                      rgba(0, 0, 0, 0.20)
                    ),
                    {accent_gradient};
  color: {accent_text_color};
  border-color: transparent;
  box-shadow: none;
}}

/* ── Icon buttons (shared base) ── */
button.preview-round-button {{
  border-radius: 0;
  padding: 0;
}}

/* ── Pin toggle: neutral by default, emphasized when pinned ── */
button.preview-pin-toggle {{
  border-radius: 0;
  padding: 0;
  border: {history_inner_border_width}px solid alpha({text_color}, 0.16);
  background: alpha({canvas_background}, {history_surface_alpha:.2});
  color: {text_color};
  box-shadow: none;
}}
button.preview-pin-toggle image {{
  -gtk-icon-size: {pin_icon_size}px;
}}
button.preview-pin-toggle:hover,
button.preview-pin-toggle:active {{
  border-color: {focus_ring_color};
  background: alpha({canvas_background}, {history_tile_alpha:.2});
  color: {text_color};
  box-shadow: none;
}}
button.preview-pin-toggle:checked {{
  background-image: linear-gradient(
                      rgba(0, 0, 0, 0.24),
                      rgba(0, 0, 0, 0.24)
                    ),
                    {accent_gradient};
  background-origin: border-box;
  border-color: transparent;
  color: {accent_text_color};
  box-shadow: none;
}}
button.preview-pin-toggle:checked:hover,
button.preview-pin-toggle:checked:active {{
  background-image: linear-gradient(
                      rgba(0, 0, 0, 0.24),
                      rgba(0, 0, 0, 0.24)
                    ),
                    {accent_gradient};
  border-color: transparent;
  color: {accent_text_color};
  box-shadow: none;
}}

/* ── Opacity slider ── */
.preview-opacity-slider {{
  min-width: 180px;
}}

/* ── Launchpad layout ── */
.launchpad-root {{
  border-radius: 0;
  border: {history_border_width}px solid alpha({text_color}, 0.82);
  background: alpha({canvas_background}, {history_surface_alpha:.2});
  box-shadow: none;
}}
label.launchpad-title {{
  font-size: 18px;
  font-weight: 700;
}}
label.launchpad-subtitle,
label.launchpad-hint {{
  opacity: 0.8;
}}
label.launchpad-section-title {{
  font-size: 13px;
  font-weight: 650;
  opacity: 0.92;
}}
label.launchpad-kv-key {{
  font-size: 12px;
  opacity: 0.56;
  min-width: 72px;
}}
label.launchpad-kv-value {{
  font-size: 13px;
}}
label.launchpad-version {{
  font-size: 11px;
  opacity: 0.5;
  padding: 2px 8px;
  border-radius: 0;
  border: {history_inner_border_width}px solid alpha({text_color}, 0.18);
  background: alpha({canvas_background}, {history_surface_alpha:.2});
}}
.launchpad-info-row > * {{
  min-width: 0;
}}
label.launchpad-capture-ids {{
  font-size: 12px;
  padding-top: {spacing_4}px;
  border-top: {history_inner_border_width}px solid alpha({text_color}, 0.16);
}}
frame.launchpad-panel {{
  border-radius: 0;
  border: {history_inner_border_width}px solid alpha({text_color}, 0.16);
  background: alpha({canvas_background}, {history_surface_alpha:.2});
  padding: {spacing_12}px;
}}
frame.launchpad-panel > border {{
  border: none;
}}
box.launchpad-recording-actions-row button,
box.launchpad-recording-field-row,
box.launchpad-recording-mic-row {{
  min-height: 32px;
}}
box.launchpad-recording-controls,
box.launchpad-recording-summary {{
  min-width: 0;
}}
label.launchpad-recording-field-label {{
  min-width: 72px;
}}
box.launchpad-recording-field-row combobox,
box.launchpad-recording-mic-row entry {{
  min-width: 168px;
}}
.launchpad-root button.launchpad-primary-button {{
  background-image: linear-gradient(
                      rgba(0, 0, 0, 0.20),
                      rgba(0, 0, 0, 0.20)
                    ),
                    {accent_gradient};
  color: {accent_text_color};
  border-color: transparent;
  font-weight: 600;
}}
.launchpad-root button.launchpad-primary-button:hover,
.launchpad-root button.launchpad-primary-button:active {{
  background-image: linear-gradient(
                      rgba(0, 0, 0, 0.20),
                      rgba(0, 0, 0, 0.20)
                    ),
                    {accent_gradient};
  color: {accent_text_color};
  border-color: transparent;
}}
.launchpad-root button.launchpad-danger-button {{
  background: rgba(255, 80, 80, 0.08);
  color: rgba(255, 80, 80, 0.92);
  border-color: rgba(255, 80, 80, 0.22);
}}
.launchpad-root button.launchpad-danger-button:hover,
.launchpad-root button.launchpad-danger-button:active {{
  background: rgba(255, 80, 80, 0.16);
  color: rgba(255, 65, 65, 1.0);
  border-color: rgba(255, 80, 80, 0.34);
}}

/* ── Recording prompt ── */
window.recording-selection-window,
window.recording-prompt-window {{
  background: transparent;
}}
frame.recording-selection-frame {{
  border-radius: 0;
  border: {history_border_width}px solid {focus_ring_color};
  background: alpha({canvas_background}, 0.04);
  box-shadow: none;
}}
frame.recording-selection-frame > border {{
  border: none;
}}
box.recording-prompt-surface {{
  border-radius: 0;
  border: {history_border_width}px solid alpha({text_color}, 0.82);
  background: alpha({canvas_background}, {history_surface_alpha:.2});
  box-shadow: none;
}}
.recording-prompt-surface,
.recording-prompt-surface label,
.recording-prompt-surface button,
.recording-prompt-surface entry,
.recording-prompt-surface combobox {{
  font-family: {history_font_family};
  color: {text_color};
}}
label.recording-prompt-title {{
  font-size: {history_base_font_size}px;
  font-weight: 700;
}}
label.recording-prompt-timer {{
  font-size: {history_meta_font_size}px;
  font-weight: 700;
  opacity: 0.92;
}}
label.recording-prompt-meta,
label.recording-prompt-control-label {{
  font-size: {history_meta_font_size}px;
  opacity: 0.82;
}}
label.recording-prompt-status,
label.recording-prompt-hint {{
  font-size: {history_meta_font_size}px;
  opacity: 0.82;
}}
box.recording-prompt-controls {{
  min-width: 0;
}}
box.recording-prompt-control-row,
box.recording-prompt-mic-row {{
  min-height: 32px;
}}
label.recording-prompt-control-label {{
  min-width: 88px;
}}
box.recording-prompt-control-row combobox,
box.recording-prompt-mic-row combobox {{
  min-width: 180px;
}}
button.recording-prompt-button {{
  min-height: 32px;
  border-radius: 0;
}}
box.recording-prompt-button-row button.recording-prompt-button {{
  min-width: 0;
}}
button.recording-prompt-button-primary {{
  background-image: linear-gradient(
                      rgba(0, 0, 0, 0.20),
                      rgba(0, 0, 0, 0.20)
                    ),
                    {accent_gradient};
  color: {accent_text_color};
  border-color: transparent;
  font-weight: 600;
}}
button.recording-prompt-button-primary:hover,
button.recording-prompt-button-primary:active {{
  background-image: linear-gradient(
                      rgba(0, 0, 0, 0.20),
                      rgba(0, 0, 0, 0.20)
                    ),
                    {accent_gradient};
  color: {accent_text_color};
  border-color: transparent;
}}
button.recording-prompt-button-danger {{
  background: rgba(255, 80, 80, 0.08);
  color: rgba(255, 80, 80, 0.92);
  border-color: rgba(255, 80, 80, 0.22);
}}
button.recording-prompt-button-danger:hover,
button.recording-prompt-button-danger:active {{
  background: rgba(255, 80, 80, 0.16);
  color: rgba(255, 65, 65, 1.0);
  border-color: rgba(255, 80, 80, 0.34);
}}

/* ── History window ── */
window.history-window {{
  background: transparent;
}}
.history-root {{
  border-radius: 0;
  border: {history_border_width}px solid alpha({text_color}, 0.82);
  background: alpha({canvas_background}, {history_surface_alpha:.2});
  box-shadow: none;
}}
.history-root,
.history-root label,
.history-root button {{
  font-family: {history_font_family};
  color: {text_color};
}}
frame.history-header-card,
frame.history-tile,
frame.history-thumbnail-frame {{
  border-radius: 0;
  border: {history_inner_border_width}px solid alpha({text_color}, 0.14);
  background: alpha({canvas_background}, {history_surface_alpha:.2});
}}
frame.history-header-card > border,
frame.history-tile > border,
frame.history-thumbnail-frame > border {{
  border: none;
}}
frame.history-header-card {{
  background: transparent;
  border: none;
}}
label.history-kicker {{
  font-size: {history_meta_font_size}px;
  font-weight: 700;
  opacity: 0.70;
}}
label.history-title {{
  font-size: {history_base_font_size}px;
  font-weight: 700;
}}
label.history-subtitle,
label.history-tile-meta,
label.history-shortcut-tip,
label.history-empty-state {{
  opacity: 0.76;
}}
label.history-count {{
  font-size: {history_meta_font_size}px;
  font-weight: 600;
  opacity: 1.0;
  padding: 4px 10px;
  border-radius: 0;
  border: {history_inner_border_width}px solid alpha({text_color}, 0.18);
  background: alpha({canvas_background}, {history_surface_alpha:.2});
}}
box.history-filter-row button.history-filter-button {{
  min-height: 26px;
  padding: 2px 10px;
  font-size: {history_button_font_size}px;
  border-radius: 0;
  border: {history_inner_border_width}px solid alpha({text_color}, 0.16);
  background: alpha({canvas_background}, {history_surface_alpha:.2});
}}
box.history-filter-row button.history-filter-button.history-filter-active {{
  background-image: linear-gradient(
                      rgba(0, 0, 0, 0.20),
                      rgba(0, 0, 0, 0.20)
                    ),
                    {accent_gradient};
  color: {accent_text_color};
  border-color: transparent;
}}
box.history-filter-row button.history-filter-button.history-filter-active:hover,
box.history-filter-row button.history-filter-button.history-filter-active:active {{
  background-image: linear-gradient(
                      rgba(0, 0, 0, 0.20),
                      rgba(0, 0, 0, 0.20)
                    ),
                    {accent_gradient};
  color: {accent_text_color};
  border-color: transparent;
}}
frame.history-tile {{
  min-width: 264px;
}}
frame.history-thumbnail-frame {{
  background: alpha({canvas_background}, {history_tile_alpha:.2});
}}
label.history-media-badge {{
  padding: 3px 7px;
  border-radius: 0;
  border: {history_inner_border_width}px solid alpha({text_color}, 0.18);
  font-size: {history_button_font_size}px;
  font-weight: 700;
  background: alpha({canvas_background}, {history_tile_alpha:.2});
}}
label.history-media-badge-video {{
  border-color: alpha({focus_ring_color}, 0.45);
  color: {focus_ring_color};
}}
label.history-tile-title {{
  font-size: {history_meta_font_size}px;
  font-weight: 650;
}}
label.history-tile-meta,
label.history-shortcut-tip {{
  font-size: {history_meta_font_size}px;
}}
label.history-status-chip {{
  padding: 3px 8px;
  border-radius: 0;
  border: {history_inner_border_width}px solid alpha({text_color}, 0.16);
  font-size: {history_button_font_size}px;
  font-weight: 600;
}}
label.history-status-saved {{
  background: alpha({canvas_background}, {history_surface_alpha:.2});
}}
label.history-status-unsaved {{
  opacity: 0.72;
  background: alpha({canvas_background}, {history_surface_alpha:.2});
}}
box.history-info-row {{
  min-height: 24px;
}}
revealer.history-action-revealer {{
  background: linear-gradient(to top,
                              rgba(0, 0, 0, {history_overlay_top_alpha:.2}),
                              rgba(0, 0, 0, {history_overlay_mid_alpha:.2}),
                              rgba(0, 0, 0, 0.0));
}}
box.history-action-row button.history-action-button {{
  min-width: 64px;
  min-height: 30px;
  padding: 4px 10px;
  font-size: {history_button_font_size}px;
  border-radius: 0;
  border: {history_inner_border_width}px solid alpha({text_color}, 0.16);
  background: alpha({canvas_background}, {history_tile_alpha:.2});
}}
box.history-action-row button.history-action-button.suggested-action {{
  background-image: linear-gradient(
                      rgba(0, 0, 0, 0.20),
                      rgba(0, 0, 0, 0.20)
                    ),
                    {accent_gradient};
  color: {accent_text_color};
  border-color: transparent;
}}
box.history-action-row button.history-action-button.suggested-action:hover,
box.history-action-row button.history-action-button.suggested-action:active {{
  background-image: linear-gradient(
                      rgba(0, 0, 0, 0.20),
                      rgba(0, 0, 0, 0.20)
                    ),
                    {accent_gradient};
  color: {accent_text_color};
  border-color: transparent;
}}

/* ── Toast badge ── */
.toast-badge {{
  border-radius: 0;
  border: {history_border_width}px solid alpha({text_color}, 0.82);
  background: alpha({canvas_background}, {history_surface_alpha:.2});
  color: {text_color};
  padding: {spacing_8}px {spacing_16}px;
  font-size: 13px;
  font-weight: 500;
  font-family: {history_font_family};
  box-shadow: none;
}}

/* ── Focus visible ── */
.jjaeng-root button:focus-visible,
.jjaeng-root scale:focus-visible,
.jjaeng-root entry:focus-visible,
.jjaeng-root text:focus-visible,
.jjaeng-root combobox button:focus-visible,
.jjaeng-root dropdown > button:focus-visible {{
  border-color: {focus_ring_color};
  box-shadow: none;
}}
",
        accent_gradient = colors.accent_gradient,
        accent_text_color = colors.accent_text_color,
        canvas_background = colors.canvas_background,
        text_color = colors.text_color,
        focus_ring_color = colors.focus_ring_color,
        history_font_family = history_font_family,
        history_base_font_size = history_base_font_size,
        history_meta_font_size = history_meta_font_size,
        history_button_font_size = history_button_font_size,
        history_border_width = history_border_width,
        history_inner_border_width = history_inner_border_width,
        history_surface_alpha = history_surface_alpha,
        history_tile_alpha = history_tile_alpha,
        history_overlay_top_alpha = history_overlay_top_alpha,
        history_overlay_mid_alpha = history_overlay_mid_alpha,
        spacing_8 = tokens.spacing_8,
        spacing_4 = tokens.spacing_4,
        spacing_12 = tokens.spacing_12,
        spacing_16 = tokens.spacing_16,
        control_size = tokens.control_size,
        close_icon_size = close_icon_size,
        pin_icon_size = pin_icon_size,
        motion_standard_ms = motion_standard_ms,
        motion_hover_ms = motion_hover_ms,
    );

    let provider = CssProvider::new();
    provider.load_from_data(&css);
    if let Some(display) = gtk4::gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
