# Инвентаризация Python-контрактов поведения для parity

Дата: 2026-04-04

Источник поведения: `/home/alko/develop/open-source/ai/infrastructure/codex-worker`

## Цель

Зафиксировать поведенческие контракты Python-реализации, которые должны быть перенесены в Rust MVP без функциональной деградации.

## Блоки контрактов

### 1) Claim / Finalize

- Источник в Python:
  - `src/codex_worker/task_file.py`
  - `src/codex_worker/runner.py`
  - `tests/test_task_file.py::test_claim_next_task_accepts_running_status`
  - `tests/test_runner.py::test_successful_run_completes_task_and_writes_logs`
  - `tests/test_runner.py::test_run_next_processes_following_task_after_success`
- Краткий контракт поведения:
  - `claim_next_task` выбирает первую доступную задачу (`[ ]` или возобновляемую `[>]`) и записывает lease-метаданные (`last_run_id`, `lease_owner`, heartbeat).
  - После успешного выполнения задача финализируется в `[x]` с `last_result: success`; при ошибках в `[!]` с корректным reason.
  - После завершения всех задач файл переносится в `archive/`.
- Приоритет: `required-for-parity`
- Целевое место в Rust-плане/модулях:
  - Этапы 2 и 5
  - `src/task_file.rs`, `src/runner.rs`, `src/logs.rs`

### 2) Heartbeat / Recovery

- Источник в Python:
  - `src/codex_worker/task_file.py`
  - `src/codex_worker/runner.py`
  - `tests/test_task_file.py::test_recover_stale_running_marks_failed`
- Краткий контракт поведения:
  - Во время выполнения обновляется heartbeat lease.
  - Устаревшие running-задачи переводятся в failed через recovery с результатом вида `worker_interrupted`.
- Приоритет: `required-for-parity`
- Целевое место в Rust-плане/модулях:
  - Этапы 2 и 3
  - `src/task_file.rs`, `src/lockfile.rs`, `src/runner.rs`

### 3) Fail-closed

- Источник в Python:
  - `src/codex_worker/event_readers.py`
  - `src/codex_worker/runner.py`
  - `tests/test_runner.py::test_invalid_json_fails_closed`
  - `tests/test_event_readers.py::test_reader_emits_error_event_type_for_top_level_error`
- Краткий контракт поведения:
  - Некорректный runtime output (включая невалидный JSON) не допускает "ложный успех".
  - Run переводится в failed (`runtime_output_invalid`), а проблемные строки сохраняются в `raw_unparsed/*` и `problem_examples/*`.
- Приоритет: `required-for-parity`
- Целевое место в Rust-плане/модулях:
  - Этапы 4 и 5
  - `src/events/readers.rs`, `src/runner.rs`, `src/logs.rs`

### 4) Artifacts

- Источник в Python:
  - `src/codex_worker/logs.py`
  - `src/codex_worker/runner.py`
  - `tests/test_runner.py::test_successful_run_completes_task_and_writes_logs`
  - `tests/test_runner.py::test_subagent_unparsed_events_are_copied_to_dedicated_directory`
- Краткий контракт поведения:
  - Для каждого run создаётся полный набор артефактов (`prompt.md`, `stdout.jsonl`, `stderr.log`, `events.jsonl`, `summary.json`, подпапки `raw_unparsed`, `problem_examples`, `subagents`).
  - Артефакты должны быть пригодны для post-mortem и классификации сетевых/протокольных ошибок.
- Приоритет: `required-for-parity`
- Целевое место в Rust-плане/модулях:
  - Этап 5
  - `src/logs.rs`, `src/runner.rs`

### 5) Event normalization

- Источник в Python:
  - `src/codex_worker/events.py`
  - `src/codex_worker/event_readers.py`
  - `tests/test_event_readers.py::test_reader_emits_agent_tool_and_file_event_types`
  - `tests/test_runner.py::test_file_change_item_is_parsed_without_raw_unparsed`
  - `tests/test_runner.py::test_mcp_tool_call_item_is_parsed_without_raw_unparsed`
  - `tests/test_runner.py::test_web_search_item_is_parsed_without_raw_unparsed`
  - `tests/test_runner.py::test_todo_list_item_events_are_parsed_without_raw_unparsed`
  - `tests/test_runner.py::test_parses_modern_subagent_session_events_from_example`
