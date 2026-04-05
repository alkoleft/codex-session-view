# План: `codex-worker-rs`

Дата: 2026-04-05

Исходный проект: `/home/alko/develop/open-source/ai/infrastructure/codex-worker`

Целевой проект: `/home/alko/develop/open-source/ai/infrastructure/codex-worker-rs`

## Правило языка документации

- Вся пояснительная документация проекта пишется по-русски.
- Английский допустим только для имён файлов и каталогов, команд, API, форматов, названий библиотек, code identifiers и устоявшихся технических терминов.
- Новые документы, ADR и архитектурные описания должны соблюдать это правило по умолчанию.

## Утверждённый TODO-чеклист (Этапы 0-8)

- `[x]` Этап 0.1. Инвентаризация Python-контрактов поведения и первичный `python -> rust` mapping.
  Scope: зафиксировать источники контрактов и целевые Rust-модули для паритета.
  Артефакты: `docs/parity/python-test-inventory.md`.
  Verify: документ покрывает `claim/finalize`, `heartbeat/recovery`, fail-closed, artifacts, event normalization, runner flow, console UI и содержит явную таблицу `python -> rust`.

- `[x]` Этап 0.2. Зафиксировать минимальный acceptance set `v1` в машиночитаемом виде.
  Scope: определить обязательные сценарии `v1` и формат их запуска.
  Артефакты: `docs/parity/acceptance-v1.md`, `tests/acceptance/manifest.json` (или эквивалентный manifest).
  Verify: список кейсов воспроизводим, покрывает required-for-parity контракты и пригоден для CI.

- `[x]` Этап 0.3. Добавить и задокументировать временную репозиторную CI-проверку `acceptance-v1`.
  Scope: встроить в pipeline репозиторную валидацию артефактов acceptance (`manifest` + документация) как временную проверку до runtime-исполнения кейсов.
  Артефакты: `.github/workflows/acceptance-v1.yml` (или эквивалент), обновление `README.md`.
  Verify: job `acceptance-v1` присутствует и проверяет консистентность acceptance-артефактов; политика `required status check` настраивается только во внешней конфигурации GitHub и не верифицируется из этого workspace (нет доступа к remote/config).

- `[x]` Этап 1. Создать каркас Rust-проекта.
  Scope: инициализировать crate, CLI `run-next`, базовые модели и error layer.
  Артефакты: `Cargo.toml`, `src/main.rs`, `src/cli.rs`, `src/models.rs`, `src/error.rs`, `src/util.rs`.
  Verify: `cargo test` и `cargo run -- run-next --help` выполняются успешно.

- `[x]` Этап 2. Перенести file-based task engine.
  Scope: парсинг markdown-задач, claim/finalize, recovery, archive, atomic write.
  Артефакты: `src/task_file.rs`, `tests/task_file.rs`.
  Verify: портированные сценарии из `tests/test_task_file.py` проходят без регрессий.

- `[x]` Этап 3. Перенести locking.
  Scope: lock-файл, `flock`, heartbeat payload и конкурентные инварианты.
  Артефакты: `src/lockfile.rs`, интеграционные тесты конкурентности.
  Verify: подтверждены инварианты no double-claim, атомарный finalize, корректный stale recovery под конкуренцией.

- `[x]` Этап 4. Перенести event model и readers.
  Scope: нормализация root/subagent событий в совместимый `EventRecord`.
  Артефакты: `src/events/record.rs`, `src/events/payloads.rs`, `src/events/readers.rs`, `tests/event_readers.rs`.
  Verify: портированные сценарии из `tests/test_event_readers.py` проходят; fail-closed сигналы эквивалентны Python.

- `[x]` Этап 5. Перенести runner и artifact pipeline.
  Scope: orchestration loop `run-next`, subprocess `codex exec`, сбор stdout/stderr, summary и finalize.
  Артефакты: `src/runner.rs`, `src/logs.rs`, `tests/runner.rs`.
  Verify: ключевые сценарии из `tests/test_runner.py` проходят; layout `.codex-worker` совместим по смыслу.

