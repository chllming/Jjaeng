# Jjaeng

English | [한국어](README.ko.md)

Jjaeng is a Hyprland-first screenshot and recording tool for Wayland with a background daemon, compact bottom-left preview flow, screenshot/video history, Omarchy-aligned flat surfaces, and a built-in annotation editor.

Jjaeng originates from [ChalKak](https://github.com/BitYoungjae/ChalKak). This repository keeps the upstream licensing model and includes attribution in [NOTICE](NOTICE).

## What It Does

- Capture fullscreen, region, or a selected window.
- Start fullscreen, region, or window recordings with prompt-driven size, encoding, and audio controls.
- Run as a background daemon (`jjaengd`) with socket-based control.
- Show a compact preview with fast `Save` / `Copy` actions and `double-click` / `E` to jump into the editor.
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

## Runtime Requirements

- Wayland
- Hyprland
- `grim`
- `slurp`
- `wl-clipboard`
- GTK4 runtime libraries

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

## Development

```bash
cargo fmt --all
cargo check --workspace
cargo test --workspace
```

## License

Dual-licensed under MIT or Apache-2.0. See [LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE), and [NOTICE](NOTICE).
