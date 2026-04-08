# События логов и их нормализация

Документ фиксирует текущую модель логов в `codex-worker-rs`:

- какие источники событий есть;
- как сырой лог маппится в канонический `EventRecord`;
- какие `event_type` реально попадают в `events.jsonl`;
- какие правила дедупликации и объединения применяются при импорте, operation-stream агрегации
  и при построении дерева событий.

Источник истины в коде:

- `crates/codex-log/src/events/readers/stdout.rs`
- `crates/codex-log/src/events/readers/session.rs`
- `crates/codex-log/src/events/record.rs`
- `crates/codex-log/src/events/operation_stream.rs`
- `crates/codex-log/src/events/payloads.rs`
- `crates/codex-log/src/session.rs`
- `crates/codex-log/src/tree.rs`
- `apps/codex-log-viewer-tauri-ui/src/components/session-event-list.tsx`
- `src/runner.rs`

Совместимые re-export файлы в `src/events/*` сохранены только для плавной миграции существующего
worker crate. Каноническая логика ingestion, session discovery, tail и tree/view-model теперь
находится в `crates/codex-log`, а UI-склейка парных operation-cards для `codex-log-viewer-tauri-ui`
живёт отдельно во frontend-компоненте viewer.

## 1. Источники логов

В проекте есть три уровня представления событий.

### 1.1. Сырые входные источники

- root stdout поток `codex exec --json`
- root stderr поток процесса
- session-файлы субагентов (`CODEX_HOME/sessions/.../*.jsonl` или standalone `rollout-*.jsonl`)

### 1.1.1. Discovery и metadata overlay для session-файлов

Сейчас в проекте есть два режима discovery:

- `SessionCatalog::list_sessions` и viewer v1 продолжают считать источником истины файловую
  структуру `CODEX_HOME/sessions/YYYY/MM/DD/*.jsonl`, а `session_index.jsonl` используют как
  metadata overlay;
- `SessionCatalog::list_indexed_sessions` и dialog picker в `codex-log-viewer-tauri-ui`
  в первую очередь читают `CODEX_HOME/state_*.sqlite`, таблицу `threads`, и только при
  недоступности SQLite откатываются к `session_index.jsonl`.

Для file-backed каталога действуют прежние правила:

- файл есть, индекса нет: сессия остаётся в каталоге с `index_status="missing"`;
- индекс есть, файла нет: запись считается stale и в основной список не попадает;
- дубли в `session_index.jsonl` по `session_id`: выбирается запись с максимальным `updated_at`,
  а при равенстве последняя строка в файле;
- дубли session-файлов для одного `session_id`: primary выбирается по самому свежему `mtime`,
  остальные фиксируются в diagnostics как `duplicate_session_files`;
- битые строки `session_index.jsonl` пропускаются fail-soft и не ломают построение каталога.

Для SQLite-backed каталога действуют отдельные правила:

- при наличии `state_*.sqlite` список строится по таблице `threads`, без обхода
  `CODEX_HOME/sessions`;
- `thread_name` берётся из `threads.title`, а если он пустой, используется
  `threads.first_user_message` или `threads.agent_nickname`;
- `updated_at` берётся из `threads.updated_at` и конвертируется из Unix seconds в RFC3339;
- конкретный `session_ref` для preview не хранится в списке и резолвится отдельно по
  `session_id` через `threads.rollout_path`;
- если SQLite недоступен или несовместим по схеме, каталог fail-soft откатывается к
  `session_index.jsonl`, а причина попадает в diagnostics;
- если rollout-файл для выбранного `session_id` не найден ни по `threads.rollout_path`, ни
  файловым fallback-поиском, preview завершается ошибкой открытия.

### 1.2. Канонический on-disk формат

Все поддержанные события приводятся к `EventRecord`.
Worker по-прежнему зеркалит их в `events.jsonl`, но этот файл теперь рассматривается как
compatibility artifact для runner/tests/manual inspection, а не как источник runtime correlation.
Runtime correlation для viewer/session/tree строится по replay-цепочке `stdout.jsonl` +
subagent session / standalone rollout и поверх неё агрегируется через `operation_stream`.

```json
{
  "schema_version": 1,
  "ts": "2026-04-06T15:01:01.125Z",
  "task_id": "task-1",
  "run_id": "run-1",
  "seq": 42,
  "event_type": "shell.result",
  "raw_type": "item.completed",
  "parse_status": "parsed",
  "payload": {}
}
```

Назначение полей:

| Поле | Смысл |
| --- | --- |
| `schema_version` | Версия схемы `events.jsonl`. Сейчас всегда `1`. |
| `ts` | ISO-время события. Для subagent session берётся из `timestamp`, иначе ставится текущее UTC-время. |
| `task_id`, `run_id` | Идентификаторы текущего запуска worker. |
| `seq` | Монотонный номер события внутри run. Общий и для root, и для импортированных subagent session. Для standalone viewer tail это локальный номер внутри session file. |
| `event_type` | Канонический внутренний тип события. Именно по нему работает projector, tree, HTML render и Tauri viewer. |
| `raw_type` | Исходный тип записи до нормализации: например `thread.started`, `response_item`, `event_msg`, `stderr`, `invalid_json`. |
| `parse_status` | Качество разбора: `parsed`, `best_effort`, `unparsed`. |
| `payload` | Нормализованная полезная нагрузка. |

### 1.3. In-memory формат во время ingestion

Во время чтения используется `CodexEvent`. Это тот же `EventRecord`, но с дополнительным полем `source`:

- `json_output`
- `subagent_session`
- `event_log`

Поле `source` в `events.jsonl` не сохраняется.

### 1.4. Runtime operation stream

После нормализации `EventRecord` runtime correlation больше не опирается на исторические
`tree.rs`-эвристики напрямую. Сначала события классифицируются в `atomic` / `lifecycle` и
агрегируются в operation stream:

- lifecycle surface задаётся per-kind policy registry в
  `crates/codex-log/src/events/operation_stream.rs`;
- на этом слое вычисляются `OperationSnapshot`, `revision` и terminal/update semantics;
- перед thread grouping, child-thread anchors и lifecycle parent/root anchoring `tree.rs`
  канонизирует `run_id`, `thread_id`, `sender_thread_id`, `parent_thread_id` и
  `receiver_thread_ids` теми же trim/empty->`None` правилами, что и `operation_stream`;
- если lifecycle terminal из legacy `event_msg.*_end` уже помечен как `duplicate_of` для
  `response_item.*`, snapshot должен сохранять этот canonical terminal и не переключаться на более
  поздний `response_item.function_call_output`;
- `tree.rs` использует snapshot metadata как приоритетный источник parent/root anchors для
  lifecycle-операций;
- поддерживаемый downstream renderer сейчас один: `codex-log-viewer-tauri-ui`; он должен
  предпочитать snapshot-selected terminal/result и откатываться к старым UI-эвристикам только как
  fallback для старых логов, где operation metadata ещё нет;
