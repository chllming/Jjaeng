#!/usr/bin/env bash
# Compatibility wrapper for the historical Agent Screen helper name.
exec "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/agent-screen-waybar-status.sh" "$@"
