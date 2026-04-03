# Acceptance Set `v1` для Rust-порта

Дата: 2026-04-04

Цель: зафиксировать минимальный набор acceptance-кейсов, который подтверждает поведенческий паритет Rust-версии с Python-контрактами из `docs/parity/python-test-inventory.md` для блоков `required-for-parity`.

## Правила набора

- В `acceptance-v1` входят только кейсы с приоритетом `required-for-parity`.
- Каждый кейс должен быть воспроизводим через заранее подготовленные фикстуры.
- Итоговая проверка `v1` считается пройденной только при `PASS` всех кейсов.

## Кейсы

### ACPT-001 — Claim/finalize happy path

- Идентификатор: `ACPT-001`
- Цель: подтвердить атомарный захват задачи и корректную финализацию успешного run.
- Входные данные/фикстуры:
  - task file с одной pending-задачей (`[ ]`), фиксированным `id` и `cwd`;
  - fake `codex exec --json`, возвращающий валидную последовательность: `thread.started` -> `turn.started` -> `agent_message` -> `turn.completed`.
- Ожидаемый результат:
  - задача переводится в `[x]`;
  - присутствует `last_result: success`;
  - записан `run_id`, а lease-поля очищены/финализированы по контракту;
  - процесс возвращает успех.
- Покрываемые Python-контракты: `claim/finalize`, `runner flow`.
- Rust-этап закрытия: `Этап 2`, `Этап 5`.

### ACPT-002 — Heartbeat/recovery stale-running

- Идентификатор: `ACPT-002`
- Цель: подтвердить, что stale-running задача корректно переводится в failed через recovery.
- Входные данные/фикстуры:
  - task file с задачей в `[>]` и устаревшим `last_heartbeat_at`;
  - `stale_after` настроен так, чтобы задача считалась просроченной.
- Ожидаемый результат:
  - recovery помечает задачу как failed (`[!]`);
  - причина содержит `worker_interrupted` (или эквивалентный reason по спецификации);
  - stale-задача не считается успешно завершённой.
- Покрываемые Python-контракты: `heartbeat/recovery`.
- Rust-этап закрытия: `Этап 2`, `Этап 3`.

### ACPT-003 — Fail-closed на невалидном JSON

- Идентификатор: `ACPT-003`
- Цель: исключить ложный успех при некорректном runtime output.
- Входные данные/фикстуры:
  - task file с одной pending-задачей;
  - fake `codex exec`, печатающий одну валидную JSON-строку и затем невалидный JSON.
- Ожидаемый результат:
  - run завершается ошибкой;
  - задача переводится в `[!]` с `runtime_output_invalid` (или эквивалентный fail-closed reason);
  - проблемные строки попадают в `raw_unparsed/*` и `problem_examples/*`.
- Покрываемые Python-контракты: `fail-closed`, `runner flow`.
- Rust-этап закрытия: `Этап 4`, `Этап 5`.

### ACPT-004 — Артефакты run layout

- Идентификатор: `ACPT-004`
- Цель: подтвердить обязательный набор run-артефактов и их пригодность для post-mortem.
- Входные данные/фикстуры:
  - task file с успешным сценарием;
  - fake `codex exec`, генерирующий stdout events и stderr line.
- Ожидаемый результат:
  - создана директория run в `.codex-worker`;
  - присутствуют `prompt.md`, `stdout.jsonl`, `stderr.log`, `events.jsonl`, `summary.json`;
  - при наличии проблемных строк создаются `raw_unparsed/*` и `problem_examples/*`.
- Покрываемые Python-контракты: `artifacts`.
- Rust-этап закрытия: `Этап 5`.

### ACPT-005 — Event normalization core stream

- Идентификатор: `ACPT-005`
- Цель: подтвердить нормализацию основных root-событий в канонический `EventRecord`.
- Входные данные/фикстуры:
  - fake main stdout с типами: `agent_message`, `command_execution`, `file_change`, `web_search`, `todo_list`, `error`.
- Ожидаемый результат:
  - в `events.jsonl` появляются соответствующие `event_type`: `agent.message`, `tool.call`, `tool.result`, `file.change`, `todo.update`, `error`;
  - поддерживаемые типы не уходят в `raw.unparsed`.
- Покрываемые Python-контракты: `event normalization`.
- Rust-этап закрытия: `Этап 4`.

### ACPT-006 — Event normalization subagent session import

- Идентификатор: `ACPT-006`
- Цель: подтвердить импорт и нормализацию subagent session-файлов.
- Входные данные/фикстуры:
  - `codex_home/sessions/.../*.jsonl` с современными payload-ветками (`session_meta`, `event_msg`, `response_item.*`, `turn_context`);
  - main stream с `collab_tool_call` и `receiver_thread_ids` для запуска импорта.
- Ожидаемый результат:
  - в `events.jsonl` появляются `agent.session`, `agent.message`, `tool.call`, `tool.result`, `agent.meta` для subagent;
  - невалидные строки subagent-файлов пишутся в `raw_unparsed/subagents.jsonl` и `problem_examples/subagents.jsonl`.
- Покрываемые Python-контракты: `event normalization`, `artifacts`.
- Rust-этап закрытия: `Этап 4`, `Этап 5`.

### ACPT-007 — Runner flow: последовательная обработка очереди

- Идентификатор: `ACPT-007`
- Цель: подтвердить цикл `run-next` на нескольких задачах и архивирование после полного завершения.
- Входные данные/фикстуры:
  - task file с двумя pending-задачами;
  - fake `codex exec`, дающий успешный ответ для обеих задач.
- Ожидаемый результат:
  - обе задачи переходят в `[x]`;
  - создаются два run summary;
  - task file переносится в `archive/` после завершения всех задач.
- Покрываемые Python-контракты: `runner flow`, `claim/finalize`.
- Rust-этап закрытия: `Этап 5`.

### ACPT-008 — Runner flow: классификация connection failure

- Идентификатор: `ACPT-008`
- Цель: подтвердить структурированную классификацию сетевой/авторизационной ошибки в `summary`.
- Входные данные/фикстуры:
  - fake `codex exec`, возвращающий `error` с reconnect и `403 Forbidden` для websocket/https endpoint.
- Ожидаемый результат:
  - run завершается с `failure_reason=connection_error`;
  - `summary.failure_analysis` содержит категорию, `http_status`, endpoint(s), host(s), цепочку инцидентов;
  - задача финализируется в failed.
- Покрываемые Python-контракты: `runner flow`, `fail-closed`.
- Rust-этап закрытия: `Этап 5`.

## Матрица покрытия required-for-parity

| Блок из inventory | Покрывающие acceptance-кейсы |
| --- | --- |
| `claim/finalize` | `ACPT-001`, `ACPT-007` |
| `heartbeat/recovery` | `ACPT-002` |
| `fail-closed` | `ACPT-003`, `ACPT-008` |
| `artifacts` | `ACPT-004`, `ACPT-006` |
| `event normalization` | `ACPT-005`, `ACPT-006` |
| `runner flow` | `ACPT-001`, `ACPT-003`, `ACPT-007`, `ACPT-008` |