- `src/bin/events_tree_html.rs` остаётся reference artifact и может отставать от поддерживаемой
  runtime-поверхности.

Следствие для `events.jsonl`:

- файл может оставаться на диске ради совместимости;
- отсутствие или устаревание `events.jsonl` не должно менять runtime correlation, если replay
  строится из исходных raw run/session sources.

## 2. Внутренняя payload-модель

После записи на диск `EventRecord.payload` можно типизировать через `parse_payload(...)`,
но здесь есть один известный разрыв между ingestion и typed parser:

- динамические subagent-сообщения вида `message.<role>`, например `message.assistant`,
  downstream уже считаются message-событиями;
- при этом `parse_payload(...)` сейчас строго знает только `message.agent`,
  `message.user` и `message.commentary`, поэтому `message.assistant` остаётся в ветке `Unknown`.

| Группа событий | Внутренний payload-тип |
| --- | --- |
| `agent.started`, `agent.completed`, `agent.failed` | `AgentTurnPayload` |
| `message.agent`, `message.user`, `message.commentary`, `agent.reasoning` | `AgentMessagePayload` |
| `agent.session` | `AgentSessionPayload` |
| `agent.session.foreign`, `agent.meta`, `task.started`, `task.completed`, `runtime.context`, `context.compacted`, `context.compacted.duplicate`, `agent.aborted` | `AgentMetaPayload` |
| `info.tokens` | `InfoTokensPayload` |
| `tool.call`, `shell.call` | `ToolCallPayload` |
| `tool.result`, `shell.result`, `stdin.write`, `web.search`, `web.open`, `plan.update`, `user.input.request`, `patch.apply`, `patch.apply.duplicate`, `collab.*` | `ToolResultPayload` |
| `mcp.call` | `McpCallPayload` |
| `mcp.result` | `McpResultPayload` |
| `file.change` | `FileChangePayload` |
| `todo.update` | `TodoUpdatePayload` |
| `error` | `ErrorPayload` |
| `raw.unparsed` | `RawPayload` |
| `stderr.line` | `StderrPayload` |

Практический смысл:

- `event_type` определяет не только семантику, но и ожидаемую форму `payload`;
- `raw_type` нужен для трассировки исходного формата;
- `parse_status` нужен, чтобы отличать строгую нормализацию от fallback-пути.
- для dynamic `message.<role>` есть известная неполная типизация в `parse_payload(...)`.

## 3. Общие правила нормализации

### 3.1. `parse_status`

- `parsed`: формат распознан штатно, payload собран осознанно.
- `best_effort`: запись сохранена, но тип не поддержан полностью или разобран лишь частично.
- `unparsed`: строка вообще не была разобрана как JSON. Сейчас так помечается только root stdout `invalid_json`.

### 3.2. Нормализация tool-событий

Центральное правило задаётся функцией `normalized_tool_event_type(tool_name, is_result)`.

| Tool name | Старт | Завершение |
| --- | --- | --- |
| `update_plan` | `plan.update` | `plan.update` |
| `request_user_input` | `user.input.request` | `user.input.request` |
| `write_stdin` | `stdin.write` | `stdin.write` |
| `spawn_agent` | `collab.spawn_agent` | `collab.spawn_agent` |
| `send_input` | `collab.send_input` | `collab.send_input` |
| `wait`, `wait_agent` | `collab.wait` | `collab.wait` |
| `close_agent` | `collab.close_agent` | `collab.close_agent` |
| `resume_agent` | `collab.resume_agent` | `collab.resume_agent` |
| `command_execution`, `exec_command` | `shell.call` | `shell.result` |
| всё остальное | `tool.call` | `tool.result` |

Отдельный special-case для subagent `response_item.function_call`:

- `list_mcp_resources`
- `list_mcp_resource_templates`
- `read_mcp_resource`

они нормализуются в `mcp.call` / `mcp.result` с `server = "codex"`.

### 3.3. Root и subagent используют одну каноническую модель

Смысловой тип события должен быть одинаковым независимо от источника. Например:

- root `item.completed` c `item.type=command_execution` -> `shell.result`
- subagent `response_item.function_call_output` для `exec_command` -> `shell.result`
- subagent legacy `event_msg.exec_command_end` -> тоже `shell.result`

Разница между источниками в основном остаётся только в:

- `raw_type`
- деталях `payload`
- наличии `duplicate_of` у legacy/dedup-представлений

## 4. Маппинг root stdout -> `EventRecord`

### 4.1. Верхнеуровневые root записи

| Raw запись | Канонический `event_type` | Нормализованный payload |
| --- | --- | --- |
| `thread.started` | `thread.started` | `thread_id`, `model` |
| `turn.started` | `agent.started` | `actor_type=agent`, `thread_id` |
| `turn.completed` | `agent.completed` | `usage`, `result`, `thread_id` |
| `turn.failed` | `agent.failed` | `error`, `message`, `text_links`, `thread_id` |
| `error` | `error` | `message`, `error_type`, `text_links`, `thread_id` |
| невалидный JSON | `raw.unparsed` | `text` исходной строки, `raw_type=invalid_json`, `parse_status=unparsed` |

### 4.2. Root `item.started` / `item.updated` / `item.completed`

Фаза item-события превращается в `payload.phase`:

- `item.started` -> `started`
- `item.updated` -> `updated`
- `item.completed` -> `completed`

Дальше работает маппинг по `item.type`.

| `item.type` | Канонический `event_type` | Ключевые поля payload |
| --- | --- | --- |
| `agent_message` | `message.agent` | `item_id`, `text`, `text_links`, `thread_id` |
| `reasoning` | `agent.reasoning` | `text`, `text_links`, `thread_id` |
| `error` | `error` | `item_id`, `status`, `phase`, `message`, `error_type` |
| `command_execution` | `shell.call` или `shell.result` | `tool_name=command_execution`, `tool_use_id`, `input.command`, `output`, `stderr`, `exit_code`; для root stdout reader `shell.result.output` берётся из `aggregated_output` с fallback на `stdout`. Дополнительные tool-args вроде `workdir`, `yield_time_ms`, `max_output_tokens`, `login`, `tty`, `shell` здесь обычно отсутствуют и характерны прежде всего для subagent `exec_command`. |
| `mcp_tool_call` | `mcp.call` или `mcp.result` | `tool_use_id`, `arguments`, `result`, `error`, `server`, `tool` |
| `web_search` | `web.search` или `web.open` | `tool_name=web_search`, `query`, `action`; `open_page` уходит в `web.open` |
| `todo_list` | `todo.update` | `items[]`, `completed_count`, `total_count`, `status`, `phase` |
| `collab_tool_call` | `collab.*` или `tool.call` / `tool.result` | `sender_thread_id`, `receiver_thread_ids`, `prompt`, `agents_states`, `detection_*`, `raw_ref` |
| `tool_use` | через `normalized_tool_event_type(...)` | обычный tool/shell/collab/singleton mapping |
| `tool_result` | через `normalized_tool_event_type(...)` | `tool_use_id`, `status`, `stderr`, `error`, `output` |
| `file_change` | `file.change` | `item_id`, `status`, `changes[]` |
| всё неизвестное | `raw.unparsed` | исходный `item` в `payload.raw`, `parse_status=best_effort` |

