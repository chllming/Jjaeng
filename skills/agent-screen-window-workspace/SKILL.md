---
name: agent-screen-window-workspace
description: Resolve Hyprland windows and visible workspaces for precise capture targets.
---

# Agent Screen window/workspace resolution

Use `list_windows` to resolve address, title, class, workspace, monitor, and
geometry. Prefer `screenshot_window` with the returned address. A workspace
capture means the workspace currently visible on a monitor; do not silently
switch to hidden workspaces.

Geometry capture can include occluding windows. Describe this limitation when
the request needs a true native Wayland window recording.
