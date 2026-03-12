# Jjaeng Refactor Plan

## Goal

Refactor Jjaeng from a preview-first GTK app into a background screenshot service with an optional UI layer that fits Omarchy's Hyprland + Waybar desktop model.

The target behavior is:

- `Super+F4` triggers region capture immediately
- capture does not require the full app window to appear
- status line integration exposes the latest capture/session state
- compact preview is small, fixed, and anchored bottom-left
- double-clicking the compact preview opens the full editor
- compact preview exposes only `Save` and `Copy`
- clicking `Save` saves to the designated screenshot folder and closes the preview
- clicking `Copy` copies to the clipboard and closes the preview

## Architecture

Split the application into four parts:

1. `jjaeng-core`
- domain logic
- capture/session lifecycle
- storage policy
- clipboard/save actions
- config
- IPC types

2. `jjaengd`
- long-running background daemon
- owns capture sessions
- handles IPC
- publishes status for bar integration

3. `jjaengctl`
- CLI client
- sends commands like `capture-region`, `show-latest`, `copy-latest`, `save-latest`

4. `jjaeng-ui-gtk`
- preview/editor frontend
- opens only when policy or user action requires it

## UX Model

### Compact preview

Compact preview replaces the current multi-action floating preview window.

Behavior:

- fixed small size
- anchored bottom-left of the active monitor
- always opens in the same position unless explicitly made configurable later
- single capture image surface
- double-click on image opens editor
- action buttons: `Save`, `Copy`

Action semantics:

- `Save`
  - save image to configured screenshot directory
  - close preview
  - mark session complete

- `Copy`
  - copy image to clipboard
  - close preview
  - mark session complete

There should be no edit, OCR, opacity, pin, or inline close controls in compact mode.

### Editor

Editor is secondary and explicit.

Entry points:

- double-click compact preview
- `jjaengctl edit-latest`
- optional status-line click action

Editor remains the richer annotation surface and should not be opened automatically after every capture.

### Status line behavior

Primary plan: integrate with Waybar via a custom module backed by daemon state.

Status line should show:

- idle
- capture pending
- latest capture available
- error state

Click actions should support:

- open latest preview
- open editor
- optionally trigger a new capture

## Omarchy / Hyprland fit

Omarchy uses Hyprland with Waybar and expects user-level customization via `~/.config`.

That means:

- keep Omarchy-specific wiring outside core logic
- avoid patching Omarchy-managed defaults
- provide user override examples for Hyprland and Waybar

Recommended integration order:

1. Waybar custom module
2. optional StatusNotifierItem support later if needed

Do not start with tray/AppIndicator work. A Waybar custom module is a better first fit for Omarchy.

## Refactor Stages

### Stage 1: Extract core

Create `jjaeng-core` and move these concerns out of the GTK app:

- capture artifacts
- storage service
- clipboard/save operations
- session state
- action/event types
- configuration

Result:

- UI stops owning the business logic
- daemon and CLI can share the same core

### Stage 2: Introduce daemon + IPC

Build `jjaengd` around a local Unix socket.

Core commands:

- `capture-region`
- `capture-window`
- `capture-full`
- `show-latest`
- `edit-latest`
- `save-latest`
- `copy-latest`
- `dismiss-latest`
- `list-sessions`

Core events/state:

- session created
- preview available
- editor open
- saved
- copied
- dismissed
- error

### Stage 3: Convert CLI to client mode

Keep compatibility with the current CLI shape where practical:

- `jjaeng --region`
- `jjaeng --window`
- `jjaeng --full`

These should become thin client commands to the daemon when it is running.

### Stage 4: Rebuild preview as compact mode

Replace current preview behavior with a purpose-built compact preview component.

Implementation changes:

- remove current adaptive placement policy for compact mode
- replace remembered preview geometry with fixed policy for compact mode
- use bottom-left anchor on active monitor
- shrink default size to a compact card
- remove all controls except `Save` and `Copy`
- add double-click gesture to open editor

State behavior:

- on `Save`, persist to screenshot folder and destroy preview session
- on `Copy`, send to clipboard and destroy preview session

### Stage 5: Separate editor route

Keep editor as a standalone GTK window flow.

Editor opens only from:

- preview double-click
- explicit command
- explicit bar action

### Stage 6: Waybar status integration

Add a small status publisher from the daemon.

Recommended design:

- daemon writes JSON state
- Waybar custom module reads it
- click handlers call `jjaengctl`

Example states:

- `idle`
- `capture-ready`
- `has-latest`
- `editing`
- `error`

### Stage 7: Optional tray support

If Waybar custom module is insufficient, add freedesktop `StatusNotifierItem` support later.

This is optional and should be deferred because it adds complexity without being necessary for the first useful Omarchy integration.

## Suggested crate layout

```text
crates/
  jjaeng-core/
  jjaeng-ipc/
  jjaeng-daemon/
  jjaeng-ui-gtk/
  jjaeng-cli/
```

Or as a simpler first pass:

```text
src/core/
src/daemon/
src/ui/
src/cli/
```

## Configuration

Add config fields for the new model:

- `screenshot_dir`
- `auto_open_preview`
- `compact_preview.anchor`
- `compact_preview.width`
- `compact_preview.height`
- `compact_preview.actions = ["save", "copy"]`
- `compact_preview.double_click_action = "open-editor"`
- `status_integration = "waybar-custom"`

Recommended defaults:

- `auto_open_preview = on_capture`
- `compact_preview.anchor = bottom-left`
- small fixed width/height
- compact preview closes after `Save` or `Copy`

## Testing plan

### Unit tests

- session state transitions
- save/copy completion semantics
- compact preview policy selection
- screenshot directory resolution
- IPC command parsing/serialization

### Integration tests

- daemon/client roundtrip
- region capture creates a preview session
- `Save` writes to target directory and closes preview
- `Copy` writes to clipboard and closes preview
- double-click preview transitions to editor state

### Manual Hyprland validation

- `Super+F4` region capture
- preview appears bottom-left and small
- `Save` stores screenshot and closes preview
- `Copy` updates clipboard and closes preview
- double-click opens editor
- Waybar status reflects idle/active state correctly

## Implementation order

1. extract `jjaeng-core`
2. add IPC and daemon
3. convert current CLI flags into daemon clients
4. implement compact preview component
5. wire double-click to editor
6. implement `Save`/`Copy` close semantics
7. add Waybar custom module integration
8. evaluate optional tray support later

## Non-goals for the first refactor

- native tray/AppIndicator support
- custom layer-shell bar
- preserving current preview feature density in compact mode
- automatic editor launch on every capture

## Recommendation

Treat Jjaeng as a background screenshot service with a compact action preview and a separate editor, not as a normal windowed app that is later hidden into the status bar.

For Omarchy, the best first-class result is:

- daemon-backed capture flow
- compact bottom-left preview
- `Save` and `Copy` as terminal actions
- double-click to edit
- Waybar custom module for status/control

