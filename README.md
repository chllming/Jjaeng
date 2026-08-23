# Jjaeng

English | [한국어](README.ko.md)

Jjaeng is a Hyprland-first screenshot and recording tool for Wayland with a background daemon, a lightly transparent preview anchored to the lower-left corner, screenshot/video history, Omarchy-aligned flat surfaces, and a built-in annotation editor.

The name "Jjaeng" is a nod to something vivid, sharp, and bright, while the project itself grows out of the original [ChalKak](https://github.com/BitYoungjae/ChalKak). This repository keeps the upstream licensing model and includes attribution in [NOTICE](NOTICE).

## What It Does

- Capture fullscreen, region, or a selected window.
- Start fullscreen, region, or window recordings with a compact icon bar for target, audio source, scale, quality, and record/pause/stop actions.
- Keep a live elapsed timer while recording, and use the same compact HUD even for direct-start recordings.
- Stop into a recording result window with lighter `Save`, `Copy Path`, and `Open` actions for the finished video.
- Run as a background daemon (`jjaengd`) with socket-based control.
- Show a 20%-larger, lightly transparent preview in the lower-left corner with fast `Save` / `Copy` actions and `double-click` / `E` to jump into the editor.
- Open a history surface with image/video thumbnails, quick copy/save, and edit entrypoints.
- Edit captures with blur, pen, arrow, rectangle, crop, text, and OCR tools.
- Follow the active Omarchy palette/menu style when available, with flat square controls across preview, history, launchpad, and recording prompt.
- Copy images to the clipboard as PNG.
- Save editor output as PNG, JPEG, or WEBP from the editor format dropdown.

## Workspace

- `crates/jjaeng-core`: capture, storage, clipboard, OCR, IPC, history, and shared services
- `crates/jjaeng-ui`: GTK runtime for preview, history, launchpad, and editor
- `crates/jjaeng-daemon`: hidden daemon binary (`jjaengd`)
- `crates/jjaeng-cli`: user-facing CLI binary (`jjaeng`)
- `crates/jjaeng-mcp`: local stdio MCP server (`agent-screen`, with `jjaeng-mcp` compatibility) for screen inspection, screenshots, recording, history, and UI actions

## Runtime Requirements

- Wayland
- Hyprland
- `grim`
- `slurp`
- `wl-clipboard`
- `gpu-screen-recorder` or `wl-screenrec` for video recording
- `pactl` for recording audio source discovery
- GTK4 runtime libraries
- `ffmpeg` for recording thumbnails

## Install

### AUR

```bash
yay -S jjaeng
```

Prebuilt binary package:

```bash
yay -S jjaeng-bin
```

Optional OCR models:

```bash
yay -S jjaeng-ocr-models
```

### Build From Source

```bash
git clone https://github.com/chllming/Jjaeng.git
cd Jjaeng
cargo build --release --workspace
install -Dm755 target/release/jjaeng ~/.local/bin/jjaeng
install -Dm755 target/release/jjaengd ~/.local/bin/jjaengd
install -Dm755 target/release/jjaeng-mcp ~/.local/bin/jjaeng-mcp
install -Dm755 target/release/agent-screen ~/.local/bin/agent-screen
```

## Usage

Start the daemon:

```bash
jjaengd
```

Capture commands:

```bash
jjaeng --capture-region
jjaeng --capture-window
jjaeng --capture-full
```

Recording commands:

```bash
jjaeng --record-region
jjaeng --record-region-prompt
jjaeng --record-window-prompt
jjaeng --stop-recording
```

`--record-*-prompt` opens the compact recording bar before capture starts so you can pick scale, quality, and either a system-audio source or microphone source. Press `Esc` before recording starts to cancel both the armed selection and the control bar. Plain `--record-*` starts immediately with current defaults, then keeps the same live HUD on screen for timer, pause, and stop.

Jjaeng uses whichever supported recorder backend is available, preferring `gpu-screen-recorder` and falling back to `wl-screenrec`. Finished recordings are written into history immediately, and the result window `Save` action copies the video into `~/Videos/` by default. In the result window, `S` saves, `C` copies the path, `O` opens the video, and `Esc` closes it.

History and follow-up actions:

```bash
jjaeng --launchpad
jjaeng --toggle-history
jjaeng --open-history
jjaeng --open-preview
jjaeng --edit-latest
jjaeng --copy-latest
jjaeng --save-latest
jjaeng --status-json
```

## MCP integration

`agent-screen` is the preferred local stdio Model Context Protocol server
identity, with `jjaeng-mcp` retained as a compatibility executable. It does not
open a network listener. Observation tools enumerate Hyprland monitors,
workspaces, windows, audio sources, and history; screenshot tools support
focused monitors, visible workspaces, regions, and selected windows; recording
tools support monitor, region, window, and workspace targets with pause/resume
and bounded duration. Recording and UI-mutating tools should remain
approval-gated in the MCP client, especially when microphone audio is requested.

Register it with Codex:

```bash
codex mcp add agent-screen -- ~/.local/bin/agent-screen
codex mcp list
```

The absolute path form is recommended for packaged installs:

```bash
codex mcp add agent-screen -- /usr/bin/agent-screen
```

The same executable can be registered in OpenClaw's `mcp.servers` registry. If
that agent uses an explicit tool allowlist, include `agent-screen__*` only for
agents trusted to see local screen contents, and keep recording tools in
prompt/approval mode. Existing `jjaeng__*` registrations remain compatible.

MCP Inspector can validate the stdio handshake and tool schemas:

```bash
npx @modelcontextprotocol/inspector /usr/bin/agent-screen
```

Logs are written to stderr so stdout remains reserved for MCP JSON-RPC.

## Agent Screen skills

Reusable agent workflows live under [`skills/`](skills/), including the
`agent-screen` entrypoint plus screen observation,
capture evidence, bounded recording, window/workspace resolution, visual QA,
privacy safeguards, and web research. The research workflow uses the host's
web-search tools and never uploads local screenshots or recordings implicitly.

## Desktop Integration

- Waybar helper script: [scripts/jjaeng-waybar-status.sh](scripts/jjaeng-waybar-status.sh)
- Omarchy/Hyprland bindings and daemon setup are expected to live in `~/.config`, not inside Omarchy-managed files
- When Omarchy is installed, Jjaeng reads the current Omarchy palette and menu typography as its runtime base theme

## Configuration

Config directory:

- `$XDG_CONFIG_HOME/jjaeng/`
- fallback: `$HOME/.config/jjaeng/`

Primary files:

- `config.json`
- `theme.json`
- `keybindings.json`

Notable setting:

- `screenshot_dir`: overrides the default output folder (default: `$HOME/Pictures`)
- `recording_dir`: overrides the default video save folder (default: `$HOME/Videos`)

## Development

```bash
cargo fmt --all
cargo check --workspace
cargo test --workspace
```

## License

Dual-licensed under MIT or Apache-2.0. See [LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE), and [NOTICE](NOTICE).
