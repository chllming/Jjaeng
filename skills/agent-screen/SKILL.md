---
name: agent-screen
description: Use the local Agent Screen MCP for safe screen observation, focused captures, bounded recordings, and explicit desktop actions.
---

# Agent Screen

Use the MCP server registered as `agent-screen` for screen-aware agent work.
The legacy `jjaeng-mcp` executable exposes the same protocol for existing
clients.

- Inspect monitors, workspaces, windows, and the active window before choosing
  a target.
- Prefer stable window addresses and workspace/monitor selectors over title
  matching or interactive selection.
- Keep observation and screenshots read-only; request explicit approval before
  recording, microphone capture, or UI-mutating actions.
- Preserve returned artifact IDs, target geometry, timestamps, and media
  metadata so later visual QA can reproduce the capture.
- Keep local screenshots and recordings local when researching online; use the
  host web tools for external sources rather than uploading media implicitly.