Дополнительно runner сам генерирует:

| Источник | Канонический `event_type` | Payload |
| --- | --- | --- |
| stderr строка процесса | `stderr.line` | `actor_type=system`, `text` |

## 5. Маппинг subagent session -> `EventRecord`

### 5.1. `session_meta`

| Raw запись | Канонический `event_type` | Нормализованный payload |
| --- | --- | --- |
| `session_meta`, `payload.id == thread_id файла` | `agent.session` | `actor_type=subagent`, `thread_id`, `parent_thread_id`, `forked_from_id`, `cwd`, `agent_nickname`, `agent_role`, `session_path` |
| `session_meta`, `payload.id != thread_id файла` | `agent.session.foreign` | те же данные плюс `foreign_thread_id`, `raw`; это не открытие целевой сессии, а imported/forked history |

### 5.2. `response_item`

| `payload.type` | Канонический `event_type` | Нормализованный payload |
| --- | --- | --- |
| `function_call` | `mcp.call`, `shell.call`, `tool.call`, `plan.update`, `user.input.request`, `stdin.write`, `collab.*` | `call_id` -> `tool_use_id`, `arguments` -> `input`, `session_path`, subagent metadata. Для `exec_command` в `input` могут приходить `cmd`, `workdir`, `yield_time_ms`, `max_output_tokens`, `login`, `tty`, `shell`. |
| `function_call_output` | `mcp.result`, `shell.result`, `tool.result`, `plan.update`, `user.input.request`, `stdin.write`, `collab.*` | `output`, `status=completed`, `phase=completed`; если `output` строка с JSON, сначала пробуется parse JSON. Для subagent `exec_command` formatted output дополнительно разбирается на предмет строк `Process exited with code N` и `Original token count: N`, чтобы восстановить `exit_code` и `original_token_count`; исходный wrapper-текст сохраняется в `formatted_output`. |
| `custom_tool_call` | `patch.apply` или `tool.call` | для `apply_patch` старт тоже хранится как `ToolResultPayload` с `phase=started` |
| `custom_tool_call_output` | `patch.apply`, `patch.apply.duplicate` или `tool.result` | для `apply_patch` возможен duplicate-режим, см. раздел 7 |
| `web_search_call` | `web.search` или `web.open` | `query`, `action`, `phase`, `status`, `session_path` |
| `message` | `message.<role>` | `role`, `direction`, `phase`, `text`, `text_links`, `session_path` |
| `reasoning` | `agent.reasoning` | `role`, `phase`, `text`, `text_links`, `session_path` |
| всё неизвестное | `raw.unparsed` | `raw`, `reason`, `session_path`, `parse_status=best_effort` |

Примечание по сообщениям:

- event type строится динамически как `message.<role>`;
- на практике тестами покрыты `message.assistant` и `message.user`;
- если роль отсутствует, берётся `assistant`.

### 5.3. `compacted`

| Raw запись | Канонический `event_type` | Payload |
| --- | --- | --- |
| `compacted` | `context.compacted` | `message`, `replacement_history`, `thread_id`, `parent_thread_id`, `session_path` |

### 5.4. `event_msg`

| `payload.type` | Канонический `event_type` | Нормализованный payload |
| --- | --- | --- |
| `agent_message` | `message.<role>` | `text`, `phase`, `direction=output_text`, `session_path` |
| `token_count` | `info.tokens` | `input_tokens`, `cached_input_tokens`, `output_tokens`, `reasoning_output_tokens`, `total_tokens`, `total_token_usage`, `rate_limits` |
| `task_started` | `task.started` | `turn_id`, `model_context_window`, `collaboration_mode_kind`, `session_path` |
| `task_complete` | `task.completed` | `turn_id`, `last_agent_message`, `session_path` |
| `user_message` | `message.user` | `text`, `direction=input_text`, счётчики `images/local_images/text_elements` |
| `context_compacted` | `context.compacted` или `context.compacted.duplicate` | зависит от pending-флага, см. раздел 7 |
| `turn_aborted` | `agent.aborted` | `turn_id`, `reason`, `session_path` |
| `patch_apply_end` | `patch.apply` | `tool_name=apply_patch`, `success`, `changes`, `stdout`, `stderr`, `phase=completed` |
| `exec_command_end` | `shell.result` | legacy форма завершения команды, помечается `duplicate_of=response_item.function_call_output`; дополнительно сохраняет `cwd`, `process_id`, `source`, `duration`, `parsed_cmd`, `formatted_output` |
| `web_search_end` | `web.search` или `web.open` | legacy форма web search, помечается `duplicate_of=response_item.web_search_call` |
| `collab_agent_spawn_end` | `collab.spawn_agent` | legacy завершение spawn, с enrichment по `new_thread_id`, `new_agent_*`, `model`, `reasoning_effort`, `agents_states` |
| `collab_waiting_end` | `collab.wait` | legacy wait result, `receiver_thread_ids`, `agents_states`, `duplicate_of=response_item.function_call_output` |
| `collab_close_end` | `collab.close_agent` | legacy close result, `receiver_thread_ids`, `duplicate_of=response_item.function_call_output` |
| `collab_agent_interaction_end` | `collab.send_input` | legacy send_input result, `receiver_thread_ids`, `prompt`, `duplicate_of=response_item.function_call_output` |
| `item_completed` c `item.type=Plan` | `plan.update` | `tool_name=update_plan`, `output.text`, `duplicate_of=response_item.message` |
| всё остальное | `raw.unparsed` | `raw`, `reason`, `session_path`, `parse_status=best_effort` |

### 5.5. `turn_context`

| Raw запись | Канонический `event_type` | Особенности |
| --- | --- | --- |
| `turn_context` | `runtime.context` | `payload.turn_id` намеренно выбрасывается, остальные поля копируются как есть |

## 6. Полный перечень канонических событий

### 6.1. События, которые реально пишутся в `events.jsonl` текущими readers/runner

