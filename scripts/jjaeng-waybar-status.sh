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

text="shot"
class="idle"
tooltip="Jjaeng: ${latest_label}"

case "${state}" in
  preview)
    class="preview"
    text="copy/save"
    ;;
  editor)
    class="editor"
    text="edit"
    ;;
  idle)
    if [[ "${capture_count}" -gt 0 ]]; then
      class="ready"
      text="ready"
    fi
    ;;
  *)
    class="error"
    text="error"
    ;;
esac

printf '{"text":%s,"class":%s,"tooltip":%s}\n' \
  "$(jq -Rn --arg value "${text}" '$value')" \
  "$(jq -Rn --arg value "${class}" '$value')" \
  "$(jq -Rn --arg value "${tooltip}" '$value')"
