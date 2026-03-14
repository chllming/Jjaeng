use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use crate::ui::StyleTokens;
use gtk4::prelude::*;
use gtk4::{
    Align, Application, ApplicationWindow, Box as GtkBox, Button, Frame, GestureClick, Orientation,
    Overflow, Overlay, Revealer, RevealerTransitionType,
};
use jjaeng_core::capture;
use jjaeng_core::input::{resolve_shortcut, InputContext, InputMode, ShortcutAction};
use jjaeng_core::preview::{self, PreviewAction};

use super::hover_controls::set_revealer_visibility;
use super::hypr::request_window_floating_with_geometry;
use super::input_bridge::{normalize_shortcut_key, shortcut_modifiers};
use super::launchpad_actions::LaunchpadActionExecutor;
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
    preview_windows: Rc<RefCell<HashMap<String, PreviewWindowRuntime>>>,
    preview_action_target_capture_id: Rc<RefCell<Option<String>>>,
    launchpad_actions: LaunchpadActionExecutor,
    render_handle: Rc<RefCell<Option<Rc<dyn Fn()>>>>,
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
        preview_windows: Rc<RefCell<HashMap<String, PreviewWindowRuntime>>>,
        preview_action_target_capture_id: Rc<RefCell<Option<String>>>,
        launchpad_actions: LaunchpadActionExecutor,
        render_handle: Rc<RefCell<Option<Rc<dyn Fn()>>>>,
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
            preview_windows,
            preview_action_target_capture_id,
            launchpad_actions,
            render_handle,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreviewShortcutTarget {
    Save,
    Copy,
    Ocr,
    Edit,
    Delete,
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PreviewPrimaryClickOutcome {
    Focus,
    Edit,
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

fn preview_primary_click_outcome(n_press: i32) -> PreviewPrimaryClickOutcome {
    if n_press >= 2 {
        PreviewPrimaryClickOutcome::Edit
    } else {
        PreviewPrimaryClickOutcome::Focus
    }
}

fn request_preview_render(context: &PreviewRenderContext) {
    let render = context.render_handle.borrow().as_ref().cloned();
    if let Some(render) = render {
        (render.as_ref())();
    }
}

fn select_preview_capture(context: &PreviewRenderContext, capture_id: &str) {
    *context.preview_action_target_capture_id.borrow_mut() = Some(capture_id.to_string());
}

fn run_preview_action(context: &PreviewRenderContext, capture_id: &str, action: PreviewAction) {
    select_preview_capture(context, capture_id);
    match action {
        PreviewAction::Save | PreviewAction::Copy => {
            let launchpad_actions = context.launchpad_actions.clone();
            let context = context.clone();
            launchpad_actions.run_preview_action_async(action, move || {
                request_preview_render(&context);
            });
        }
        PreviewAction::Edit => {
            context.launchpad_actions.open_editor();
            request_preview_render(context);
        }
        PreviewAction::Delete => {
            let launchpad_actions = context.launchpad_actions.clone();
            let context = context.clone();
            launchpad_actions.delete_active_capture_async(move || {
                request_preview_render(&context);
            });
        }
        PreviewAction::Close => {
            context.launchpad_actions.close_preview();
            request_preview_render(context);
        }
    }
}

fn run_preview_ocr_action(context: &PreviewRenderContext, capture_id: &str) {
    if !context.ocr_available {
        return;
    }
    select_preview_capture(context, capture_id);
    context.launchpad_actions.run_preview_ocr_action();
    request_preview_render(context);
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
    let controls_layout = GtkBox::new(Orientation::Horizontal, context.style_tokens.spacing_4);
    controls_layout.set_halign(Align::End);
    controls_layout.set_valign(Align::Start);
    controls_layout.set_margin_top(context.style_tokens.spacing_16);
    controls_layout.set_margin_end(context.style_tokens.spacing_16);
    controls_layout.add_css_class("preview-top-controls");
    controls_layout.add_css_class("preview-action-group");

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

    controls_layout.append(&quick_save_button);
    controls_layout.append(&quick_copy_button);

    let controls_revealer = Revealer::new();
    controls_revealer.add_css_class("preview-controls-revealer");
    controls_revealer.set_transition_duration(context.motion_hover_ms);
    controls_revealer.set_transition_type(RevealerTransitionType::Crossfade);
    controls_revealer.set_halign(Align::End);
    controls_revealer.set_valign(Align::Start);
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
    preview_window_instance.set_focusable(true);

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
    preview_surface.set_focusable(true);
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
    {
        let context = context.clone();
        let capture_id = capture_id.to_string();
        build.quick_save_button.connect_clicked(move |_| {
            run_preview_action(&context, &capture_id, PreviewAction::Save);
        });
    }
    {
        let context = context.clone();
        let capture_id = capture_id.to_string();
        build.quick_copy_button.connect_clicked(move |_| {
            run_preview_action(&context, &capture_id, PreviewAction::Copy);
        });
    }

    {
        let context = context.clone();
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
            match preview_shortcut_target(action) {
                Some(PreviewShortcutTarget::Save) => {
                    run_preview_action(&context, &capture_id, PreviewAction::Save);
                    gtk4::glib::Propagation::Stop
                }
                Some(PreviewShortcutTarget::Copy) => {
                    run_preview_action(&context, &capture_id, PreviewAction::Copy);
                    gtk4::glib::Propagation::Stop
                }
                Some(PreviewShortcutTarget::Ocr) => {
                    if context.ocr_available {
                        run_preview_ocr_action(&context, &capture_id);
                        gtk4::glib::Propagation::Stop
                    } else {
                        gtk4::glib::Propagation::Proceed
                    }
                }
                Some(PreviewShortcutTarget::Edit) => {
                    run_preview_action(&context, &capture_id, PreviewAction::Edit);
                    gtk4::glib::Propagation::Stop
                }
                Some(PreviewShortcutTarget::Delete) => {
                    run_preview_action(&context, &capture_id, PreviewAction::Delete);
                    gtk4::glib::Propagation::Stop
                }
                Some(PreviewShortcutTarget::Close) => {
                    run_preview_action(&context, &capture_id, PreviewAction::Close);
                    gtk4::glib::Propagation::Stop
                }
                None => gtk4::glib::Propagation::Proceed,
            }
        });
        build.window.add_controller(key_controller);
    }

    {
        let preview_surface = build.preview_surface.clone();
        let window = build.window.clone();
        let context = context.clone();
        let capture_id = capture_id.to_string();
        let primary_click = GestureClick::new();
        primary_click.set_button(1);
        primary_click.connect_pressed(move |_, n_press, _, _| {
            window.present();
            preview_surface.grab_focus();
            if preview_primary_click_outcome(n_press) == PreviewPrimaryClickOutcome::Edit {
                run_preview_action(&context, &capture_id, PreviewAction::Edit);
            }
        });
        build.preview_surface.add_controller(primary_click);
    }

    let close_guard = Rc::new(Cell::new(false));
    {
        let context = context.clone();
        let capture_id = capture_id.to_string();
        let close_guard = close_guard.clone();
        build.window.connect_close_request(move |_| {
            if close_guard.get() {
                return gtk4::glib::Propagation::Proceed;
            }
            run_preview_action(&context, &capture_id, PreviewAction::Close);
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
    if !artifact.temp_path.exists() {
        *context.status_log.borrow_mut() = format!(
            "preview skipped: image file missing at {}",
            artifact.temp_path.display()
        );
        tracing::warn!(
            capture_id = %artifact.capture_id,
            path = %artifact.temp_path.display(),
            "skipping preview window creation for missing temp file"
        );
        return;
    }
    let build = build_preview_window(context, artifact);
    let close_guard = connect_preview_window_action_wiring(context, &build, &artifact.capture_id);
    connect_preview_window_interactions(&build);

    build.window.present();
    {
        let preview_surface = build.preview_surface.clone();
        build.window.connect_is_active_notify(move |window| {
            if window.is_active() {
                preview_surface.grab_focus();
            }
        });
    }
    {
        let preview_surface = build.preview_surface.clone();
        gtk4::glib::timeout_add_local_once(Duration::from_millis(1), move || {
            preview_surface.grab_focus();
        });
    }
    request_window_floating_with_geometry(
        "preview",
        &build.title,
        true,
        Some(build.floating_geometry),
        false,
        false,
        true,
    );

    context.preview_windows.borrow_mut().insert(
        artifact.capture_id.clone(),
        PreviewWindowRuntime {
            window: build.window,
            selection_outline_window: None,
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

    #[test]
    fn preview_primary_click_outcome_focuses_on_single_click() {
        assert_eq!(
            preview_primary_click_outcome(1),
            PreviewPrimaryClickOutcome::Focus
        );
    }

    #[test]
    fn preview_primary_click_outcome_opens_editor_on_double_click() {
        assert_eq!(
            preview_primary_click_outcome(2),
            PreviewPrimaryClickOutcome::Edit
        );
        assert_eq!(
            preview_primary_click_outcome(3),
            PreviewPrimaryClickOutcome::Edit
        );
    }
}
