---
name: agent-screen-capture-evidence
description: Capture reproducible screenshot evidence with target and artifact metadata.
---

# Agent Screen capture evidence

Resolve the monitor/workspace/window first. Use the most specific screenshot
tool available, preserve the returned artifact ID, and report the target
geometry, timestamp, and media path. Use `history` or `get_artifact` to verify
that the artifact was persisted before referring to it later.
