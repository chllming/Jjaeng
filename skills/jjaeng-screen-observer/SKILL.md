---
name: jjaeng-screen-observer
description: Inspect the current Hyprland monitor, workspace, and window state, then capture a focused screenshot without changing desktop state.
---

# Jjaeng screen observer

Use `status`, `list_monitors`, `list_workspaces`, `list_windows`, and
`active_window` before choosing a capture target. Prefer a stable Hyprland
window address over title matching. Do not switch workspaces or invoke desktop
control while observing.

Return the artifact ID, timestamp, target, dimensions when available, and any
limitations caused by Wayland visibility or occlusion.
