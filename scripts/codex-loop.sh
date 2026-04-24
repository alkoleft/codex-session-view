#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CODEX_BIN="${CODEX_BIN:-codex}"
PYTHON_BIN="${PYTHON_BIN:-python3}"

COUNT=""
PROMPT=""
PROMPT_FILE=""
WORK_DIR="$ROOT_DIR"
OUTPUT_DIR=""
SLEEP_SECONDS="0"
CONTINUE_ON_ERROR=0
OPENSPEC_APPLY_MODE=0
OPENSPEC_CHANGE=""
OPENSPEC_CHANGE_ORDER=""
PROMPT_TEMPLATE="$ROOT_DIR/scripts/impl-prompt.md"
DRY_RUN=0

declare -a CODEX_ARGS=()

usage() {
  cat <<'EOF'
Usage:
  scripts/codex-loop.sh --count N (--prompt TEXT | --prompt-file FILE) [options] [-- codex exec options]
  scripts/codex-loop.sh --count N [options] [-- codex exec options] < prompt.txt
  scripts/codex-loop.sh --count N --openspec-apply [--change NAME | --change-order A,B,C] [options] [-- codex exec options]

Runs `codex exec` in a loop.

Required:
  -n, --count N              Number of Codex runs.

Prompt input for generic mode:
  -p, --prompt TEXT          Prompt text.
  -f, --prompt-file FILE     Read prompt from file.
  stdin                      Used when neither --prompt nor --prompt-file is provided.

OpenSpec mode:
  --openspec-apply           Generate prompt from `openspec instructions apply --json`.
  --change NAME              Use a specific OpenSpec change.
  --change-order A,B,C       Preferred order of changes in OpenSpec mode.
  --prompt-template FILE     Template prepended to generated OpenSpec prompt.
                             Defaults to scripts/impl-prompt.md.
  --dry-run                  Print selected change and generated prompt, do not run Codex.

Options:
  -C, --cd DIR               Working directory passed to `codex exec -C`.
                             Defaults to the repository root.
  -o, --output-dir DIR       Write each run's final Codex message to DIR/run-NNN.md.
                             In OpenSpec mode also writes DIR/prompt-NNN.md.
  --sleep SECONDS            Sleep between runs. Defaults to 0.
  --continue-on-error        Keep running after a failed Codex invocation.
  -h, --help                 Show this help.

Everything after `--` is forwarded to `codex exec`.

Examples:
  scripts/codex-loop.sh --count 3 --prompt "Summarize current git status" -- --sandbox read-only
  scripts/codex-loop.sh -n 5 -f prompt.md -o var/codex-loop -- --model gpt-5.4 --sandbox workspace-write
  printf '%s\n' "Run repo checks and report failures" | scripts/codex-loop.sh -n 2 -- --full-auto
  scripts/codex-loop.sh --count 1 --openspec-apply --change add-project-context-skill-metrics --dry-run
  scripts/codex-loop.sh --count 4 --openspec-apply --change-order add-project-context-skill-metrics,add-task-metrics-grain -- -m gpt-5.4 --sandbox workspace-write
EOF
}

log() {
  printf '[codex-loop] %s\n' "$*"
}

die() {
  printf '[codex-loop] %s\n' "$*" >&2
  exit 1
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    die "Missing required command: $1"
  fi
}