- `[x]` Этап 6. Перенести event projector и live UI.
  Scope: snapshot/timeline/tree и терминальное отображение событий.
  Артефакты: `src/events/projector.rs`, `src/ui/console.rs`, `tests/event_projector.rs`, `tests/console_ui.rs`.
  Verify: портированные сценарии из `tests/test_event_model.py` и `tests/test_console_ui.py` проходят.

- `[x]` Этап 7. Devcontainer, DX и документация.
  Scope: стандартизировать окружение сборки/тестов и запуск в контейнере.
  Артефакты: `.devcontainer/devcontainer.json`, `README.md`.
  Verify: проект открывается в devcontainer без ручной донастройки; воспроизводимы `cargo test`, `cargo run -- run-next --help`, `python3 scripts/acceptance_v1.py`.

- `[x]` Этап 8. Проверка паритета с Python-версией.
  Scope: выполнить acceptance-набор и smoke-сверку статусов, summary и event types.
  Артефакты: `scripts/acceptance_v1.py`, `.github/workflows/acceptance-v1.yml`, `tests/acceptance/manifest.json`, обновлённый раздел parity в документации.
  Verify: Rust-версия проходит согласованный acceptance set и может использоваться как drop-in replacement для `run-next`; политика `required status check` остаётся внешней GitHub-настройкой.

## 1. Цель

Собрать новую Rust-версию `codex-worker` с сохранением ключевого поведения текущей Python-реализации:

- file-based очередь задач в markdown;
- атомарный claim/finalize задач;
- запуск `codex exec --json`;
- нормализация root/subagent событий в единый event log;
- сохранение run artifacts в `.codex-worker`;
- fail-closed логика при некорректном runtime output;
- live console UI;
- покрытие тестами на уровне поведения, а не только модулей.

## 2. Что переносим в первую версию

В scope первой рабочей Rust-версии включаем только то, что уже реально используется в текущем проекте:

1. CLI-команду `run-next`.
2. Markdown task file со статусами `[ ]`, `[>]`, `[x]`, `[!]`.
3. File lock и heartbeat.
4. Recovery stale-running задач.
5. Prompt builder на основе `prompts/orchestrator.md`.
6. Запуск `codex exec` как дочернего процесса.
7. Чтение `stdout`/`stderr`, запись `stdout.jsonl`, `stderr.log`, `events.jsonl`, `summary.json`.
8. Парсинг root events и импорт subagent sessions из `<codex_home>/sessions`.
9. Event projector и live terminal dashboard.
10. Архивацию task file при завершении всех задач.

## 3. Что пока не делаем

Не включаем в первую фазу:

- control plane из архитектурных заметок;
- web UI / Tauri;
- SQLite;
- сетевой API;
- параллельное выполнение нескольких задач одним worker;
- перенос один-в-один внутренних Python-структур, если в Rust можно сделать чище без потери поведения.

## 4. Краткий разбор текущего Python-проекта

Сейчас проект состоит из нескольких чётких подсистем:

- `cli.py`: парсинг аргументов и запуск `run-next`;
- `task_file.py`: парсинг markdown-задач, claim/finalize, archive, stale recovery;
- `lockfile.py`: `fcntl.flock` + payload lock-файла;
- `runner.py`: orchestration loop, subprocess, heartbeat, финализация, failure analysis;
- `events.py` + `event_readers.py`: канонический event model и адаптеры root/subagent событий;
- `event_model.py`: проекция event log в snapshot/timeline/tree;
- `console_ui.py`: live UI поверх projector;
- `logs.py`: layout артефактов run.

Тестовое покрытие уже хорошо задаёт контракт поведения:

- task parsing и recovery;
- fail-closed обработка runtime output;
- импорт subagent sessions;
- классификация connection failures;
- консольное отображение событий;
- summary/timeline/tool/file-change сценарии.

Это хороший кандидат для поэтапного behavioural port.

## 5. Рекомендуемая Rust-архитектура

