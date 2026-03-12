use std::process::Command;
use std::time::Duration;

const HYPR_FLOAT_RETRY_COUNT: u8 = 40;
const HYPR_FLOAT_RETRY_DELAY: Duration = Duration::from_millis(50);
#[derive(Debug, Clone)]
pub(super) struct HyprClientMatch {
    pub(super) address: String,
    pub(super) center: Option<(i32, i32)>,
    pub(super) geometry: Option<(i32, i32, i32, i32)>,
}

fn parse_client_geometry(client: &serde_json::Value) -> Option<(i32, i32, i32, i32)> {
    let at = client.get("at")?.as_array()?;
    let size = client.get("size")?.as_array()?;
    if at.len() != 2 || size.len() != 2 {
        return None;
    }
    let x = i32::try_from(at[0].as_i64()?).ok()?;
    let y = i32::try_from(at[1].as_i64()?).ok()?;
    let width = i32::try_from(size[0].as_i64()?).ok()?;
    let height = i32::try_from(size[1].as_i64()?).ok()?;
    if width <= 0 || height <= 0 {
        return None;
    }
    Some((x, y, width, height))
}

fn parse_client_center(client: &serde_json::Value) -> Option<(i32, i32)> {
    let (x, y, width, height) = parse_client_geometry(client)?;
    Some((x.saturating_add(width / 2), y.saturating_add(height / 2)))
}

fn parse_monitor_geometry(monitor: &serde_json::Value) -> Option<(i32, i32, i32, i32)> {
    let x = i32::try_from(monitor.get("x")?.as_i64()?).ok()?;
    let y = i32::try_from(monitor.get("y")?.as_i64()?).ok()?;
    let width = i32::try_from(monitor.get("width")?.as_i64()?).ok()?;
    let height = i32::try_from(monitor.get("height")?.as_i64()?).ok()?;
    if width <= 0 || height <= 0 {
        return None;
    }
    Some((x, y, width, height))
}

fn focused_monitor_geometry_from_json(stdout: &[u8]) -> Option<(i32, i32, i32, i32)> {
    let parsed: serde_json::Value = serde_json::from_slice(stdout).ok()?;
    let monitors = parsed.as_array()?;
    monitors
        .iter()
        .find(|monitor| {
            monitor
                .get("focused")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        })
        .and_then(parse_monitor_geometry)
}

pub(super) fn hypr_client_match_from_json(
    stdout: &[u8],
    expected_title: &str,
) -> Option<HyprClientMatch> {
    let parsed: serde_json::Value = serde_json::from_slice(stdout).ok()?;
    let clients = parsed.as_array()?;
    for client in clients {
        let Some(title) = client.get("title").and_then(serde_json::Value::as_str) else {
            continue;
        };
        if title != expected_title {
            continue;
        }
        let Some(address) = client.get("address").and_then(serde_json::Value::as_str) else {
            continue;
        };
        return Some(HyprClientMatch {
            address: address.to_string(),
            center: parse_client_center(client),
            geometry: parse_client_geometry(client),
        });
    }
    None
}

fn find_hypr_window_match(expected_title: &str) -> Option<HyprClientMatch> {
    let outcome = Command::new("hyprctl")
        .args(["-j", "clients"])
        .output()
        .ok()?;
    if !outcome.status.success() {
        return None;
    }
    hypr_client_match_from_json(&outcome.stdout, expected_title)
}

fn retry_until_some<T, F, S>(
    retry_count: u8,
    retry_delay: Duration,
    mut action: F,
    mut sleep: S,
) -> Option<T>
where
    F: FnMut(u8) -> Option<T>,
    S: FnMut(Duration),
{
    if retry_count == 0 {
        return None;
    }

    for attempt in 1..=retry_count {
        if let Some(value) = action(attempt) {
            return Some(value);
        }

        if attempt < retry_count {
            sleep(retry_delay);
        }
    }

    None
}