python_json() {
  local mode="$1"
  local payload="${2-}"
  JSON_PAYLOAD="$payload" "$PYTHON_BIN" - "$mode" <<'PY'
import json
import os
import sys

mode = sys.argv[1]
payload = json.loads(os.environ["JSON_PAYLOAD"])

if mode == "list_active_change_names":
    for change in payload.get("changes", []):
        if change.get("status") == "in-progress" and change.get("completedTasks", 0) < change.get("totalTasks", 0):
            print(change.get("name", ""))
elif mode == "schema_name":
    print(payload["schemaName"])
elif mode == "progress_total":
    print(payload["progress"]["total"])
elif mode == "progress_complete":
    print(payload["progress"]["complete"])
elif mode == "progress_remaining":
    print(payload["progress"]["remaining"])
elif mode == "change_dir":
    print(payload["changeDir"])
elif mode == "instruction":
    print(payload["instruction"])
elif mode == "state":
    print(payload["state"])
elif mode == "context_files":
    for key, values in payload.get("contextFiles", {}).items():
        for value in values:
            print(f"- [{key}] {value}")
elif mode == "pending_tasks":
    for task in payload.get("tasks", []):
        if not task.get("done"):
            print(f"- [ ] {task['description']}")
else:
    raise SystemExit(f"unknown python_json mode: {mode}")
PY
}

is_positive_integer() {
  [[ "$1" =~ ^[1-9][0-9]*$ ]]
}

is_non_negative_integer() {
  [[ "$1" =~ ^[0-9]+$ ]]
}

read_stdin_prompt() {
  if [[ -t 0 ]]; then
    die "Prompt is required. Use --prompt, --prompt-file, or pipe prompt text to stdin."
  fi

  PROMPT="$(cat)"
}

