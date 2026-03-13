use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::ui::{icon_button, StyleTokens};
use gtk4::gdk::Key;
use gtk4::prelude::*;
use gtk4::{
    Align, Application, ApplicationWindow, Box as GtkBox, Button, EventControllerKey, Frame, Label,
    Orientation, Overflow, Overlay, Picture,
};
use jjaeng_core::identity::APP_CSS_ROOT;

use super::hypr::{focused_monitor_center, request_window_floating_with_geometry};
use super::layout::compute_media_preview_geometry_for_point;

const RECORDING_RESULT_TITLE: &str = "Jjaeng Recording Preview";

#[derive(Clone)]
pub(super) struct RecordingResultArtifact {
    pub(super) output_path: PathBuf,
    pub(super) thumbnail_path: PathBuf,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) duration_ms: u64,
    pub(super) saved_path: Option<PathBuf>,
    pub(super) cleanup_output_on_close: bool,
    pub(super) cleanup_thumbnail_on_close: bool,
}

#[derive(Clone)]
pub(super) struct RecordingResultRuntime {
    window: ApplicationWindow,
    close_guard: Rc<Cell<bool>>,
    save_button: Button,
    output_path: PathBuf,
    thumbnail_path: PathBuf,
    cleanup_output_on_close: Rc<Cell<bool>>,
    cleanup_thumbnail_on_close: Rc<Cell<bool>>,
}

impl RecordingResultRuntime {
    fn close(self) {
        self.close_guard.set(true);
        self.window.close();
        if self.cleanup_output_on_close.get() {
            let _ = std::fs::remove_file(&self.output_path);
        }
        if self.cleanup_thumbnail_on_close.get() {
            let _ = std::fs::remove_file(&self.thumbnail_path);
        }
    }

    fn mark_cleanup(&self, cleanup_output: bool, cleanup_thumbnail: bool) {
        if cleanup_output {
            self.cleanup_output_on_close.set(true);
        }
        if cleanup_thumbnail {
            self.cleanup_thumbnail_on_close.set(true);
        }
    }

    fn set_saved_path(&self, saved_path: &Path, cleanup_output: bool, cleanup_thumbnail: bool) {
        let saved_path_text = saved_path.to_string_lossy().into_owned();
        self.window.set_tooltip_text(Some(&saved_path_text));
        self.save_button.set_sensitive(false);
        self.mark_cleanup(cleanup_output, cleanup_thumbnail);
    }
}

pub(super) fn recording_result_open(
    recording_result: &Rc<RefCell<Option<RecordingResultRuntime>>>,
) -> bool {
    recording_result.borrow().is_some()
}

pub(super) fn dismiss_recording_result(
    recording_result: &Rc<RefCell<Option<RecordingResultRuntime>>>,
) {
    if let Some(runtime) = recording_result.borrow_mut().take() {
        runtime.close();
    }
}

pub(super) fn set_recording_result_saved_path(
    recording_result: &Rc<RefCell<Option<RecordingResultRuntime>>>,
    saved_path: &Path,
    cleanup_output: bool,
    cleanup_thumbnail: bool,
) {
    if let Some(runtime) = recording_result.borrow().as_ref() {
        runtime.set_saved_path(saved_path, cleanup_output, cleanup_thumbnail);
    }
}