| `event_type` | Откуда появляется | Комментарий |
| --- | --- | --- |
| `thread.started` | root stdout | открытие root thread |
| `agent.started` | root stdout | начало turn |
| `agent.completed` | root stdout | успешное завершение turn |
| `agent.failed` | root stdout | неуспешное завершение turn |
| `agent.reasoning` | root stdout, subagent session | reasoning summary |
| `agent.session` | subagent session | нормальное открытие subagent session |
| `agent.session.foreign` | subagent session | `session_meta` не для целевого файла |
| `agent.aborted` | subagent session | legacy `turn_aborted` |
| `message.agent` | root stdout | root assistant/user-facing message из main потока |
| `message.assistant` | subagent session | самый частый subagent message |
| `message.user` | subagent session | user message внутри subagent session |
| `task.started` | subagent session | legacy meta о начале task |
| `task.completed` | subagent session | legacy meta о завершении task |
| `runtime.context` | subagent session | context turn/subagent |
| `context.compacted` | subagent session | compaction event |
| `context.compacted.duplicate` | subagent session | duplicate-marker после `compacted` |
| `info.tokens` | subagent session | token usage snapshot |
| `tool.call` | root stdout, subagent session | generic tool call |
| `tool.result` | root stdout, subagent session | generic tool result |
| `shell.call` | root stdout, subagent session | command execution start |
| `shell.result` | root stdout, subagent session | command execution end |
| `mcp.call` | root stdout, subagent session | MCP call |
| `mcp.result` | root stdout, subagent session | MCP result |
| `stdin.write` | root stdout, subagent session | singleton tool event |
| `web.search` | root stdout, subagent session | search action |
| `web.open` | root stdout, subagent session | `open_page` action |
| `plan.update` | root stdout, subagent session | singleton tool event или legacy plan item |
| `user.input.request` | root stdout, subagent session | singleton tool event |
| `patch.apply` | subagent session | `apply_patch` start/end |
| `patch.apply.duplicate` | subagent session | duplicate completion после `patch_apply_end` |
| `collab.spawn_agent` | root stdout, subagent session | singleton collab event |
| `collab.send_input` | root stdout, subagent session | singleton collab event |
| `collab.wait` | root stdout, subagent session | singleton collab event |
| `collab.close_agent` | root stdout, subagent session | singleton collab event |
| `collab.resume_agent` | root stdout, subagent session | singleton collab event |
| `file.change` | root stdout | file change item |
| `todo.update` | root stdout | todo list item |
| `error` | root stdout, subagent session | top-level или item error |
| `raw.unparsed` | root stdout, subagent session | fallback для неподдержанного/битого ввода |
| `stderr.line` | runner | отдельная линия stderr |

### 6.2. Поддерживаются downstream, но сейчас не являются обычным результатом readers

| `event_type` | Статус |
| --- | --- |
| `agent.meta` | compatibility/derived тип, поддержан projector и payload parser, но текущие readers его почти не генерируют |
| `message.commentary` | константа существует, но текущая нормализация обычно использует `message.assistant` + `phase=commentary` |
| `message.plan` | это не сырой `events.jsonl`, а производное tree/view-событие после merge `plan.update` + message |

### 6.3. Реестр известных канонических `event_type`

Этот реестр фиксирует prerequisite-результат задачи
`2026-04-08-event-type-category-classification`.
Базовая единица здесь именно канонический `event_type`, а не operation family.

#### 6.3.1. Что показали два rollout-лога от 2026-04-08

- rollout reviewer-subagent
  `/home/alko/.codex/sessions/2026/04/08/rollout-2026-04-08T15-08-19-019d6cfe-531c-7bc2-aa6f-546351ea71ac.jsonl`
  содержит только `response_item.*`, `event_msg.*`, `turn_context` и `session_meta`;
  из lifecycle-path там наблюдаются только `exec_command -> shell.call/shell.result`;
- rollout regular-agent
  `/home/alko/.codex/sessions/2026/04/08/rollout-2026-04-08T03-31-35-019d6a80-6ed7-7fe2-96e0-84c14c7c2f16.jsonl`
  дополнительно показывает `write_stdin`, `update_plan`, `request_user_input`,
  `spawn_agent`, `wait_agent`, `apply_patch`, `web_search_call`, а также legacy
  `event_msg.exec_command_end`, `event_msg.patch_apply_end`, `event_msg.web_search_end`,
  `event_msg.collab_agent_spawn_end`, `event_msg.collab_waiting_end`,
  `event_msg.item_completed(item.type=Plan)`, `event_msg.context_compacted`,
  `event_msg.turn_aborted`;
- ни один из этих rollout-файлов не содержит root `item.*`, потому что это session-логи;
  поэтому покрытие `item.*` ниже фиксируется по `stdout` reader и разделу 4 этого документа,
  а не по наблюдённым строкам;
- в reviewer rollout реально наблюдался `response_item.message` с `role=developer`,
  то есть динамический namespace `message.<role>` на практике шире, чем явный реестр констант.
  В таблицу ниже он не включён намеренно: эта фиксация ограничена объединением
  `docs/log-events.md (6.1/6.2)` и `crates/codex-log/src/events/types.rs`.

#### 6.3.2. `event_type -> source_presence`

| `event_type` | docs 6.1 | docs 6.2 | types.rs | note |
| --- | --- | --- | --- | --- |
| `thread.started` | `yes` | `no` | `yes` |  |
| `agent.started` | `yes` | `no` | `yes` |  |
| `agent.completed` | `yes` | `no` | `yes` |  |
| `agent.failed` | `yes` | `no` | `yes` |  |
| `message.agent` | `yes` | `no` | `yes` |  |
| `agent.reasoning` | `yes` | `no` | `yes` |  |
| `agent.session` | `yes` | `no` | `yes` |  |
| `agent.session.foreign` | `yes` | `no` | `yes` |  |
| `agent.meta` | `no` | `yes` | `yes` | downstream-only тип из docs 6.2 и `types.rs`; текущие readers почти не эмитят |
| `agent.aborted` | `yes` | `no` | `yes` |  |
| `message.commentary` | `no` | `yes` | `yes` | downstream-only тип из docs 6.2 и `types.rs`; текущая нормализация обычно выражает commentary через `message.assistant` + `phase=commentary` |
| `message.user` | `yes` | `no` | `yes` |  |
| `message.plan` | `no` | `yes` | `yes` | downstream-only производный merge `plan.update` + `response_item.message`; не сырой ingestion event |
| `message.assistant` | `yes` | `no` | `no` | docs-only явный член динамического namespace `message.<role>`; в `types.rs` отдельной константы нет |
| `task.started` | `yes` | `no` | `yes` |  |
| `task.completed` | `yes` | `no` | `yes` |  |
| `runtime.context` | `yes` | `no` | `yes` |  |
| `context.compacted` | `yes` | `no` | `yes` |  |
| `context.compacted.duplicate` | `yes` | `no` | `yes` |  |
| `info.tokens` | `yes` | `no` | `yes` |  |
| `tool.call` | `yes` | `no` | `yes` |  |
| `tool.result` | `yes` | `no` | `yes` |  |
| `shell.call` | `yes` | `no` | `yes` |  |
| `shell.result` | `yes` | `no` | `yes` |  |
| `mcp.call` | `yes` | `no` | `yes` |  |
| `mcp.result` | `yes` | `no` | `yes` |  |
| `stdin.write` | `yes` | `no` | `yes` |  |
| `web.search` | `yes` | `no` | `yes` |  |
| `web.open` | `yes` | `no` | `yes` |  |
| `plan.update` | `yes` | `no` | `yes` |  |
| `user.input.request` | `yes` | `no` | `yes` |  |
| `patch.apply` | `yes` | `no` | `yes` |  |
| `patch.apply.duplicate` | `yes` | `no` | `yes` |  |
| `collab.spawn_agent` | `yes` | `no` | `yes` |  |
| `collab.send_input` | `yes` | `no` | `yes` |  |
| `collab.wait` | `yes` | `no` | `yes` |  |
| `collab.close_agent` | `yes` | `no` | `yes` |  |
| `collab.resume_agent` | `yes` | `no` | `yes` |  |
| `file.change` | `yes` | `no` | `yes` |  |
| `todo.update` | `yes` | `no` | `yes` |  |
| `error` | `yes` | `no` | `yes` |  |
| `raw.unparsed` | `yes` | `no` | `yes` |  |
| `stderr.line` | `yes` | `no` | `yes` |  |