parse_args() {
  while (($#)); do
    case "$1" in
      -n|--count)
        [[ $# -ge 2 ]] || die "$1 requires a value"
        COUNT="$2"
        shift 2
        ;;
      -p|--prompt)
        [[ $# -ge 2 ]] || die "$1 requires a value"
        PROMPT="$2"
        shift 2
        ;;
      -f|--prompt-file)
        [[ $# -ge 2 ]] || die "$1 requires a value"
        PROMPT_FILE="$2"
        shift 2
        ;;
      -C|--cd)
        [[ $# -ge 2 ]] || die "$1 requires a value"
        WORK_DIR="$2"
        shift 2
        ;;
      -o|--output-dir)
        [[ $# -ge 2 ]] || die "$1 requires a value"
        OUTPUT_DIR="$2"
        shift 2
        ;;
      --sleep)
        [[ $# -ge 2 ]] || die "$1 requires a value"
        SLEEP_SECONDS="$2"
        shift 2
        ;;
      --continue-on-error)
        CONTINUE_ON_ERROR=1
        shift
        ;;
      --openspec-apply)
        OPENSPEC_APPLY_MODE=1
        shift
        ;;
      --change)
        [[ $# -ge 2 ]] || die "$1 requires a value"
        OPENSPEC_CHANGE="$2"
        shift 2
        ;;
      --change-order)
        [[ $# -ge 2 ]] || die "$1 requires a value"
        OPENSPEC_CHANGE_ORDER="$2"
        shift 2
        ;;
      --prompt-template)
        [[ $# -ge 2 ]] || die "$1 requires a value"
        PROMPT_TEMPLATE="$2"
        shift 2
        ;;
      --dry-run)
        DRY_RUN=1
        shift
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      --)
        shift
        CODEX_ARGS=("$@")
        break
        ;;
      *)
        die "Unknown argument: $1"
        ;;
    esac
  done
}

validate_config() {
  [[ -n "$COUNT" ]] || die "--count is required"
  is_positive_integer "$COUNT" || die "--count must be a positive integer"
  is_non_negative_integer "$SLEEP_SECONDS" || die "--sleep must be a non-negative integer"
  [[ -d "$WORK_DIR" ]] || die "Working directory does not exist: $WORK_DIR"

  if [[ -n "$OUTPUT_DIR" ]]; then
    mkdir -p "$OUTPUT_DIR"
  fi

  if ((OPENSPEC_APPLY_MODE == 1)); then
    [[ -f "$PROMPT_TEMPLATE" ]] || die "Prompt template does not exist: $PROMPT_TEMPLATE"
    [[ -z "$PROMPT" && -z "$PROMPT_FILE" ]] || die "Do not combine --openspec-apply with --prompt/--prompt-file"
    [[ -z "$OPENSPEC_CHANGE" || -z "$OPENSPEC_CHANGE_ORDER" ]] || die "Use either --change or --change-order, not both"
    require_cmd openspec
    require_cmd "$PYTHON_BIN"
    return
  fi

  if [[ -n "$PROMPT" && -n "$PROMPT_FILE" ]]; then
    die "Use only one prompt source: --prompt or --prompt-file"
  fi

  if [[ -n "$PROMPT_FILE" ]]; then
    [[ -f "$PROMPT_FILE" ]] || die "Prompt file does not exist: $PROMPT_FILE"
    PROMPT="$(<"$PROMPT_FILE")"
  elif [[ -z "$PROMPT" ]]; then
    read_stdin_prompt
  fi

  [[ -n "$PROMPT" ]] || die "Prompt cannot be empty"
}

active_changes_json() {
  openspec list --json
}

list_active_change_names() {
  python_json list_active_change_names "$(cat)"
}

select_openspec_change() {
  local active_json
  local -a active_names
  local -a ordered_names
  local change

  active_json="$(active_changes_json)"
  mapfile -t active_names < <(printf '%s' "$active_json" | list_active_change_names)

  if ((${#active_names[@]} == 0)); then
    printf '%s' ""
    return 0
  fi

  if [[ -n "$OPENSPEC_CHANGE" ]]; then
    for change in "${active_names[@]}"; do
      if [[ "$change" == "$OPENSPEC_CHANGE" ]]; then
        printf '%s' "$change"
        return 0
      fi
    done
    die "OpenSpec change is not active or has no remaining tasks: $OPENSPEC_CHANGE"
  fi

  if [[ -n "$OPENSPEC_CHANGE_ORDER" ]]; then
    IFS=',' read -r -a ordered_names <<<"$OPENSPEC_CHANGE_ORDER"
    for change in "${ordered_names[@]}"; do
      change="${change#"${change%%[![:space:]]*}"}"
      change="${change%"${change##*[![:space:]]}"}"
      [[ -n "$change" ]] || continue
      for active in "${active_names[@]}"; do
        if [[ "$active" == "$change" ]]; then
          printf '%s' "$active"
          return 0
        fi
      done
    done
    die "None of the changes from --change-order are currently active with pending tasks"
  fi

  if ((${#active_names[@]} == 1)); then
    printf '%s' "${active_names[0]}"
    return 0
  fi

  printf '[codex-loop] Multiple active OpenSpec changes detected. Use --change or --change-order.\n' >&2
  printf '%s\n' "${active_names[@]}" >&2
  exit 1
}

write_generated_prompt() {
  local change="$1"
  local run_number="$2"
  local apply_json="$3"
  local prompt_file="$4"
  local schema_name
  local total_tasks
  local complete_tasks
  local remaining_tasks
  local change_dir
  local instruction

  schema_name="$(python_json schema_name "$apply_json")"
  total_tasks="$(python_json progress_total "$apply_json")"
  complete_tasks="$(python_json progress_complete "$apply_json")"
  remaining_tasks="$(python_json progress_remaining "$apply_json")"
  change_dir="$(python_json change_dir "$apply_json")"
  instruction="$(python_json instruction "$apply_json")"

  {
    cat "$PROMPT_TEMPLATE"
    cat <<EOF

---
## Automation Context

- Run: $run_number/$COUNT
- Using change: $change
- Schema: $schema_name
- Change dir: $change_dir
- Progress: $complete_tasks/$total_tasks complete, $remaining_tasks remaining

### OpenSpec Instruction

$instruction

### Context Files
EOF
    python_json context_files "$apply_json"

    cat <<'EOF'

### Pending Tasks (ordered)
EOF
    python_json pending_tasks "$apply_json"

    cat <<'EOF'

### Execution Rules

- Работай только в рамках выбранного OpenSpec change.
- Перед изменениями прочитай все context files, перечисленные выше.
- Выполняй pending tasks в указанном порядке.
- После завершения каждой задачи сразу отмечай её в `tasks.md`.
- Если задача неясна или всплыл design issue, остановись и явно опиши блокер.
- Когда все задачи change завершены, выполни UAT через Playwright в соответствии с `AGENTS.md`, исправь замечания и повтори UAT до зелёного результата.
- Не переключайся на другие OpenSpec changes, пока текущий запуск не сообщит о завершении или блокере.
EOF
  } >"$prompt_file"
}

run_generic_codex_once() {
  local run_number="$1"
  local -a cmd

  cmd=("$CODEX_BIN" exec -C "$WORK_DIR")

  if [[ -n "$OUTPUT_DIR" ]]; then
    printf -v output_file '%s/run-%03d.md' "$OUTPUT_DIR" "$run_number"
    cmd+=("--output-last-message" "$output_file")
  fi

  cmd+=("${CODEX_ARGS[@]}" -)

  log "Run $run_number/$COUNT"
  printf '%s\n' "$PROMPT" | "${cmd[@]}"
}

run_openspec_codex_once() {
  local run_number="$1"
  local change
  local apply_json
  local state
  local prompt_file
  local -a cmd

  change="$(select_openspec_change)"
  if [[ -z "$change" ]]; then
    log "No active OpenSpec changes with pending tasks remain"
    return 10
  fi

  apply_json="$(openspec instructions apply --change "$change" --json)"
  state="$(python_json state "$apply_json")"

  case "$state" in
    ready)
      ;;
    blocked)
      die "OpenSpec change '$change' is blocked. Complete missing artifacts before applying."
      ;;
    all_done)
      log "OpenSpec change '$change' has no remaining tasks"
      return 10
      ;;
    *)
      die "Unexpected OpenSpec apply state for '$change': $state"
      ;;
  esac

  prompt_file="$(mktemp)"
  trap 'rm -f "$prompt_file"' RETURN
  write_generated_prompt "$change" "$run_number" "$apply_json" "$prompt_file"

  if [[ -n "$OUTPUT_DIR" ]]; then
    local prompt_copy
    printf -v prompt_copy '%s/prompt-%03d.md' "$OUTPUT_DIR" "$run_number"
    cp "$prompt_file" "$prompt_copy"
  fi

  if ((DRY_RUN == 1)); then
    log "Dry run for change '$change'"
    cat "$prompt_file"
    rm -f "$prompt_file"
    trap - RETURN
    return 0
  fi

  cmd=("$CODEX_BIN" exec -C "$WORK_DIR")

  if [[ -n "$OUTPUT_DIR" ]]; then
    local output_file
    printf -v output_file '%s/run-%03d.md' "$OUTPUT_DIR" "$run_number"
    cmd+=("--output-last-message" "$output_file")
  fi

  cmd+=("${CODEX_ARGS[@]}" -)

  log "Run $run_number/$COUNT using change '$change'"
  "${cmd[@]}" <"$prompt_file"

  rm -f "$prompt_file"
  trap - RETURN
}

main() {
  parse_args "$@"
  validate_config
  require_cmd "$CODEX_BIN"

  local failures=0
  local run_number
  local rc

  for ((run_number = 1; run_number <= COUNT; run_number++)); do
    rc=0
    if ((OPENSPEC_APPLY_MODE == 1)); then
      if ! run_openspec_codex_once "$run_number"; then
        rc=$?
      fi
      if ((rc == 10)); then
        break
      fi
    else
      if ! run_generic_codex_once "$run_number"; then
        rc=$?
      fi
    fi

    if ((rc != 0)); then
      failures=$((failures + 1))
      log "Run $run_number failed"

      if ((CONTINUE_ON_ERROR == 0)); then
        exit 1
      fi
    fi

    if ((run_number < COUNT && SLEEP_SECONDS > 0)); then
      sleep "$SLEEP_SECONDS"
    fi
  done

  if ((failures > 0)); then
    die "$failures run(s) failed"
  fi

  log "Completed $COUNT run(s)"
}

main "$@"
