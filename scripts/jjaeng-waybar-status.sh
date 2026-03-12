#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
jjaeng_bin="${JJAENG_BIN:-}"
if [[ -z "${jjaeng_bin}" ]]; then
  if [[ -x "${repo_root}/target/debug/jjaeng" ]]; then
    jjaeng_bin="${repo_root}/target/debug/jjaeng"
  else
    jjaeng_bin="$(command -v jjaeng)"
  fi
fi

status_json="$("${jjaeng_bin}" --status-json 2>/dev/null || true)"
if [[ -z "${status_json}" ]]; then
  status_json='{"state":"error","latest_label":"Jjaeng unavailable","capture_count":0,"preview_count":0,"editor_open":false}'
fi

state="$(jq -r '.state // "idle"' <<<"${status_json}")"
latest_label="$(jq -r '.latest_label // "No capture yet"' <<<"${status_json}")"
capture_count="$(jq -r '.capture_count // 0' <<<"${status_json}")"
preview_count="$(jq -r '.preview_count // 0' <<<"${status_json}")"
editor_open="$(jq -r '.editor_open // false' <<<"${status_json}")"

text=""
class="idle"
tooltip="Jjaeng\nLatest: ${latest_label}\nSession captures: ${capture_count}\nClick: open history"

case "${state}" in
  preview)
    class="preview"
    tooltip="Jjaeng\nPreview active (${preview_count})\nLatest: ${latest_label}\nClick: open history"
    ;;
  editor)
    class="editor"
    tooltip="Jjaeng\nEditor active\nLatest: ${latest_label}\nClick: open history"
    ;;
  idle)
    if [[ "${capture_count}" -gt 0 ]]; then
      class="ready"
      tooltip="Jjaeng\nRecent capture ready\nLatest: ${latest_label}\nClick: open history"
    fi
    ;;
  *)
    class="error"
    text=""
    tooltip="Jjaeng\nStatus unavailable\nClick: open history"
    ;;
esac

if [[ "${editor_open}" == "true" && "${class}" != "error" ]]; then
  class="editor"
fi

printf '{"text":%s,"alt":%s,"class":%s,"tooltip":%s}\n' \
  "$(jq -Rn --arg value "${text}" '$value')" \
  "$(jq -Rn --arg value "jjaeng" '$value')" \
  "$(jq -Rn --arg value "${class}" '$value')" \
  "$(jq -Rn --arg value "${tooltip}" '$value')"
