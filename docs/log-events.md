# События логов и их нормализация

Документ фиксирует текущую модель логов в `codex-worker-rs`:

- какие источники событий есть;
- как сырой лог маппится в канонический `EventRecord`;
- какие `event_type` реально попадают в `events.jsonl`;
- какие правила дедупликации и объединения применяются при импорте и при построении дерева событий.

Источник истины в коде:

- `crates/codex-log/src/events/readers/stdout.rs`
- `crates/codex-log/src/events/readers/session.rs`
- `crates/codex-log/src/events/record.rs`
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

Все поддержанные события приводятся к `EventRecord` и пишутся в `events.jsonl`.

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
| `command_execution` | `shell.call` или `shell.result` | `tool_name=command_execution`, `tool_use_id`, `input.command`, `output`, `stderr`, `exit_code`; для root stdout reader `shell.result.output` берётся из `aggregated_output` с fallback на `stdout` |
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
| `function_call` | `mcp.call`, `shell.call`, `tool.call`, `plan.update`, `user.input.request`, `stdin.write`, `collab.*` | `call_id` -> `tool_use_id`, `arguments` -> `input`, `session_path`, subagent metadata |
| `function_call_output` | `mcp.result`, `shell.result`, `tool.result`, `plan.update`, `user.input.request`, `stdin.write`, `collab.*` | `output`, `status=completed`, `phase=completed`; если `output` строка с JSON, сначала пробуется parse JSON |
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
| `exec_command_end` | `shell.result` | legacy форма завершения команды, помечается `duplicate_of=response_item.function_call_output` |
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