Предлагаемая структура проекта:

```text
codex-worker-rs/
  Cargo.toml
  PLAN.md
  README.md
  prompts/
    orchestrator.md
  src/
    main.rs
    cli.rs
    config.rs
    error.rs
    util.rs
    models.rs
    task_file.rs
    lockfile.rs
    logs.rs
    runner.rs
    events/
      mod.rs
      record.rs
      payloads.rs
      readers.rs
      projector.rs
    ui/
      mod.rs
      console.rs
  tests/
    cli_run_next.rs
    task_file.rs
    event_readers.rs
    event_projector.rs
    runner.rs
```

## 6. Технические решения

Базовый стек:

- `clap` для CLI;
- `serde` + `serde_json` для JSON/JSONL;
- `thiserror` + `anyhow` для ошибок;
- `regex` для task/event parsing;
- `uuid` для `run_id` и `worker_id`;
- `sha2` для `sha256`/`hash8`;
- `chrono` или `time` для UTC timestamps;
- `tempfile` для atomic write через temp file + rename;
- `nix` для `flock`, `fsync`, POSIX-совместимого low-level FS поведения;
- `crossbeam-channel` или `std::sync::mpsc` для межпоточной передачи stdout/stderr/event ticks;
- `ratatui` + `crossterm` для live terminal UI.

Ключевое архитектурное решение:

- не начинать с async-first дизайна;
- первую версию делать на `std::thread` + blocking I/O, потому что текущая Python-реализация тоже thread-based, а значит проще удержать паритет поведения и упростить диагностику.

## 7. План реализации

### Этап 0. Зафиксировать контракт поведения

Результат:

- создать в Rust-проекте `README.md` с описанием цели и MVP;
- перенести `prompts/orchestrator.md`;
- собрать список Python-тестов и разбить их на обязательные для паритета и второстепенные;
- определить минимальный acceptance set для первого релиза Rust-версии;
- зафиксировать acceptance set в машиночитаемом и воспроизводимом виде;
- добавить в CI отдельный job `acceptance-v1` с явным описанием критериев прохождения.

Критерий готовности:

- есть список сценариев, без которых Rust-версия не считается рабочей;
- acceptance set существует в репозитории и покрывает `claim/finalize`, `heartbeat/recovery`, fail-closed, artifacts и event normalization;
- job `acceptance-v1` уже описан как временная репозиторная проверка и валидирует только acceptance-артефакты (без runtime-исполнения кейсов);
- политика `required status check` для ветки `v1` относится к внешней GitHub-конфигурации и не может быть проверена из этого workspace (нет доступа к remote/config).

### Этап 1. Создать каркас Rust-проекта

Результат:

- `cargo init`;
- CLI с командой `run-next`;
- базовые структуры `WorkerConfig`, `TaskBlock`, `RunSummary`, `EventRecord`;
- error layer и util-функции.

Критерий готовности:

- `cargo test` и `cargo run -- run-next --help` работают;
- проект собирается на хосте (проверка devcontainer выполняется в Этапе 7).

### Этап 2. Перенести file-based task engine

Результат:

- парсер markdown task file;
- генерация `id`;
- сериализация задач;
- `load_snapshot`, `claim_next_task`, `finalize_task`, `refresh_heartbeat`;
- `recover_stale_tasks`;
- `archive_snapshot_if_completed`;
- atomic write с `fsync`.

Критерий готовности:

- портированы тесты из `tests/test_task_file.py`;
- поведение по конфликтам и статусам совпадает с Python-версией.

### Этап 3. Перенести locking

Результат:

- отдельный lock-файл `.<task-file>.lock`;
- `flock(LOCK_EX)`;
- JSON payload lock-файла;
- heartbeat payload update;
- воспроизводимый протокол конкурентной проверки для двух независимых процессов.

Критерий готовности:

- lock-файл корректно создаётся, обновляется и освобождается;
- нет потери atomicity при конкурентном доступе двух процессов;
- интеграционные тесты конкурентности подтверждают отсутствие double-claim, потери статуса и ложного stale recovery.

