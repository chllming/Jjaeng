use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use crate::ui::StyleTokens;
use gtk4::prelude::*;
use gtk4::{
    Align, Application, ApplicationWindow, Box as GtkBox, Button, Frame, GestureClick, Orientation,
    Overflow, Overlay, Revealer, RevealerTransitionType,
};
use jjaeng_core::capture;
use jjaeng_core::input::{resolve_shortcut, InputContext, InputMode, ShortcutAction};
use jjaeng_core::preview;

use super::hover_controls::set_revealer_visibility;
use super::hypr::request_window_floating_with_geometry;
use super::input_bridge::{normalize_shortcut_key, shortcut_modifiers};
use super::layout::compute_initial_preview_placement;
use super::runtime_support::{
    close_all_preview_windows, close_preview_window_for_capture, PreviewWindowRuntime, ToastRuntime,
};
use super::window_state::RuntimeWindowState;
use super::{close_editor_if_open_and_clear, EditorRuntimeState};

#[derive(Clone)]
pub(super) struct PreviewRenderContext {
    app: Application,
    style_tokens: StyleTokens,
    motion_hover_ms: u32,
    status_log: Rc<RefCell<String>>,
    save_button: Button,
    copy_button: Button,
    ocr_button: Button,
    open_editor_button: Button,
    close_preview_button: Button,
    delete_button: Button,
    preview_windows: Rc<RefCell<HashMap<String, PreviewWindowRuntime>>>,
    preview_action_target_capture_id: Rc<RefCell<Option<String>>>,
    runtime_window_state: Rc<RefCell<RuntimeWindowState>>,
    editor_window: Rc<RefCell<Option<ApplicationWindow>>>,
    editor_close_guard: Rc<Cell<bool>>,
    editor_runtime: Rc<EditorRuntimeState>,
    ocr_available: bool,
}

impl PreviewRenderContext {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        app: Application,
        style_tokens: StyleTokens,
        motion_hover_ms: u32,
        status_log: Rc<RefCell<String>>,
        save_button: Button,
        copy_button: Button,
        ocr_button: Button,
        open_editor_button: Button,
        close_preview_button: Button,
        delete_button: Button,
        preview_windows: Rc<RefCell<HashMap<String, PreviewWindowRuntime>>>,
        preview_action_target_capture_id: Rc<RefCell<Option<String>>>,
        runtime_window_state: Rc<RefCell<RuntimeWindowState>>,
        editor_window: Rc<RefCell<Option<ApplicationWindow>>>,
        editor_close_guard: Rc<Cell<bool>>,
        editor_runtime: Rc<EditorRuntimeState>,
        ocr_available: bool,
    ) -> Self {
        Self {
            app,
            style_tokens,
            motion_hover_ms,
            status_log,
            save_button,
            copy_button,
            ocr_button,
            open_editor_button,
            close_preview_button,
            delete_button,
            preview_windows,
            preview_action_target_capture_id,
            runtime_window_state,
            editor_window,
            editor_close_guard,
            editor_runtime,
            ocr_available,
        }
    }
}

pub(super) fn render_preview_state(
    context: &PreviewRenderContext,
    captures: &[capture::CaptureArtifact],
) {
    close_editor_if_open_and_clear(
        &context.editor_window,
        &context.runtime_window_state,
        &context.editor_close_guard,
        context.editor_runtime.as_ref(),
        context.style_tokens,
    );
    close_stale_preview_windows(
        captures,
        &context.preview_windows,
        &context.runtime_window_state,
    );
    prune_stale_preview_window_geometries(captures, &context.runtime_window_state);

    for artifact in captures {
        if let Some(runtime) = context
            .preview_windows
            .borrow()
            .get(&artifact.capture_id)
            .cloned()
        {
            sync_existing_preview_runtime(&runtime);
            continue;
        }
        create_preview_window_for_capture(context, artifact);
    }

    if captures.is_empty() {
        close_all_preview_windows(&context.preview_windows, &context.runtime_window_state);
    }
}

fn sync_existing_preview_runtime(runtime: &PreviewWindowRuntime) {
    set_revealer_visibility(&runtime.controls, true);
    runtime
        .preview_surface
        .set_opacity(runtime.shell.borrow().transparency() as f64);
}

