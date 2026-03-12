# Waybar Integration

This repo now exposes two pieces needed for Omarchy/Waybar integration:

- `jjaeng --daemon`
- `jjaeng --status-json`

A helper script is included at:

- `scripts/jjaeng-waybar-status.sh`

## Start the daemon

Run Jjaeng as a hidden daemon:

```bash
jjaeng --daemon
```

Then normal capture commands can target the running daemon:

```bash
jjaeng --region
jjaeng --copy-latest
jjaeng --save-latest
jjaeng --dismiss-latest
jjaeng --edit-latest
```

## Waybar custom module

Example Waybar module:

```json
"custom/jjaeng": {
  "exec": "~/Code/Jjaeng/scripts/jjaeng-waybar-status.sh",
  "return-type": "json",
  "interval": 2,
  "on-click": "jjaeng --open-preview",
  "on-click-right": "jjaeng --edit-latest",
  "on-click-middle": "jjaeng --region"
}
```

Example placement in a module list:

```json
"modules-right": ["custom/jjaeng", "tray", "clock"]
```

## Omarchy notes

For Omarchy, keep this in your user-managed Waybar config under `~/.config/waybar/`. Do not edit Omarchy-managed files under `~/.local/share/omarchy/`.