Проверка:

- тест `concurrent_claim_finalize_two_processes`: два независимых процесса одновременно выполняют `claim_next_task` и `finalize_task` на одном task file;
- тест `heartbeat_recovery_under_load`: конкурентные heartbeat/recovery под нагрузкой не меньше 100 итераций с принудительным устареванием heartbeat у одного процесса;
- инварианты: одна задача не захватывается дважды, `finalize` остаётся атомарным, stale recovery не перехватывает живой lock.

### Этап 4. Перенести event model и readers

Результат:

- Rust-аналог `EventRecord` и typed payloads;
- parser для main `codex exec --json` stream;
- parser для subagent session files;
- нормализация `tool.call`, `tool.result`, `agent.message`, `agent.turn.*`, `error`, `file.change`, `todo.update`, `raw.unparsed`;
- совместимая сериализация в `events.jsonl`.

Критерий готовности:

- портированы тесты из `tests/test_event_readers.py`;
- события из реальных примеров читаются без деградации ключевых полей;
- fail-closed флаги (`saw_thread_started`, `saw_turn_completed`, `final_agent_message`, и т.д.) работают как в Python.

### Этап 5. Перенести runner и artifact pipeline

Результат:

- orchestration loop `run-next`;
- запуск `codex exec`;
- сбор `stdout`/`stderr`;
- запись артефактов в layout, совместимый с текущим `.codex-worker`;
- summary generation;
- failure analysis для connection/auth/network ошибок;
- импорт subagent sessions во время и после выполнения.

Критерий готовности:

- портированы ключевые тесты из `tests/test_runner.py`;
- успешный run завершает задачу, неуспешный переводит в failed;
- структура артефактов совпадает по смыслу и именам файлов.

### Этап 6. Перенести event projector и live UI

Результат:

- snapshot состояния run;
- дерево агентов;
- timeline;
- краткие строки статуса и завершения;
- форматирование tool/file/error событий в терминале.

Критерий готовности:

- портированы тесты из `tests/test_event_model.py` и `tests/test_console_ui.py`;
- UI читаемо показывает root/subagent ход выполнения.

### Этап 7. Devcontainer, DX и документация

Результат:

- `.devcontainer/devcontainer.json` для Rust;
- установка Rust toolchain, `cargo`, `clippy`, `rustfmt`;
- установка `@openai/codex`;
- `CODEX_HOME` mount и совместимый layout окружения;
- краткая документация по запуску и тестам.

Критерий готовности:

- проект открывается в контейнере без ручной донастройки;
- локально можно выполнить `cargo test`, `cargo run -- run-next --help`, `python3 scripts/acceptance_v1.py`.

### Этап 8. Проверка паритета с Python-версией

Результат:

- smoke-набор одинаковых task files и fake-codex сценариев;
- сравнение итогового статуса задач;
- сравнение ключевых полей `summary.json`;
- выборочная сверка `events.jsonl` на уровень event type и обязательных payload fields;
- автоматизируемый acceptance-набор из Этапа 0, запускаемый как обязательный gate `acceptance-v1`.

Критерий готовности:

- Rust-версия проходит agreed acceptance set;
- job `acceptance-v1` запускает runtime acceptance-набор автоматически;
- можно использовать её как drop-in replacement для `run-next` в текущем workflow.

Проверка:

- `acceptance-v1` запускает acceptance-набор автоматически;
- отчёт проверки содержит явный pass/fail по каждому acceptance-кейсу;
- ручные smoke-проверки остаются дополнительными и не заменяют блокирующий gate.

## 8. Порядок выполнения

Рекомендуемый порядок такой:

1. Этап 0: фиксируем acceptance set и критерии паритета.
2. Этапы 1-3: переносим ядро task/lock и закрываем конкурентность.
3. Этап 4 без UI.
4. Этап 5 и runner integration tests.
5. Только после этого Этап 6.
6. Потом Этап 7 и финальная проверка паритета через Этап 8.