fn close_stale_preview_windows(
    captures: &[capture::CaptureArtifact],
    preview_windows: &Rc<RefCell<HashMap<String, PreviewWindowRuntime>>>,
    runtime_window_state: &Rc<RefCell<RuntimeWindowState>>,
) {
    let capture_ids = captures
        .iter()
        .map(|artifact| artifact.capture_id.clone())
        .collect::<Vec<_>>();
    let stale_preview_ids = preview_windows
        .borrow()
        .keys()
        .filter(|capture_id| !capture_ids.iter().any(|id| id == *capture_id))
        .cloned()
        .collect::<Vec<_>>();
    for capture_id in stale_preview_ids {
        close_preview_window_for_capture(preview_windows, &capture_id, runtime_window_state);
    }
}

fn prune_stale_preview_window_geometries(
    captures: &[capture::CaptureArtifact],
    runtime_window_state: &Rc<RefCell<RuntimeWindowState>>,
) {
    let capture_ids = captures
        .iter()
        .map(|artifact| artifact.capture_id.clone())
        .collect::<Vec<_>>();
    let stale_geometry_ids = runtime_window_state
        .borrow()
        .preview_geometry_capture_ids()
        .into_iter()
        .filter(|capture_id| !capture_ids.iter().any(|id| id == capture_id))
        .collect::<Vec<_>>();
    if stale_geometry_ids.is_empty() {
        return;
    }
    let mut state = runtime_window_state.borrow_mut();
    for capture_id in stale_geometry_ids {
        state.remove_preview_geometry_for_capture(&capture_id);
    }
}

fn connect_preview_action_bridge(
    trigger_button: &Button,
    launchpad_button: &Button,
    preview_action_target_capture_id: &Rc<RefCell<Option<String>>>,
    capture_id: &str,
) {
    let launchpad_button = launchpad_button.clone();
    let preview_action_target_capture_id = preview_action_target_capture_id.clone();
    let capture_id = capture_id.to_string();
    trigger_button.connect_clicked(move |_| {
        *preview_action_target_capture_id.borrow_mut() = Some(capture_id.clone());
        launchpad_button.emit_clicked();
    });
}

fn connect_preview_action_bridges(
    bridges: &[(&Button, &Button)],
    preview_action_target_capture_id: &Rc<RefCell<Option<String>>>,
    capture_id: &str,
) {
    for (trigger_button, launchpad_button) in bridges {
        connect_preview_action_bridge(
            trigger_button,
            launchpad_button,
            preview_action_target_capture_id,
            capture_id,
        );
    }
}

#[derive(Clone)]
struct PreviewLaunchpadButtons {
    save_button: Button,
    copy_button: Button,
    ocr_button: Button,
    open_editor_button: Button,
    close_preview_button: Button,
    delete_button: Button,
    ocr_available: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreviewShortcutTarget {
    Save,
    Copy,
    Ocr,
    Edit,
    Delete,
    Close,
}

fn preview_shortcut_target(action: ShortcutAction) -> Option<PreviewShortcutTarget> {
    match action {
        ShortcutAction::PreviewSave => Some(PreviewShortcutTarget::Save),
        ShortcutAction::PreviewCopy => Some(PreviewShortcutTarget::Copy),
        ShortcutAction::PreviewOcr => Some(PreviewShortcutTarget::Ocr),
        ShortcutAction::PreviewEdit => Some(PreviewShortcutTarget::Edit),
        ShortcutAction::PreviewDelete => Some(PreviewShortcutTarget::Delete),
        ShortcutAction::PreviewClose => Some(PreviewShortcutTarget::Close),
        _ => None,
    }
}

impl PreviewLaunchpadButtons {
    fn from_context(context: &PreviewRenderContext) -> Self {
        Self {
            save_button: context.save_button.clone(),
            copy_button: context.copy_button.clone(),
            ocr_button: context.ocr_button.clone(),
            open_editor_button: context.open_editor_button.clone(),
            close_preview_button: context.close_preview_button.clone(),
            delete_button: context.delete_button.clone(),
            ocr_available: context.ocr_available,
        }
    }