#### 6.3.3. `event_type -> category`

| `event_type` | `category` | note |
| --- | --- | --- |
| `thread.started` | `atomic` |  |
| `agent.started` | `atomic` |  |
| `agent.completed` | `atomic` |  |
| `agent.failed` | `atomic` |  |
| `message.agent` | `atomic` |  |
| `agent.reasoning` | `atomic` |  |
| `agent.session` | `atomic` |  |
| `agent.session.foreign` | `atomic` |  |
| `agent.meta` | `atomic` | compatibility/derived meta event, не operation lifecycle |
| `agent.aborted` | `atomic` |  |
| `message.commentary` | `atomic` |  |
| `message.user` | `atomic` |  |
| `message.plan` | `atomic` | derived downstream merge, не отдельный ingestion lifecycle |
| `message.assistant` | `atomic` | atomic факт внутри динамического namespace `message.<role>` |
| `task.started` | `atomic` |  |
| `task.completed` | `atomic` |  |
| `runtime.context` | `atomic` |  |
| `context.compacted` | `atomic` |  |
| `context.compacted.duplicate` | `atomic` |  |
| `info.tokens` | `atomic` |  |
| `tool.call` | `lifecycle` | generic lifecycle bucket для не-specialized tool/collab/custom-tool путей |
| `tool.result` | `lifecycle` | generic lifecycle result bucket для не-specialized tool/collab/custom-tool путей |
| `shell.call` | `lifecycle` | старт операции shell/exec |
| `shell.result` | `lifecycle` | завершение или update операции shell/exec |
| `mcp.call` | `lifecycle` | старт MCP call |
| `mcp.result` | `lifecycle` | завершение или update MCP call |
| `stdin.write` | `lifecycle` | singleton lifecycle: один `event_type`, различение через `payload.phase` |
| `web.search` | `lifecycle` | phased lifecycle search-path; `open_page` вынесен в `web.open` |
| `web.open` | `lifecycle` | phased lifecycle open-page path внутри web search surface |
| `plan.update` | `lifecycle` | singleton lifecycle; legacy completion также приходит через `event_msg.item_completed` |
| `user.input.request` | `lifecycle` | singleton lifecycle; различение через `payload.phase` |
| `patch.apply` | `lifecycle` | phased lifecycle с legacy completion `event_msg.patch_apply_end` |
| `patch.apply.duplicate` | `atomic` | duplicate-marker после `event_msg.patch_apply_end`; не owner lifecycle state |
| `collab.spawn_agent` | `lifecycle` | singleton lifecycle; legacy completion есть |
| `collab.send_input` | `lifecycle` | singleton lifecycle; legacy completion есть в коде, но не наблюдалась в двух логах |
| `collab.wait` | `lifecycle` | singleton lifecycle; legacy completion наблюдалась |
| `collab.close_agent` | `lifecycle` | singleton lifecycle; legacy completion есть в коде, но не наблюдалась в двух логах |
| `collab.resume_agent` | `lifecycle` | singleton lifecycle без legacy `event_msg.*` completion |
| `file.change` | `lifecycle` | root-only phased lifecycle по `item.started/updated/completed` |
| `todo.update` | `lifecycle` | root-only phased lifecycle по `item.started/updated/completed` |
| `error` | `atomic` |  |
| `raw.unparsed` | `atomic` |  |
| `stderr.line` | `atomic` |  |

В этой фиксации `undecided` не осталось: все типы из объединённого реестра получили
категорию `atomic` или `lifecycle`.

### 6.4. Lifecycle boundary mapping

Boundary-профиль ниже зафиксирован только для `lifecycle`-типов. Для каждой строки
ячейки показывают `start:` и `end:` внутри трёх raw-семейств:

- `item.*` для root stdout reader;
- `response_item.*` для subagent session item-layer;
- `event_msg.*` для legacy subagent session path.

Пустая ячейка означает, что в текущем ingestion такого raw-пути нет. Для пар
`tool.call/tool.result`, `shell.call/shell.result` и `mcp.call/mcp.result` boundary-семейство
в таблице повторяется осознанно: единица классификации остаётся канонический `event_type`,
а не operation family.