fn apply_hypr_window_surface_props(window_name: &str, selector: &str) {
    for (property, value) in [
        ("decorate", "off"),
        ("border_size", "0"),
        ("rounding", "0"),
        ("no_blur", "on"),
        ("no_dim", "on"),
        ("no_shadow", "on"),
    ] {
        let outcome = Command::new("hyprctl")
            .args(["dispatch", "setprop", selector, property, value])
            .output();

        match outcome {
            Ok(result) if result.status.success() => {
                tracing::debug!(
                    window = window_name,
                    selector = selector,
                    property = property,
                    value = value,
                    "hyprctl setprop applied"
                );
            }
            Ok(result) => {
                let stderr = String::from_utf8_lossy(&result.stderr);
                tracing::debug!(
                    window = window_name,
                    selector = selector,
                    property = property,
                    status = result.status.code(),
                    stderr = stderr.trim(),
                    "hyprctl setprop returned non-zero status"
                );
            }
            Err(err) => {
                tracing::debug!(
                    window = window_name,
                    selector = selector,
                    property = property,
                    ?err,
                    "hyprctl setprop failed"
                );
            }
        }
    }
}

pub(super) fn request_window_floating_with_geometry(
    window_name: &str,
    expected_title: &str,
    strip_surface: bool,
    geometry: Option<(i32, i32, i32, i32)>,
) {
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none() {
        tracing::debug!(
            window = window_name,
            "skipping floating dispatch outside Hyprland"
        );
        return;
    }

    let window_name = window_name.to_string();
    let expected_title = expected_title.to_string();
    std::thread::spawn(move || {
        let Some(matched) = retry_until_some(
            HYPR_FLOAT_RETRY_COUNT,
            HYPR_FLOAT_RETRY_DELAY,
            |_| find_hypr_window_match(&expected_title),
            std::thread::sleep,
        ) else {
            tracing::debug!(
                window = window_name,
                title = expected_title,
                "hypr window address lookup failed for floating request"
            );
            return;
        };

        let selector = format!("address:{}", matched.address);
        let outcome = Command::new("hyprctl")
            .args(["dispatch", "setfloating", &selector])
            .output();

        match outcome {
            Ok(result) if result.status.success() => {
                tracing::debug!(
                    window = window_name,
                    selector = selector,
                    title = expected_title,
                    "requested Hyprland floating for exact window"
                );
                if strip_surface {
                    apply_hypr_window_surface_props(&window_name, &selector);
                }
                if let Some((x, y, width, height)) = geometry {
                    let resize_arg =
                        format!("exact {} {},{}", width.max(1), height.max(1), selector);
                    let move_arg = format!("exact {x} {y},{selector}");
                    for (dispatcher, arg) in [
                        ("resizewindowpixel", resize_arg),
                        ("movewindowpixel", move_arg),
                    ] {
                        let outcome = Command::new("hyprctl")
                            .args(["dispatch", dispatcher, &arg])
                            .output();
                        match outcome {
                            Ok(result) if result.status.success() => {
                                tracing::debug!(
                                    window = window_name,
                                    dispatcher = dispatcher,
                                    arg = arg,
                                    "applied window geometry dispatch"
                                );
                            }
                            Ok(result) => {
                                let stderr = String::from_utf8_lossy(&result.stderr);
                                tracing::warn!(
                                    window = window_name,
                                    dispatcher = dispatcher,
                                    arg = arg,
                                    status = result.status.code(),
                                    stderr = stderr.trim(),
                                    "window geometry dispatch returned non-zero status"
                                );
                            }
                            Err(err) => {
                                tracing::debug!(
                                    window = window_name,
                                    dispatcher = dispatcher,
                                    arg = arg,
                                    ?err,
                                    "window geometry dispatch failed"
                                );
                            }
                        }
                    }
                }
            }
            Ok(result) => {
                let stderr = String::from_utf8_lossy(&result.stderr);
                tracing::warn!(
                    window = window_name,
                    selector = selector,
                    status = result.status.code(),
                    stderr = stderr.trim(),
                    "hyprctl setfloating address returned non-zero status"
                );
            }
            Err(err) => {
                tracing::debug!(
                    window = window_name,
                    selector = selector,
                    ?err,
                    "hyprctl setfloating address failed"
                );
            }
        }
    });
}

pub(super) fn current_window_center(expected_title: &str) -> Option<(i32, i32)> {
    std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
    find_hypr_window_match(expected_title).and_then(|matched| matched.center)
}

pub(super) fn focused_monitor_geometry() -> Option<(i32, i32, i32, i32)> {
    std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
    let outcome = Command::new("hyprctl")
        .args(["monitors", "-j"])
        .output()
        .ok()?;
    if !outcome.status.success() {
        return None;
    }
    focused_monitor_geometry_from_json(&outcome.stdout)
}

