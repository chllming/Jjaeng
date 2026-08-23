# Waybar Integration

This repo now exposes two pieces needed for Omarchy/Waybar integration:

- `agent-screen --daemon`
- `agent-screen --status-json`

A helper script is included at:

- `scripts/agent-screen-waybar-status.sh`

## Start the daemon

Run Agent Screen as a hidden daemon:

```bash
agent-screen --daemon
```

Then normal capture commands can target the running daemon:

```bash
agent-screen --region
agent-screen --copy-latest
agent-screen --save-latest
agent-screen --dismiss-latest
agent-screen --edit-latest
```

## Waybar custom module

Example Waybar module:

```json
"custom/agent-screen": {
  "exec": "~/Code/Agent Screen/scripts/agent-screen-waybar-status.sh",
  "return-type": "json",
  "interval": 2,
  "on-click": "agent-screen --open-preview",
  "on-click-right": "agent-screen --edit-latest",
  "on-click-middle": "agent-screen --region"
}
```

Example placement in a module list:

```json
"modules-right": ["custom/agent-screen", "tray", "clock"]
```

## Omarchy notes

For Omarchy, keep this in your user-managed Waybar config under `~/.config/waybar/`. Do not edit Omarchy-managed files under `~/.local/share/omarchy/`.