| `event_type` | boundary role | `item.*` | `response_item.*` | `event_msg.*` | note |
| --- | --- | --- | --- | --- | --- |
| `tool.call` | `split call/result` | start: `item.started(tool_use:name=<generic>)`, `item.started(collab_tool_call:tool=<fallback>)`<br>end: `item.updated(tool_result:name=<generic>)`, `item.completed(tool_result:name=<generic>)`, `item.updated(collab_tool_call:tool=<fallback>)`, `item.completed(collab_tool_call:tool=<fallback>)` | start: `response_item.function_call(name=<generic>)`, `response_item.custom_tool_call(name!=apply_patch)`<br>end: `response_item.function_call_output(name=<generic>)`, `response_item.custom_tool_call_output(name!=apply_patch)` | — | generic fallback bucket; в двух rollout-логах не наблюдался |
| `tool.result` | `split call/result` | start: `item.started(tool_use:name=<generic>)`, `item.started(collab_tool_call:tool=<fallback>)`<br>end: `item.updated(tool_result:name=<generic>)`, `item.completed(tool_result:name=<generic>)`, `item.updated(collab_tool_call:tool=<fallback>)`, `item.completed(collab_tool_call:tool=<fallback>)` | start: `response_item.function_call(name=<generic>)`, `response_item.custom_tool_call(name!=apply_patch)`<br>end: `response_item.function_call_output(name=<generic>)`, `response_item.custom_tool_call_output(name!=apply_patch)` | — | same family as `tool.call`; canonical result/update half |
| `shell.call` | `split call/result` | start: `item.started(command_execution)`<br>end: `item.updated(command_execution)`, `item.completed(command_execution)` | start: `response_item.function_call(exec_command)`<br>end: `response_item.function_call_output(exec_command)` | end: `event_msg.exec_command_end` | `response_item.*` observed в reviewer и regular-agent rollout; legacy end observed в regular-agent rollout |
| `shell.result` | `split call/result` | start: `item.started(command_execution)`<br>end: `item.updated(command_execution)`, `item.completed(command_execution)` | start: `response_item.function_call(exec_command)`<br>end: `response_item.function_call_output(exec_command)` | end: `event_msg.exec_command_end` | same family as `shell.call`; canonical result/update half |
| `mcp.call` | `split call/result` | start: `item.started(mcp_tool_call)`<br>end: `item.updated(mcp_tool_call)`, `item.completed(mcp_tool_call)` | start: `response_item.function_call(list_mcp_resources|list_mcp_resource_templates|read_mcp_resource)`<br>end: `response_item.function_call_output(list_mcp_resources|list_mcp_resource_templates|read_mcp_resource)` | — | code/docs-only in provided logs |
| `mcp.result` | `split call/result` | start: `item.started(mcp_tool_call)`<br>end: `item.updated(mcp_tool_call)`, `item.completed(mcp_tool_call)` | start: `response_item.function_call(list_mcp_resources|list_mcp_resource_templates|read_mcp_resource)`<br>end: `response_item.function_call_output(list_mcp_resources|list_mcp_resource_templates|read_mcp_resource)` | — | same family as `mcp.call`; canonical result/update half |
| `stdin.write` | `phased singleton` | start: `item.started(tool_use:name=write_stdin)`<br>end: `item.updated(tool_result:name=write_stdin)`, `item.completed(tool_result:name=write_stdin)` | start: `response_item.function_call(write_stdin)`<br>end: `response_item.function_call_output(write_stdin)` | — | observed в regular-agent rollout по `response_item.*` |
| `web.search` | `phased same-type` | start: `item.started(web_search:action!=open_page)`<br>end: `item.updated(web_search:action!=open_page)`, `item.completed(web_search:action!=open_page)` | start: `response_item.web_search_call(action!=open_page, phase=started)`<br>end: `response_item.web_search_call(action!=open_page, phase=updated|completed)` | end: `event_msg.web_search_end(action!=open_page)` | observed в regular-agent rollout; `open_page` path в двух логах не встретился |
| `web.open` | `phased same-type` | start: `item.started(web_search:action=open_page)`<br>end: `item.updated(web_search:action=open_page)`, `item.completed(web_search:action=open_page)` | start: `response_item.web_search_call(action=open_page, phase=started)`<br>end: `response_item.web_search_call(action=open_page, phase=updated|completed)` | end: `event_msg.web_search_end(action=open_page)` | code/docs-only in provided logs |
| `plan.update` | `phased singleton` | start: `item.started(tool_use:name=update_plan)`<br>end: `item.updated(tool_result:name=update_plan)`, `item.completed(tool_result:name=update_plan)` | start: `response_item.function_call(update_plan)`<br>end: `response_item.function_call_output(update_plan)` | end: `event_msg.item_completed(item.type=Plan)` | `response_item.*` и legacy end observed в regular-agent rollout |
| `user.input.request` | `phased singleton` | start: `item.started(tool_use:name=request_user_input)`<br>end: `item.updated(tool_result:name=request_user_input)`, `item.completed(tool_result:name=request_user_input)` | start: `response_item.function_call(request_user_input)`<br>end: `response_item.function_call_output(request_user_input)` | — | observed один раз в regular-agent rollout по `response_item.*` |
| `patch.apply` | `phased same-type` | — | start: `response_item.custom_tool_call(apply_patch)`<br>end: `response_item.custom_tool_call_output(apply_patch)` | end: `event_msg.patch_apply_end` | both completion paths observed в regular-agent rollout; если `patch_apply_end` пришёл первым, поздний `custom_tool_call_output` уходит в atomic `patch.apply.duplicate` |
| `collab.spawn_agent` | `phased singleton` | start: `item.started(tool_use:name=spawn_agent)`, `item.started(collab_tool_call:tool=spawn_agent)`<br>end: `item.updated(tool_result:name=spawn_agent)`, `item.completed(tool_result:name=spawn_agent)`, `item.updated(collab_tool_call:tool=spawn_agent)`, `item.completed(collab_tool_call:tool=spawn_agent)` | start: `response_item.function_call(spawn_agent)`<br>end: `response_item.function_call_output(spawn_agent)` | end: `event_msg.collab_agent_spawn_end` | `response_item.*` и legacy end observed в regular-agent rollout |
| `collab.send_input` | `phased singleton` | start: `item.started(tool_use:name=send_input)`, `item.started(collab_tool_call:tool=send_input)`<br>end: `item.updated(tool_result:name=send_input)`, `item.completed(tool_result:name=send_input)`, `item.updated(collab_tool_call:tool=send_input)`, `item.completed(collab_tool_call:tool=send_input)` | start: `response_item.function_call(send_input)`<br>end: `response_item.function_call_output(send_input)` | end: `event_msg.collab_agent_interaction_end` | code/docs-only in provided logs |
| `collab.wait` | `phased singleton` | start: `item.started(tool_use:name=wait|wait_agent)`, `item.started(collab_tool_call:tool=wait|wait_agent)`<br>end: `item.updated(tool_result:name=wait|wait_agent)`, `item.completed(tool_result:name=wait|wait_agent)`, `item.updated(collab_tool_call:tool=wait|wait_agent)`, `item.completed(collab_tool_call:tool=wait|wait_agent)` | start: `response_item.function_call(wait|wait_agent)`<br>end: `response_item.function_call_output(wait|wait_agent)` | end: `event_msg.collab_waiting_end` | `response_item.*` и legacy end observed в regular-agent rollout |
| `collab.close_agent` | `phased singleton` | start: `item.started(tool_use:name=close_agent)`, `item.started(collab_tool_call:tool=close_agent)`<br>end: `item.updated(tool_result:name=close_agent)`, `item.completed(tool_result:name=close_agent)`, `item.updated(collab_tool_call:tool=close_agent)`, `item.completed(collab_tool_call:tool=close_agent)` | start: `response_item.function_call(close_agent)`<br>end: `response_item.function_call_output(close_agent)` | end: `event_msg.collab_close_end` | code/docs-only in provided logs |
| `collab.resume_agent` | `phased singleton` | start: `item.started(tool_use:name=resume_agent)`, `item.started(collab_tool_call:tool=resume_agent)`<br>end: `item.updated(tool_result:name=resume_agent)`, `item.completed(tool_result:name=resume_agent)`, `item.updated(collab_tool_call:tool=resume_agent)`, `item.completed(collab_tool_call:tool=resume_agent)` | start: `response_item.function_call(resume_agent)`<br>end: `response_item.function_call_output(resume_agent)` | — | code/docs-only in provided logs; legacy `event_msg.*` completion в current reader нет |
| `file.change` | `phased same-type` | start: `item.started(file_change)`<br>end: `item.updated(file_change)`, `item.completed(file_change)` | — | — | root-only lifecycle; session rollout logs его не показывают |
| `todo.update` | `phased same-type` | start: `item.started(todo_list)`<br>end: `item.updated(todo_list)`, `item.completed(todo_list)` | — | — | root-only lifecycle; session rollout logs его не показывают |