- Краткий контракт поведения:
  - Поток `codex exec --json` и session-файлы субагентов приводятся к единой схеме `EventRecord`.
  - Обязательные нормализованные типы: `agent.message`, `tool.call`, `tool.result`, `file.change`, `todo.update`, `error`, `agent.session`, `agent.meta`, `raw.unparsed`.
  - Для поддерживаемых item/event типов не должно быть лишнего ухода в `raw.unparsed`.
- Приоритет: `required-for-parity`
- Целевое место в Rust-плане/модулях:
  - Этап 4
  - `src/events/record.rs`, `src/events/payloads.rs`, `src/events/readers.rs`

### 6) Runner flow

- Источник в Python:
  - `src/codex_worker/runner.py`
  - `src/codex_worker/cli.py`
  - `tests/test_runner.py::test_successful_run_completes_task_and_writes_logs`
  - `tests/test_runner.py::test_run_next_processes_following_task_after_success`
  - `tests/test_runner.py::test_real_codex_event_order_succeeds_when_agent_message_precedes_turn_completed`
  - `tests/test_runner.py::test_connection_failure_with_https_fallback_is_classified_from_problem_examples_file`
- Краткий контракт поведения:
  - `run-next` делает последовательный цикл claim -> spawn codex -> ingest events -> finalize.
  - Успешные и неуспешные запуски корректно отражаются в task status и `summary.json`.
  - Анализ connection/auth/network ошибок попадает в structured `failure_analysis`.
- Приоритет: `required-for-parity`
- Целевое место в Rust-плане/модулях:
  - Этап 5
  - `src/cli.rs`, `src/runner.rs`, `src/models.rs`

### 7) Console UI

- Источник в Python:
  - `src/codex_worker/console_ui.py`
  - `src/codex_worker/event_model.py`
  - `tests/test_console_ui.py`
  - `tests/test_event_model.py`
- Краткий контракт поведения:
  - Live UI показывает компактный status line, start/finish banners и timeline-строки по `EventRecord`.
  - Есть форматирование для `assistant/tool/tool result/file change/subagent/error` и консистентное дерево агентов.
- Приоритет: `secondary`
- Целевое место в Rust-плане/модулях:
  - Этап 6
  - `src/events/projector.rs`, `src/ui/console.rs`

## Явный mapping `python -> rust`

| Python (источник контракта) | Rust (целевой модуль/этап) |
| --- | --- |
| `src/codex_worker/task_file.py` | `src/task_file.rs` (Этап 2), `src/lockfile.rs` (Этап 3) |
| `src/codex_worker/lockfile.py` | `src/lockfile.rs` (Этап 3) |
| `src/codex_worker/runner.py` | `src/runner.rs` (Этап 5) |
| `src/codex_worker/logs.py` | `src/logs.rs` (Этап 5) |
| `src/codex_worker/events.py` | `src/events/payloads.rs` + `src/events/record.rs` (Этап 4) |
| `src/codex_worker/event_readers.py` | `src/events/readers.rs` (Этап 4) |
| `src/codex_worker/event_model.py` | `src/events/projector.rs` (Этап 6) |
| `src/codex_worker/console_ui.py` | `src/ui/console.rs` (Этап 6) |
| `src/codex_worker/cli.py` | `src/cli.rs` + `src/main.rs` (Этап 1/5) |
| `tests/test_task_file.py` | `tests/task_file.rs` (Этап 2/3 parity-набор) |
| `tests/test_event_readers.py` | `tests/event_readers.rs` (Этап 4 parity-набор) |
| `tests/test_runner.py` | `tests/runner.rs` + acceptance `v1` (Этап 5/8) |
| `tests/test_console_ui.py` + `tests/test_event_model.py` | `tests/event_projector.rs` + `tests/console_ui.rs` (Этап 6) |

## Статус для Этапа 0.1

- Инвентарь источников и контрактов подготовлен.
- Блоки `claim/finalize`, `heartbeat/recovery`, fail-closed, artifacts, event normalization, runner flow, console UI зафиксированы.
- Приоритеты и целевые точки переноса в Rust обозначены для дальнейшего acceptance-планирования (Этапы 0.2/0.3).