pub(super) fn focused_monitor_center() -> Option<(i32, i32)> {
    let (x, y, width, height) = focused_monitor_geometry()?;
    Some((x.saturating_add(width / 2), y.saturating_add(height / 2)))
}

pub(super) fn current_window_geometry(expected_title: &str) -> Option<(i32, i32, i32, i32)> {
    std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
    find_hypr_window_match(expected_title).and_then(|matched| matched.geometry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn hypr_client_address_from_json(stdout: &[u8], expected_title: &str) -> Option<String> {
        hypr_client_match_from_json(stdout, expected_title).map(|item| item.address)
    }

    #[test]
    fn hypr_client_address_from_json_matches_exact_title() {
        let payload = br#"
[
  {"address":"0x100","title":"Preview - first"},
  {"address":"0x200","title":"Preview - second"}
]
"#;
        let address = hypr_client_address_from_json(payload, "Preview - second");
        assert_eq!(address.as_deref(), Some("0x200"));
    }

    #[test]
    fn hypr_client_address_from_json_ignores_non_object_entries() {
        let payload = br#"
[
  "ok",
  {"address":"0x300","title":"Preview - stable"}
]
"#;
        let address = hypr_client_address_from_json(payload, "Preview - stable");
        assert_eq!(address.as_deref(), Some("0x300"));
    }

    #[test]
    fn hypr_client_match_from_json_parses_center_when_available() {
        let stdout = br#"[
            {"title":"Preview - a","address":"0x1","pinned":false,"at":[100,200],"size":[600,400]}
        ]"#;
        let item = hypr_client_match_from_json(stdout, "Preview - a").expect("match");
        assert_eq!(item.address, "0x1");
        assert_eq!(item.center, Some((400, 400)));
        assert_eq!(item.geometry, Some((100, 200, 600, 400)));
    }

    #[test]
    fn focused_monitor_geometry_from_json_parses_focused_monitor() {
        let stdout = br#"[
            {"name":"DP-1","focused":false,"x":0,"y":0,"width":1920,"height":1080},
            {"name":"DP-2","focused":true,"x":1920,"y":0,"width":2560,"height":1440}
        ]"#;

        assert_eq!(
            focused_monitor_geometry_from_json(stdout),
            Some((1920, 0, 2560, 1440))
        );
    }

    #[test]
    fn focused_monitor_geometry_from_json_rejects_invalid_sizes() {
        let stdout = br#"[
            {"name":"DP-1","focused":true,"x":0,"y":0,"width":0,"height":1440}
        ]"#;

        assert_eq!(focused_monitor_geometry_from_json(stdout), None);
    }

    #[test]
    fn retry_until_some_returns_value_without_extra_retries() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let sleeps = Rc::new(RefCell::new(Vec::new()));

        let result = retry_until_some(
            5,
            Duration::from_millis(10),
            {
                let calls = calls.clone();
                move |attempt| {
                    calls.borrow_mut().push(attempt);
                    (attempt == 3).then_some("matched")
                }
            },
            {
                let sleeps = sleeps.clone();
                move |duration| sleeps.borrow_mut().push(duration)
            },
        );

        assert_eq!(result, Some("matched"));
        assert_eq!(*calls.borrow(), vec![1, 2, 3]);
        assert_eq!(
            *sleeps.borrow(),
            vec![Duration::from_millis(10), Duration::from_millis(10)]
        );
    }

    #[test]
    fn retry_until_some_stops_after_max_attempts() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let sleeps = Rc::new(RefCell::new(Vec::new()));

        let result = retry_until_some(
            4,
            Duration::from_millis(5),
            {
                let calls = calls.clone();
                move |attempt| {
                    calls.borrow_mut().push(attempt);
                    None::<u8>
                }
            },
            {
                let sleeps = sleeps.clone();
                move |duration| sleeps.borrow_mut().push(duration)
            },
        );

        assert_eq!(result, None);
        assert_eq!(*calls.borrow(), vec![1, 2, 3, 4]);
        assert_eq!(
            *sleeps.borrow(),
            vec![
                Duration::from_millis(5),
                Duration::from_millis(5),
                Duration::from_millis(5)
            ]
        );
    }
}