## 7. Правила дедупликации при ingestion

### 7.1. Жёсткое подавление дублей сообщений

Subagent message вообще не добавляется в `events.jsonl`, если:

- предыдущее subagent message пришло из другого представления (`response_item.message` против `event_msg.agent_message`/`event_msg.user_message`);
- совпадают `role`;
- совпадает `phase` после trim/normalization;
- совпадает непустой `text` после trim.

Иными словами, пара:

- `response_item.message`
- `event_msg.agent_message`

или:

- `event_msg.user_message`
- `response_item.message`

с одинаковым смыслом превращается в одно событие.

### 7.2. `context_compacted` как дубликат `compacted`

После `compacted` выставляется флаг `pending_context_compacted_duplicate`.

Если следующая совместимая legacy-запись:

- `event_msg.payload.type == "context_compacted"`

приходит до любого другого ломающего флаг события, то генерируется:

- `context.compacted.duplicate`
- `payload.duplicate_of = "compacted"`

Иначе legacy запись трактуется как самостоятельный `context.compacted`.

### 7.3. `patch_apply_end` против `custom_tool_call_output`

Если для `apply_patch` уже был `event_msg.patch_apply_end`, то следующий
`response_item.custom_tool_call_output` с тем же `call_id` не считается новым
результатом. Вместо этого пишется:

- `patch.apply.duplicate`
- `payload.duplicate_of = "event_msg.patch_apply_end"`

### 7.4. Legacy result events не выкидываются, а маркируются

Некоторые legacy `event_msg.*` сохраняются как самостоятельные события, но с явной
ссылкой на более современное представление:

| Legacy event | `duplicate_of` |
| --- | --- |
| `exec_command_end` | `response_item.function_call_output` |
| `collab_*_end` | `response_item.function_call_output` |
| `item_completed` для `Plan` | `response_item.message` |
| `web_search_end` | `response_item.web_search_call` |

Причина: эти legacy записи часто содержат дополнительные поля, полезные для tree/view слоя,
поэтому они не удаляются на ingestion-этапе.

### 7.5. Tail semantics для `SessionReader`

Incremental tail для standalone rollout и live-import subagent sessions работает через
`TailCursor` и следующие правила:

- если `offset <= file.size` и `file_identity` не изменилась, чтение продолжается с `offset`;
- если `offset > file.size`, это считается truncate, возвращается `reset=true`, чтение
  начинается с начала файла;
- если изменилась `file_identity` (`device` / `inode` / `mtime+size` fallback), это считается
  rotate/recreate, возвращается `reset=true`, чтение начинается с начала файла;
- незавершённая JSONL-строка без финального `\n` не эмитится; байты копятся в
  `TailCursor.pending_fragment` до завершения строки;
- на границе reset/rotate используется короткое rolling dedup-window по стабильному event key,
  а при его отсутствии по hash сырой строки, чтобы не дублировать только что импортированные
  события;
- `session_ref` в `TailCursor` обязан совпадать с целевой сессией; смена файла для другой
  сессии требует нового cursor.

## 8. Правила объединения после ingestion

Дедупликация выше отвечает за содержимое `events.jsonl`. Ниже описано вторичное
объединение уже нормализованных событий при построении дерева и при UI-рендере merged cards
в `codex-log-viewer-tauri-ui`.

Источник истины этого слоя разделён на два уровня:

- `crates/codex-log/src/tree.rs` строит базовое дерево событий и привязку child-thread к якорным
  операциям;
- `apps/codex-log-viewer-tauri-ui/src/components/session-event-list.tsx` поверх этого дерева
  склеивает парные `started/completed` события в одну карточку и inline-раскрывает subagent
  timeline в общую хронологическую ленту.

### 8.1. Склейка start/result одной операции

Внутри потока thread дерево ищет пары по ключу:

- `operation_kind`
- `operation_id`

`operation_id` берётся из:

- `tool_use_id`, если он есть;
- иначе из `item_id`.

Результат (`completed` / `updated`) подвешивается под стартовое событие той же операции.

Это работает для:

- `tool.call` / `tool.result`
- `shell.call` / `shell.result`
- `mcp.call` / `mcp.result`
- singleton tool-событий (`stdin.write`, `web.*`, `plan.update`, `user.input.request`, `patch.apply`, `collab.*`)
- `file.change`
- `todo.update`

### 8.2. Follow-up события подвешиваются под ближайший tool/shell result

После `tool.result` или `shell.result` следующие события считаются follow-up и
временно цепляются как дети, пока не встретится barrier event.

Follow-up:

- `agent.reasoning`
- `stderr.line`
- любое `message.*`, кроме `message.user`, если `phase=commentary`
- `agent.meta` с `meta_type in {"message", "task_complete", "user_message"}`

Barrier event:

- любой новый tool/shell/mcp event
- lifecycle/meta события (`agent.started`, `agent.completed`, `task.*`, `runtime.context`, `thread.started`, `agent.session`, ...)
- `message.user`
- `context.compacted*`
- `patch.apply.duplicate`

### 8.3. `plan.update` + message -> производный `message.plan`

В `build_event_tree` выполняется специальная склейка пары:

- `plan.update` из `event_msg.item_completed`
- следующий за ним `response_item.message`

если одновременно верно:

- у `plan.update` стоит `duplicate_of = "response_item.message"`;
- текст плана совпадает;
- события соседние в потоке thread.

Результат:

- вместо двух записей появляется производный `message.plan`;
- `duplicate_of` у merged view очищается;
- `tool_name` и `operation_id` убираются;
- роль/направление/phase берутся в первую очередь из `response_item.message`.

Важно: это merge только на уровне дерева/HTML. Исходные записи в `events.jsonl` не переписываются.

### 8.4. Объединённые HTML-карточки операций

HTML renderer рендерит единые карточки для пар:

- `shell.call` + `shell.result`
- `patch.apply` started + completed
- `collab.spawn_agent` started + completed
- `user.input.request` started + completed
- `collab.send_input` started + completed
- `collab.wait` started + completed
- `collab.close_agent` started + completed
- `collab.resume_agent` started + completed

Если у операции есть несколько candidate result-событий, выбирается preferred result.

Общее правило preference:

