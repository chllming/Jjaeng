use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::ui::StyleTokens;
use gtk4::prelude::*;
use gtk4::{
    Align, Application, ApplicationWindow, Box as GtkBox, Button, Frame, Label, Orientation,
    Picture,
};
use jjaeng_core::identity::APP_CSS_ROOT;

use super::hypr::{focused_monitor_center, request_window_floating_with_geometry};
use super::layout::bottom_centered_window_geometry_for_point;
use super::window_state::RuntimeWindowGeometry;

const RECORDING_RESULT_TITLE: &str = "Jjaeng Recording Preview";
const RECORDING_RESULT_WIDTH: i32 = 448;
const RECORDING_RESULT_HEIGHT: i32 = 368;

#[derive(Clone)]
pub(super) struct RecordingResultArtifact {
    pub(super) recording_id: String,
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
    path_label: Label,
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
        self.path_label
            .set_text(saved_path.to_string_lossy().as_ref());
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

    let window = ApplicationWindow::new(app);
    window.set_title(Some(RECORDING_RESULT_TITLE));
    window.set_decorated(false);
    window.set_resizable(false);
    window.add_css_class(APP_CSS_ROOT);
    window.add_css_class("recording-result-window");

    let root = GtkBox::new(Orientation::Vertical, style_tokens.spacing_12);
    root.add_css_class("recording-result-surface");
    root.set_margin_top(style_tokens.spacing_12);
    root.set_margin_bottom(style_tokens.spacing_12);
    root.set_margin_start(style_tokens.spacing_12);
    root.set_margin_end(style_tokens.spacing_12);

    let title_label = Label::new(Some("Recording Finished"));
    title_label.add_css_class("recording-result-title");
    title_label.set_halign(Align::Start);
    title_label.set_xalign(0.0);

    let meta_label = Label::new(Some(&format_recording_result_meta(artifact)));
    meta_label.add_css_class("recording-result-meta");
    meta_label.set_halign(Align::Start);
    meta_label.set_xalign(0.0);

    let path_label = Label::new(Some(
        artifact
            .saved_path
            .as_ref()
            .unwrap_or(&artifact.output_path)
            .to_string_lossy()
            .as_ref(),
    ));
    path_label.add_css_class("recording-result-path");
    path_label.set_halign(Align::Start);
    path_label.set_xalign(0.0);
    path_label.set_wrap(true);
    path_label.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
    path_label.set_selectable(true);

    let thumbnail_frame = Frame::new(None);
    thumbnail_frame.add_css_class("recording-result-thumbnail-frame");
    thumbnail_frame.set_size_request(400, 225);
    let thumbnail = Picture::for_file(&gtk4::gio::File::for_path(&artifact.thumbnail_path));
    thumbnail.set_keep_aspect_ratio(true);
    thumbnail.set_can_shrink(true);
    thumbnail.set_hexpand(true);
    thumbnail.set_vexpand(true);
    thumbnail_frame.set_child(Some(&thumbnail));

    let button_row = GtkBox::new(Orientation::Horizontal, style_tokens.spacing_8);
    button_row.add_css_class("recording-result-button-row");
    button_row.set_homogeneous(true);

    let save_button = Button::with_label("Save");
    save_button.add_css_class("recording-prompt-button");
    save_button.add_css_class("recording-prompt-button-primary");
    save_button.set_sensitive(artifact.saved_path.is_none());
    let copy_button = Button::with_label("Copy Path");
    copy_button.add_css_class("recording-prompt-button");
    let open_button = Button::with_label("Open");
    open_button.add_css_class("recording-prompt-button");
    let close_button = Button::with_label("Close");
    close_button.add_css_class("recording-prompt-button");
    close_button.add_css_class("recording-prompt-button-danger");

    button_row.append(&save_button);
    button_row.append(&copy_button);
    button_row.append(&open_button);
    button_row.append(&close_button);

    root.append(&title_label);
    root.append(&meta_label);
    root.append(&thumbnail_frame);
    root.append(&path_label);
    root.append(&button_row);
    window.set_child(Some(&root));
    window.set_default_size(RECORDING_RESULT_WIDTH, RECORDING_RESULT_HEIGHT);
    window.set_size_request(RECORDING_RESULT_WIDTH, RECORDING_RESULT_HEIGHT);

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

    window.present();
    let (anchor_x, anchor_y) = focused_monitor_center().unwrap_or((0, 0));
    let window_geometry = bottom_centered_window_geometry_for_point(
        anchor_x,
        anchor_y,
        RuntimeWindowGeometry::new(RECORDING_RESULT_WIDTH, RECORDING_RESULT_HEIGHT),
        style_tokens.spacing_24,
    );
    request_window_floating_with_geometry(
        "recording-result",
        RECORDING_RESULT_TITLE,
        true,
        Some(window_geometry),
        false,
        true,
        true,
    );

    recording_result
        .borrow_mut()
        .replace(RecordingResultRuntime {
            window,
            close_guard,
            path_label,
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
        "{} · {:02}:{:02} · {} x {}",
        artifact.recording_id, minutes, seconds, artifact.width, artifact.height
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_recording_result_meta_uses_duration_and_geometry() {
        let artifact = RecordingResultArtifact {
            recording_id: "recording-1".to_string(),
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
            "recording-1 · 01:12 · 1920 x 1080"
        );
    }
}