    fn emit_shortcut_action(&self, action: ShortcutAction) -> bool {
        match preview_shortcut_target(action) {
            Some(PreviewShortcutTarget::Save) => self.save_button.emit_clicked(),
            Some(PreviewShortcutTarget::Copy) => self.copy_button.emit_clicked(),
            Some(PreviewShortcutTarget::Ocr) => {
                if self.ocr_available {
                    self.ocr_button.emit_clicked();
                } else {
                    return false;
                }
            }
            Some(PreviewShortcutTarget::Edit) => self.open_editor_button.emit_clicked(),
            Some(PreviewShortcutTarget::Delete) => self.delete_button.emit_clicked(),
            Some(PreviewShortcutTarget::Close) => self.close_preview_button.emit_clicked(),
            _ => return false,
        }
        true
    }
}

struct PreviewWindowBuild {
    window: ApplicationWindow,
    title: String,
    floating_geometry: (i32, i32, i32, i32),
    shell: Rc<RefCell<preview::PreviewWindowShell>>,
    controls_revealer: Revealer,
    preview_surface: Frame,
    toast_label: gtk4::Label,
    quick_copy_button: Button,
    quick_save_button: Button,
}

struct PreviewControlsBuild {
    controls_revealer: Revealer,
    quick_copy_button: Button,
    quick_save_button: Button,
}

fn build_preview_quick_action_button(
    label: &str,
    shortcut: char,
    tooltip: &str,
    suggested: bool,
) -> Button {
    let button = Button::new();
    button.add_css_class("preview-quick-action");
    if suggested {
        button.add_css_class("suggested-action");
    }
    button.set_tooltip_text(Some(tooltip));

    let content = GtkBox::new(Orientation::Horizontal, 6);
    let label_widget = gtk4::Label::new(Some(label));
    let shortcut_widget = gtk4::Label::new(Some(&format!("({})", shortcut.to_ascii_uppercase())));
    shortcut_widget.add_css_class("preview-shortcut-hint");
    content.append(&label_widget);
    content.append(&shortcut_widget);
    button.set_child(Some(&content));

    button
}

fn build_preview_controls(context: &PreviewRenderContext) -> PreviewControlsBuild {
    let controls_layout = GtkBox::new(Orientation::Vertical, 0);
    controls_layout.set_hexpand(true);
    controls_layout.set_vexpand(true);
    controls_layout.set_margin_top(context.style_tokens.spacing_16);
    controls_layout.set_margin_bottom(context.style_tokens.spacing_16);
    controls_layout.set_margin_start(context.style_tokens.spacing_16);
    controls_layout.set_margin_end(context.style_tokens.spacing_16);

    let top_controls_wrap = GtkBox::new(Orientation::Horizontal, context.style_tokens.spacing_4);
    top_controls_wrap.set_halign(Align::End);
    top_controls_wrap.set_valign(Align::Start);
    top_controls_wrap.add_css_class("preview-top-controls");
    top_controls_wrap.add_css_class("preview-action-group");

    let quick_save_button = build_preview_quick_action_button(
        "Save",
        's',
        "Save to the screenshot folder and close (S)",
        true,
    );
    let quick_copy_button = build_preview_quick_action_button(
        "Copy",
        'c',
        "Copy to the clipboard and close (C)",
        false,
    );

    top_controls_wrap.append(&quick_save_button);
    top_controls_wrap.append(&quick_copy_button);

    let controls_spacer = GtkBox::new(Orientation::Vertical, 0);
    controls_spacer.set_vexpand(true);

    controls_layout.append(&top_controls_wrap);
    controls_layout.append(&controls_spacer);

    let controls_revealer = Revealer::new();
    controls_revealer.add_css_class("preview-controls-revealer");
    controls_revealer.set_transition_duration(context.motion_hover_ms);
    controls_revealer.set_transition_type(RevealerTransitionType::Crossfade);
    controls_revealer.set_halign(Align::Fill);
    controls_revealer.set_valign(Align::Fill);
    controls_revealer.set_child(Some(&controls_layout));
    controls_revealer.set_reveal_child(true);

    PreviewControlsBuild {
        controls_revealer,
        quick_copy_button,
        quick_save_button,
    }
}

fn build_preview_window(
    context: &PreviewRenderContext,
    artifact: &capture::CaptureArtifact,
) -> PreviewWindowBuild {
    let preview_window_instance = ApplicationWindow::new(&context.app);
    let preview_title = format!("Preview - {}", artifact.capture_id);
    preview_window_instance.set_title(Some(&preview_title));
    preview_window_instance.set_decorated(false);
    preview_window_instance.add_css_class(jjaeng_core::identity::APP_CSS_ROOT);
    preview_window_instance.add_css_class("floating-preview-window");

    let placement = compute_initial_preview_placement(artifact, context.style_tokens);
    let mut preview_shell_model = preview::PreviewWindowShell::with_capture_size(
        artifact.screen_width,
        artifact.screen_height,
    );
    preview_shell_model.set_geometry(placement.geometry);
    let preview_shell = Rc::new(RefCell::new(preview_shell_model));
    let geometry = preview_shell.borrow().geometry();

    preview_window_instance.set_default_size(geometry.width, geometry.height);
    preview_window_instance.set_size_request(geometry.width, geometry.height);
    preview_window_instance.set_resizable(false);

    let preview_overlay = Overlay::new();
    preview_overlay.add_css_class("transparent-bg");
    if !artifact.temp_path.exists() {
        *context.status_log.borrow_mut() = format!(
            "preview image path missing: {}",
            artifact.temp_path.display()
        );
    }

    let preview_controls = build_preview_controls(context);

    let preview_image = gtk4::Picture::for_file(&gtk4::gio::File::for_path(&artifact.temp_path));
    preview_image.set_hexpand(true);
    preview_image.set_vexpand(true);
    preview_image.set_can_shrink(true);
    preview_image.set_keep_aspect_ratio(true);
    let preview_surface = Frame::new(None);
    preview_surface.add_css_class("preview-surface");
    preview_surface.set_hexpand(true);
    preview_surface.set_vexpand(true);
    preview_surface.set_overflow(Overflow::Hidden);
    preview_surface.set_opacity(preview_shell.borrow().transparency() as f64);
    preview_surface.set_child(Some(&preview_image));
    preview_overlay.set_child(Some(&preview_surface));
    preview_overlay.add_overlay(&preview_controls.controls_revealer);

    let preview_toast_anchor = GtkBox::new(Orientation::Vertical, 0);
    preview_toast_anchor.set_halign(Align::End);
    preview_toast_anchor.set_valign(Align::End);
    preview_toast_anchor.set_margin_top(context.style_tokens.spacing_12);
    preview_toast_anchor.set_margin_bottom(context.style_tokens.spacing_12);
    preview_toast_anchor.set_margin_start(context.style_tokens.spacing_12);
    preview_toast_anchor.set_margin_end(context.style_tokens.spacing_12);
    let preview_toast_label = gtk4::Label::new(Some(""));
    preview_toast_label.add_css_class("toast-badge");
    preview_toast_label.set_visible(false);
    preview_toast_anchor.append(&preview_toast_label);
    preview_overlay.add_overlay(&preview_toast_anchor);

    preview_window_instance.set_child(Some(&preview_overlay));
    PreviewWindowBuild {
        window: preview_window_instance,
        title: preview_title,
        floating_geometry: (geometry.x, geometry.y, geometry.width, geometry.height),
        shell: preview_shell,
        controls_revealer: preview_controls.controls_revealer,
        preview_surface,
        toast_label: preview_toast_label,
        quick_copy_button: preview_controls.quick_copy_button,
        quick_save_button: preview_controls.quick_save_button,
    }
}

fn connect_preview_window_action_wiring(
    context: &PreviewRenderContext,
    build: &PreviewWindowBuild,
    capture_id: &str,
) -> Rc<Cell<bool>> {
    connect_preview_action_bridges(
        &[
            (&build.quick_save_button, &context.save_button),
            (&build.quick_copy_button, &context.copy_button),
        ],
        &context.preview_action_target_capture_id,
        capture_id,
    );

    let launchpad_buttons = PreviewLaunchpadButtons::from_context(context);
    {
        let preview_action_target_capture_id = context.preview_action_target_capture_id.clone();
        let capture_id = capture_id.to_string();
        let key_controller = gtk4::EventControllerKey::new();
        key_controller.connect_key_pressed(move |_, key, keycode, modifier| {
            let Some(shortcut_key) = normalize_shortcut_key(key, keycode) else {
                return gtk4::glib::Propagation::Proceed;
            };
            let shortcut = resolve_shortcut(
                shortcut_key,
                shortcut_modifiers(modifier),
                InputContext {
                    mode: InputMode::Preview,
                },
            );
            let Some(action) = shortcut else {
                return gtk4::glib::Propagation::Proceed;
            };

            *preview_action_target_capture_id.borrow_mut() = Some(capture_id.clone());
            if launchpad_buttons.emit_shortcut_action(action) {
                gtk4::glib::Propagation::Stop
            } else {
                gtk4::glib::Propagation::Proceed
            }
        });
        build.window.add_controller(key_controller);
    }

    {
        let open_editor_button = context.open_editor_button.clone();
        let preview_action_target_capture_id = context.preview_action_target_capture_id.clone();
        let capture_id = capture_id.to_string();
        let double_click = GestureClick::new();
        double_click.set_button(1);
        double_click.connect_pressed(move |_, n_press, _, _| {
            if n_press < 2 {
                return;
            }
            *preview_action_target_capture_id.borrow_mut() = Some(capture_id.clone());
            open_editor_button.emit_clicked();
        });
        build.preview_surface.add_controller(double_click);
    }

    let close_guard = Rc::new(Cell::new(false));
    {
        let close_preview_button = context.close_preview_button.clone();
        let preview_action_target_capture_id = context.preview_action_target_capture_id.clone();
        let capture_id = capture_id.to_string();
        let close_guard = close_guard.clone();
        build.window.connect_close_request(move |_| {
            if close_guard.get() {
                return gtk4::glib::Propagation::Proceed;
            }
            *preview_action_target_capture_id.borrow_mut() = Some(capture_id.clone());
            close_preview_button.emit_clicked();
            gtk4::glib::Propagation::Stop
        });
    }

    close_guard
}

fn connect_preview_window_interactions(build: &PreviewWindowBuild) {
    set_revealer_visibility(&build.controls_revealer, true);
}

fn create_preview_window_for_capture(
    context: &PreviewRenderContext,
    artifact: &capture::CaptureArtifact,
) {
    let build = build_preview_window(context, artifact);
    let close_guard = connect_preview_window_action_wiring(context, &build, &artifact.capture_id);
    connect_preview_window_interactions(&build);

    build.window.present();
    request_window_floating_with_geometry(
        "preview",
        &build.title,
        true,
        Some(build.floating_geometry),
        false,
        false,
    );

    context.preview_windows.borrow_mut().insert(
        artifact.capture_id.clone(),
        PreviewWindowRuntime {
            window: build.window,
            shell: build.shell,
            preview_surface: build.preview_surface,
            controls: build.controls_revealer,
            toast: ToastRuntime::new(&build.toast_label),
            close_guard,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_shortcut_target_maps_preview_actions() {
        assert_eq!(
            preview_shortcut_target(ShortcutAction::PreviewSave),
            Some(PreviewShortcutTarget::Save)
        );
        assert_eq!(
            preview_shortcut_target(ShortcutAction::PreviewCopy),
            Some(PreviewShortcutTarget::Copy)
        );
        assert_eq!(
            preview_shortcut_target(ShortcutAction::PreviewEdit),
            Some(PreviewShortcutTarget::Edit)
        );
        assert_eq!(
            preview_shortcut_target(ShortcutAction::PreviewDelete),
            Some(PreviewShortcutTarget::Delete)
        );
        assert_eq!(
            preview_shortcut_target(ShortcutAction::PreviewClose),
            Some(PreviewShortcutTarget::Close)
        );
    }

    #[test]
    fn preview_shortcut_target_ignores_non_preview_actions() {
        assert_eq!(preview_shortcut_target(ShortcutAction::EditorSave), None);
        assert_eq!(preview_shortcut_target(ShortcutAction::DialogConfirm), None);
    }
}
