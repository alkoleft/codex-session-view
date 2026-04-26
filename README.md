# codex-session-view

`codex-session-view` это монорепозиторий для работы с логами Codex-сессий. Это не инструмент запуска `codex`; текущий scope ограничен чтением, нормализацией и просмотром логов.

- `crates/codex-log` — Rust-библиотека для чтения, нормализации, replay и проекции логов;
- `crates/codex-session-explorer-backend` — общий Rust backend-слой viewer без зависимости от Tauri runtime;
- `apps/codex-session-explorer` — desktop-приложение `codex-session-explorer` для интерактивного просмотра этих логов.
- `apps/codex-session-explorer-http` — локальный HTTP backend, который раздаёт встроенный frontend `codex-session-explorer` для browser-based запуска.

## Скриншоты

### Окно выбора сессии

![Окно выбора сессии codex-session-explorer](./docs/screenshots/codex-session-explorer-real-session-picker.png)

### Выбранная сессия

![Выбранная сессия codex-session-explorer](./docs/screenshots/codex-session-explorer-real-selected-session.png)

### Сессия со spawned agents

![Сессия со spawned agents в codex-session-explorer](./docs/screenshots/codex-session-explorer-spawned-agents.png)

### Project metrics

![Project metrics в codex-session-explorer](./docs/screenshots/codex-session-project-metrics.png)

Проект больше не содержит worker-runtime, root bin или task-runner сценариев. Вся активная разработка сосредоточена на логовом ядре и viewer.

## Быстрый старт

```bash
cargo test --workspace
npm --prefix apps/codex-session-explorer test
npm --prefix apps/codex-session-explorer run build
cargo build -p codex-session-explorer-http
```

Для browser-based варианта production flow такой:

```bash
npm --prefix apps/codex-session-explorer run build
cargo run -p codex-session-explorer-http -- --port 4321
```

По умолчанию HTTP app пытается определить `CODEX_HOME` автоматически; при необходимости можно явно передать `--codex-home /path/to/.codex`.

## Структура репозитория

- `crates/codex-log` содержит source of truth для `EventRecord`, readers, session catalog, replay и tree/view-model.
- `crates/codex-session-explorer-backend` содержит переиспользуемую доменную логику viewer для desktop и HTTP runtime.
- `apps/codex-session-explorer/src-tauri` теперь является thin adapter над общим backend-слоем.
- `apps/codex-session-explorer-http` поднимает same-origin `/api/viewer/*` и раздаёт встроенный frontend.
- `docs/` фиксирует актуальную модель логов, архитектуру и roadmap будущего CLI.

## Аналоги

| Проект | Ссылка | Тип | Поддержка Codex | Основные возможности | Ограничения / замечания | Оценка релевантности |
|---|---|---|---|---|---|---|
| agentsview | https://github.com/wesm/agentsview | Аналитика и визуализация сессий | Да | Full-text search по сообщениям, token/cost dashboard, activity heatmaps, tool usage, velocity metrics, project breakdowns, live updates | Поддерживает несколько агентных систем, не сфокусирован только на Codex | Очень высокая |
| Codex Viz | https://github.com/caua68/codex-viz | Локальный dashboard | Да | Trends, token usage, tool insights по истории сессий Codex CLI | Скорее dashboard по истории, чем глубокая трассировка исполнения | Очень высокая |
| Agent Sessions | https://github.com/jazzyalex/agent-sessions | Session browser / operational UI | Да | Поиск и фильтрация сессий, transcript view, resume workflow, live HUD, usage/rate-limit tracking | Сильнее как operational-интерфейс, чем как аналитическая система | Высокая |
| CodexMonitor | https://github.com/Cocoanetics/CodexMonitor | Мониторинг и просмотр сессий | Да | Чтение `~/.codex/sessions`, просмотр недавних сессий и сообщений, поддержка Codex CLI и VS Code extension | Больше inspection/monitoring, чем аналитика | Средняя |
| codex-session-view | https://github.com/AcidicSoil/codex-session-view | Trace/session viewer | Да | Visualizing, analyzing, debugging AI coding agent sessions, interactive timeline of execution traces | Нишевый и менее зрелый проект | Высокая |
| AI Agent Session Center | https://github.com/coding-by-feng/ai-agent-session-center | Real-time dashboard / orchestration | Да | Prompt history, tool logs, live terminals, визуализация активных сессий | Больше про live-операции и orchestration, чем про post-hoc аналитику | Средняя |
| VibeBar | https://github.com/yelog/vibebar | Menu bar monitoring | Да | Live TUI session activity, token usage trend, мониторинг CLI-агентов | Лёгкий мониторинг, без глубокой аналитики | Средняя |
| GitHub Copilot Agent Sessions view | https://docs.github.com/en/copilot/concepts/agents/openai-codex | IDE session management | Частично | Просмотр running tasks, progress UI для OpenAI Codex в VS Code / Copilot workflow | Не является самостоятельной системой аналитики истории сессий | Низкая |
| Codex CLI (официальные возможности) | https://developers.openai.com/codex/cli/features | Официальный CLI/UI | Базово | Interactive terminal UI, workflows beyond chat | Нет явной встроенной полноценной аналитики сессий, cost dashboard и исторической визуализации | Низкая |

## Что дальше

Следующий архитектурный этап уже зарезервирован в документации: отдельный CLI для чтения логов и отправки их в базу данных. В этом репозитории он пока не реализован и не имеет crate/bin.