Причина проста: UI не должен блокировать перенос ядра worker.

## 9. Основные риски

1. POSIX-специфика `flock` и `fsync`.
2. Отличия буферизации stdout/stderr у дочернего процесса в Rust по сравнению с Python.
3. Различия в regex/slugify логике, из-за которых могут поменяться `task_id`.
4. Сложность точного переноса current event normalization для subagent sessions.
5. Высокая стоимость полного визуального паритета console UI.

Митигация:

- сначала зафиксировать behavioural tests;
- сохранять format compatibility артефактов;
- не рефакторить протокол событий одновременно с переносом языка;
- UI делать последним слоем.

## 10. Предлагаемый первый рабочий срез

Первым инкрементом имеет смысл собрать не весь проект, а такой MVP:

- CLI `run-next`;
- task parsing;
- locking;
- prompt build;
- subprocess run;
- basic event parsing для `thread.started`, `turn.started`, `agent_message`, `turn.completed`, `turn.failed`, `error`, invalid JSON;
- запись `summary.json` и финализация task file;
- без live UI, но с корректными логами.

Это даст ранний end-to-end вертикальный срез и позволит быстрее проверить архитектуру.

## 11. Definition of Done для Rust-версии v1

Считаем первую Rust-версию готовой, когда:

1. `run-next` стабильно выполняет те же task files, что и Python-версия.
2. Артефакты run пригодны для post-mortem и имеют совместимую структуру.
3. Fail-closed сценарии покрыты тестами.
4. Subagent session import работает на реальных примерах.
5. В devcontainer проект собирается и тестируется без ручных обходов.
6. Пройдена блокирующая CI-проверка `acceptance-v1` с полным acceptance set из Этапа 0.
   Политика required-check остаётся внешней GitHub-настройкой и не проверяется из workspace.
7. Пройден воспроизводимый протокол конкурентной проверки для `claim/finalize` и `heartbeat/recovery`.

## 12. Следующий шаг

После утверждения этого плана начинаем с Этапа 0:

- зафиксировать acceptance set;
- описать и завести job `acceptance-v1`;
- после этого перейти к Этапу 1.

Первый технический шаг после завершения Этапа 0:

- инициализировать `cargo`-проект;
- перенести `prompts/orchestrator.md`;
- завести базовый crate layout;
- затем сразу реализовать `task_file` и `lockfile` как основу всего worker runtime.

## 13. Вектор развития после `v1`

Ниже зафиксирован roadmap следующего уровня. Он не входит в текущий план переноса Python-воркера на Rust, но задаёт направление развития после достижения паритета `v1`.

Цель следующей фазы:

- превратить локальный file-based worker в управляемый runtime-компонент;
- отделить источник задач от локального markdown task file;
- сделать worker наблюдаемым и управляемым извне;
- подготовить почву для control plane и operator UI.

## 14. План развития после `v1`

### Этап F1. Внешнее управление worker

Добавить слой внешних команд для управления жизненным циклом обработки.

Функции:

- запуск обработки;
- остановка обработки после завершения текущей задачи;
- немедленная остановка worker-процесса;
- pause/resume приёма новых задач;
- статус worker: `idle`, `running`, `paused`, `stopping`, `stopped`, `error`;
- heartbeat и признак liveness/readiness.

Возможная форма реализации:

- локальный HTTP API или Unix socket control endpoint;
- файловая control-команда как временный fallback;
- отдельный worker state directory с `worker.json`, `heartbeat.json`, `current-run.json`, `commands.jsonl`.

Минимальные внешние команды:

- `start`;
- `stop_graceful`;
- `stop_now`;
- `pause`;
- `resume`;
- `retry_task`;
- `cancel_run`.

Критерий готовности:

- worker можно безопасно остановить и снова запустить без ручного вмешательства в task artifacts;
- внешняя команда не ломает атомарность claim/finalize.

### Этап F2. Внешняя очередь задач

Уйти от единственного локального markdown task file как от канонического источника задач.

Цель:

- поддерживать внешний producer задач;
- развязать постановку задач и их выполнение;
- подготовить несколько worker-процессов и разные workspace.

Возможные варианты очереди:

- file-based control-plane queue;
- SQLite как индекс и очередь;
- HTTP API поверх file/SQLite storage.

Рекомендуемый путь:

1. Сначала сделать file-based external queue.
2. Затем при росте нагрузки вынести индекс и query layer в SQLite.
3. API добавлять поверх уже стабильной модели задач и lease.

Минимальная модель:

- `task.created`;
- `task.queued`;
- `task.claimed`;
- `task.completed`;
- `task.failed`;
- `task.requeued`;
- `task.cancel_requested`.

Что должно появиться:

- уникальный `task_id` вне markdown-файла;
- очередь с приоритетом и временем создания;
- lease-модель для нескольких worker;
- retry policy;
- routing по `workspace`, `project`, `capabilities`, `tags`.

Критерий готовности:

- задачи можно подавать извне без прямого редактирования локального task file;
- несколько worker могут конкурировать за одну очередь без двойного claim.

### Этап F3. Отправка логов и телеметрии

Добавить не только локальное хранение артефактов, но и внешнюю публикацию operational signals.

Что отправляем наружу:

- run lifecycle events;
- worker lifecycle events;
- queue metrics;
- ошибки и failure classification;
- audit trail операторских команд;
- summary по каждому run.

Что остаётся локально:

- `prompt.md`;
- `stdout.jsonl`;
- `stderr.log`;
- `events.jsonl`;
- `summary.json`;
- subagent artifacts.

Принцип:

- локальные файлы остаются первичным источником истины;
- внешняя отправка является производной репликацией;
- потеря внешнего telemetry sink не должна ломать run.

Каналы доставки:

- JSONL append-only export;
- HTTP batch export;
- OpenTelemetry / OTLP как следующий шаг;
- webhook/event sink для control plane.

Минимальный набор телеметрии:

- `worker.started`;
- `worker.heartbeat`;
- `worker.paused`;
- `worker.stopped`;
- `task.claimed`;
- `run.started`;
- `run.completed`;
- `run.failed`;
- `operator.command.received`;
- `operator.command.applied`.

Критерий готовности:

- можно наблюдать состояние worker и run без чтения локальной файловой системы;
- при недоступности внешнего sink worker продолжает выполнять задачи.

### Этап F4. Control Plane

После появления внешнего управления, очереди и телеметрии можно выделять отдельный control plane.

Control plane должен отвечать за:

- реестр проектов;
- реестр workspace и worker;
- очередь и назначение задач;
- операторские команды;
- агрегированное состояние для UI;
- аудит ручных действий.

На этом этапе `codex-worker-rs` становится не просто CLI, а исполнителем в более широкой системе orchestration.

### Этап F5. UI оператора

Последним крупным слоем можно строить UI.

Минимальные сценарии:

- посмотреть очередь;
- увидеть online/offline workers;
- остановить или возобновить worker;
- отменить run;
- переоткрыть failed task;
- провалиться в timeline, logs и artifacts;
- видеть причины сбоев и текущую стадию выполнения.

## 15. Архитектурные принципы для будущего развития

Чтобы пост-`v1` развитие не потребовало переделывать ядро worker, стоит держать такие ограничения:

- worker runtime не должен зависеть от конкретного транспорта control plane;
- команды управления должны быть идемпотентными;
- queue state и run artifacts должны быть разделены;
- локальные artifacts должны оставаться пригодными для post-mortem без внешних сервисов;
- telemetry export должен быть best-effort;
- при потере control plane worker должен уметь корректно завершить уже захваченную задачу.

## 16. Приоритет следующей фазы

После завершения `v1` разумный порядок такой:

1. Внешнее управление worker.
2. Внешняя очередь задач.
3. Отправка логов и телеметрии.
4. Control plane.
5. UI оператора.

Такой порядок снижает риск: сначала делаем worker управляемым, потом подключаем его к общей системе.