pub(super) fn present_recording_result(
    app: &Application,
    style_tokens: StyleTokens,
    recording_result: &Rc<RefCell<Option<RecordingResultRuntime>>>,
    artifact: &RecordingResultArtifact,
    on_save: &Rc<dyn Fn()>,
    on_copy: &Rc<dyn Fn()>,
    on_open: &Rc<dyn Fn()>,
    on_close: &Rc<dyn Fn()>,
) {
    dismiss_recording_result(recording_result);

    let (anchor_x, anchor_y) = focused_monitor_center().unwrap_or((0, 0));
    let geometry = compute_media_preview_geometry_for_point(
        artifact.width,
        artifact.height,
        anchor_x,
        anchor_y,
        style_tokens,
    );

    let window = ApplicationWindow::new(app);
    window.set_title(Some(RECORDING_RESULT_TITLE));
    window.set_decorated(false);
    window.set_resizable(false);
    window.add_css_class(APP_CSS_ROOT);
    window.add_css_class("floating-preview-window");
    window.add_css_class("recording-result-window");
    let initial_display_path = artifact
        .saved_path
        .as_ref()
        .unwrap_or(&artifact.output_path)
        .to_string_lossy()
        .into_owned();
    window.set_tooltip_text(Some(&initial_display_path));

    let root = Overlay::new();
    root.add_css_class("transparent-bg");

    let thumbnail_frame = Frame::new(None);
    thumbnail_frame.add_css_class("preview-surface");
    thumbnail_frame.add_css_class("recording-result-thumbnail-frame");
    thumbnail_frame.set_hexpand(true);
    thumbnail_frame.set_vexpand(true);
    thumbnail_frame.set_overflow(Overflow::Hidden);
    let thumbnail = Picture::for_file(&gtk4::gio::File::for_path(&artifact.thumbnail_path));
    thumbnail.set_keep_aspect_ratio(true);
    thumbnail.set_can_shrink(true);
    thumbnail.set_hexpand(true);
    thumbnail.set_vexpand(true);
    thumbnail_frame.set_child(Some(&thumbnail));
    root.set_child(Some(&thumbnail_frame));

    let button_row = GtkBox::new(Orientation::Horizontal, style_tokens.spacing_4);
    button_row.add_css_class("preview-top-controls");
    button_row.add_css_class("preview-action-group");
    button_row.add_css_class("recording-result-button-row");
    button_row.set_halign(Align::End);
    button_row.set_valign(Align::Start);
    button_row.set_margin_top(style_tokens.spacing_12);
    button_row.set_margin_end(style_tokens.spacing_12);

    let save_button = icon_button(
        "save-symbolic",
        "Save recording",
        style_tokens.control_size as i32,
        &["recording-bar-action", "recording-bar-record"],
    );
    save_button.set_sensitive(artifact.saved_path.is_none());
    let copy_button = icon_button(
        "copy-symbolic",
        "Copy recording path",
        style_tokens.control_size as i32,
        &["recording-bar-action"],
    );
    let open_button = icon_button(
        "arrow-up-right-symbolic",
        "Open recording",
        style_tokens.control_size as i32,
        &["recording-bar-action"],
    );
    let close_button = icon_button(
        "x-symbolic",
        "Close recording preview",
        style_tokens.control_size as i32,
        &["recording-bar-action", "recording-bar-stop"],
    );

    button_row.append(&save_button);
    button_row.append(&copy_button);
    button_row.append(&open_button);
    button_row.append(&close_button);
    root.add_overlay(&button_row);

    let meta_anchor = GtkBox::new(Orientation::Vertical, 0);
    meta_anchor.set_halign(Align::Start);
    meta_anchor.set_valign(Align::End);
    meta_anchor.set_margin_bottom(style_tokens.spacing_12);
    meta_anchor.set_margin_start(style_tokens.spacing_12);
    let meta_label = Label::new(Some(&format_recording_result_meta(artifact)));
    meta_label.add_css_class("toast-badge");
    meta_anchor.append(&meta_label);
    root.add_overlay(&meta_anchor);

    window.set_child(Some(&root));
    window.set_default_size(geometry.width, geometry.height);
    window.set_size_request(geometry.width, geometry.height);

    let close_guard = Rc::new(Cell::new(false));

    {
        let close_guard = close_guard.clone();
        let on_close = on_close.clone();
        window.connect_close_request(move |_| {
            if close_guard.get() {
                return gtk4::glib::Propagation::Proceed;
            }
            (on_close.as_ref())();
            gtk4::glib::Propagation::Stop
        });
    }

    {
        let on_save = on_save.clone();
        save_button.connect_clicked(move |_| {
            (on_save.as_ref())();
        });
    }
    {
        let on_copy = on_copy.clone();
        copy_button.connect_clicked(move |_| {
            (on_copy.as_ref())();
        });
    }
    {
        let on_open = on_open.clone();
        open_button.connect_clicked(move |_| {
            (on_open.as_ref())();
        });
    }
    {
        let on_close = on_close.clone();
        close_button.connect_clicked(move |_| {
            (on_close.as_ref())();
        });
    }
    {
        let on_save = on_save.clone();
        let on_copy = on_copy.clone();
        let on_open = on_open.clone();
        let on_close = on_close.clone();
        let save_button = save_button.clone();
        let key_controller = EventControllerKey::new();
        key_controller.connect_key_pressed(move |_, key, _, _| {
            match key.to_unicode().map(|value| value.to_ascii_lowercase()) {
                Some('s') => {
                    if save_button.is_sensitive() {
                        (on_save.as_ref())();
                    }
                    gtk4::glib::Propagation::Stop
                }
                Some('c') => {
                    (on_copy.as_ref())();
                    gtk4::glib::Propagation::Stop
                }
                Some('o') => {
                    (on_open.as_ref())();
                    gtk4::glib::Propagation::Stop
                }
                _ if key == Key::Escape => {
                    (on_close.as_ref())();
                    gtk4::glib::Propagation::Stop
                }
                _ => gtk4::glib::Propagation::Proceed,
            }
        });
        window.add_controller(key_controller);
    }

    window.present();
    request_window_floating_with_geometry(
        "recording-result",
        RECORDING_RESULT_TITLE,
        true,
        Some((geometry.x, geometry.y, geometry.width, geometry.height)),
        false,
        true,
        true,
    );

    recording_result
        .borrow_mut()
        .replace(RecordingResultRuntime {
            window,
            close_guard,
            save_button,
            output_path: artifact.output_path.clone(),
            thumbnail_path: artifact.thumbnail_path.clone(),
            cleanup_output_on_close: Rc::new(Cell::new(artifact.cleanup_output_on_close)),
            cleanup_thumbnail_on_close: Rc::new(Cell::new(artifact.cleanup_thumbnail_on_close)),
        });
}

fn format_recording_result_meta(artifact: &RecordingResultArtifact) -> String {
    let total_seconds = artifact.duration_ms / 1000;
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    format!(
        "{minutes:02}:{seconds:02} · {} x {}",
        artifact.width, artifact.height
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_recording_result_meta_uses_duration_and_geometry() {
        let artifact = RecordingResultArtifact {
            output_path: PathBuf::from("/tmp/recording-1.mp4"),
            thumbnail_path: PathBuf::from("/tmp/recording-1.png"),
            width: 1920,
            height: 1080,
            duration_ms: 72_300,
            saved_path: None,
            cleanup_output_on_close: false,
            cleanup_thumbnail_on_close: false,
        };

        assert_eq!(
            format_recording_result_meta(&artifact),
            "01:12 · 1920 x 1080"
        );
    }
}
