---
name: jjaeng-recording-session
description: Run bounded Jjaeng recordings with explicit audio and stop handling.
---

# Jjaeng recording session

Ask for confirmation before recording, and always make the target, duration,
and audio mode explicit. Default to audio off. Use `recording_status` after
starting, and stop with `stop_recording` rather than killing the MCP process.
Use `pause_recording` and `resume_recording` only when the user requests them.

Microphone or combined desktop/microphone capture requires an explicit user
approval in the host MCP client.