- выше приоритет у записи с `duplicate_of = "response_item.function_call_output"`;
- затем у записи с более богатым payload;
- затем у более поздней записи по `seq`.

Из-за этого legacy-result может стать главным представлением, а обычный
`response_item.function_call_output` без `duplicate_of` будет скрыт как redundant.
Если у такого redundant `shell.result` уже есть дочерние follow-up события
(`message.*`, `agent.reasoning`, `stderr.line`), сами дети поднимаются на уровень merged-card и
не теряются.

### 8.5. Дополнительные merge-правила в HTML

- `task lifecycle`: `task.started` / `agent.meta(task_started)` открывает сегмент timeline и
  группирует все соседние `TimelineItem` до matching `task.completed` /
  `agent.meta(task_complete)` с тем же `turn_id`. Если matching completion не найден, сегмент
  остаётся открытым и закрывается перед следующим `task.started`. Для сегмента используется palette
  по `collaboration_mode_kind`.
- `task.started`: в viewer summary для такого события подавляется; вместо этого в карточке
  показываются `mode`, `turn`, `context window`, а `collaboration_mode_kind` дополнительно
  выводится как mode-badge.
- `task.completed`: если есть `last_agent_message`, viewer рендерит его как отдельный detail-блок
  `Last Agent Message` и не дублирует тем же текстом общий detail.
- `patch.apply`: header берётся из `started` события, а detail комбинируется из обеих частей. В итоговой карточке viewer по умолчанию показывает только список файлов; `phase/status` остаются только для нештатных случаев без списка изменений. Для diff `tauri-ui` в первую очередь использует `event_msg.patch_apply_end -> changes[path].unified_diff`; сырой `payload.input` start-события остаётся только fallback-источником, если per-file diff отсутствует. Colorized diff по умолчанию скрыт и раскрывается явным toggle. `patch.apply.duplicate` скрывается как redundant и не рендерится отдельной карточкой.
- `spawn_agent`: данные call/result объединяются в одну meta-модель. Предпочтение у result для `model`, `reasoning_effort`, `receiver_*`, у call для `prompt` и `requested_agent_type`.
- `user.input.request`: ответы из started/completed частей объединяются по `question.id`, одинаковые значения ответа дедуплицируются.
- `collab` states: список состояний агентов после разворачивания из `agents_states` и `output` дополнительно дедуплицируется по полному равенству записи.

### 8.6. Дополнительные projected fields для `patch.apply`

Для `load_session` / event tree `EventEntry` теперь отдельно проецирует patch-specific поля,
которые нужны `codex-log-viewer-tauri-ui` для detail-рендера `patch.apply` без парсинга сырого
payload на стороне UI:

- `patch_apply_status`: строка из `payload.status` для `patch.apply` / `patch.apply.duplicate`;
- `patch_apply_input`: строка из `payload.input`, если patch start несёт сырой `*** Begin Patch`;
- `patch_apply_changes`: нормализованный список изменений.

Правила для `patch_apply_changes`:

- если `payload.changes` является объектом вида `path -> {type|kind}`, он разворачивается в массив
  записей `{ path, change_type, unified_diff, move_path }`;
- если `payload.changes` уже массив объектов, берутся `path`, `type|kind`, `unified_diff`,
  `move_path`;
- `move_path` сохраняется только если это непустая строка; `null` не превращается в строку
  `"null"`;
- список сортируется по `path`;
- для других `event_type` эти поля остаются пустыми.

### 8.7. Дополнительные projected fields для `shell.call` / `shell.result`

Для `load_session` / event tree `EventEntry` теперь отдельно проецирует shell-specific поля,
чтобы viewer не парсил сырой `payload.input` и legacy `payload.duration/parsed_cmd` вручную:

- `shell_workdir`: requested `input.workdir` из `exec_command`, если задан;
- `shell_cwd`: фактический `cwd` из `shell.result`, если он присутствует;
- `shell_yield_time_ms`: `input.yield_time_ms`;
- `shell_max_output_tokens`: `input.max_output_tokens`;
- `shell_login`: `input.login`;
- `shell_tty`: `input.tty`;
- `shell_binary`: `input.shell`;
- `shell_process_id`: `process_id` из legacy/result payload;
- `shell_source`: `source` из legacy/result payload;
- `shell_duration_ns`: нормализованная длительность из `duration.{secs,nanos}`;
- `shell_original_token_count`: `original_token_count` из `shell.result`, если известен;
- `shell_formatted_output`: `formatted_output` из legacy/result payload;
- `shell_parsed_commands[]`: нормализованный список записей из `parsed_cmd` с полями
  `kind`, `command`, `query`, `name`, `path`.

Правила:

- поля заполняются только для canonical shell-family `command_execution` / `exec_command`;
- call-specific поля (`workdir`, `yield_time_ms`, `max_output_tokens`, `login`, `tty`, `shell`)
  берутся из `payload.input` и доступны и у `shell.call`, и у `shell.result`, если там есть
  `input`;
- result-specific поля (`cwd`, `process_id`, `source`, `duration`, `formatted_output`,
  `parsed_cmd`) проецируются только для `shell.result`;
- для subagent `response_item.function_call_output` с `tool_name=exec_command` `exit_code` и
  `original_token_count` могут восстанавливаться из formatted output строк `Process exited with
  code N` и `Original token count: N`, а сам raw wrapper-текст дополнительно сохраняется в
  `formatted_output`, даже если legacy `exec_command_end` отсутствует;
- `shell_duration_ns` хранит полную длительность в наносекундах;
- пустые или отсутствующие значения не превращаются в строки вроде `"null"` и остаются `null`
  / пустым массивом.

## 9. Нюансы и ограничения

- В namespace сообщений есть асимметрия: root поток использует `message.agent`, а subagent session строит `message.<role>`, чаще всего `message.assistant`.
- `message.commentary` поддержан downstream, но текущая нормализация обычно выражает commentary через `phase=commentary`, а не через отдельный `event_type`.
- `message.plan` существует только как производная tree/view-проекция.
- `agent.meta` тоже относится скорее к compatibility/view-слою; текущие readers чаще эмитят более конкретные `task.*`, `runtime.context`, `context.compacted`, `message.*`.
- `raw.unparsed` не означает ошибку парсинга всегда: это ещё и контейнер для поддержанного, но пока не нормализованного upstream-формата.

## 10. Краткое резюме

Если смотреть на систему как на pipeline, она состоит из четырёх шагов:

1. сырые root/subagent логи приводятся к единой схеме `EventRecord`;
2. явные дубли либо подавляются, либо помечаются через `payload.duplicate_of`;
3. `SessionCatalog` и `SessionReader` добавляют discovery, metadata overlay и incremental tail
   поверх `CODEX_HOME/sessions` и standalone rollout-файлов;
4. tree/view слой поверх `EventRecord` выполняет дополнительное семантическое объединение, не
   меняя исходный `events.jsonl`.
