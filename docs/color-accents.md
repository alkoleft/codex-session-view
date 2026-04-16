# Карта цветовых акцентов

Документ фиксирует, какие акцентные цвета используются в проекте и для каких сценариев они
применяются. По текущей реализации цвет в основном включается не для обвязки интерфейса, а для
семантики событий, статусов и режимов задач.

Визуальные плашки ниже сделаны через inline HTML в Markdown. Если конкретный Markdown renderer
режет inline styles, текстовые значения всё равно остаются каноническими.

## Источники

- `apps/codex-session-explorer/src/index.css`
- `apps/codex-session-explorer/src/components/session-event-list.tsx`
- `apps/codex-session-explorer/src/components/session-event-list-common.tsx`
- `apps/codex-session-explorer/src/components/session-event-list-shell-card.tsx`
- `apps/codex-session-explorer/src/components/session-event-list-patch-card.tsx`
- `apps/codex-session-explorer/src/components/session-event-list-plan-card.tsx`
- `apps/codex-session-explorer/src/components/session-event-list-user-input-card.tsx`
- `apps/codex-session-explorer/src/components/agents-panel.tsx`
- `crates/codex-log/src/events/projector.rs`

## 1. Базовые UI-токены `codex-session-explorer`

| Токен | Светлая тема | Тёмная тема | Назначение |
| --- | --- | --- | --- |
| `--accent` | `oklch(0.97 0 0)` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:oklch(0.97 0 0)"></span> | `oklch(0.269 0 0)` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:oklch(0.269 0 0)"></span> | Нейтральный акцент для спокойных служебных подписей и приглушённых UI-элементов. |
| `--accent-soft` | `#e5e7eb` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#e5e7eb"></span> | `#262626` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#262626"></span> | Мягкая нейтральная подложка; сейчас используется как резервный мягкий accent token. |
| `--accent-strong` | `#374151` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#374151"></span> | `#d4d4d4` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#d4d4d4"></span> | Главный структурный акцент: иконки секций, выбранные состояния, ссылки `show/hide`, левый маркер subagent-карточек. |
| `--rose` | `#991b1b` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#991b1b"></span> | `#fda4af` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#fda4af"></span> | Локальный `error` accent, явно используется в debug panel. |
| `--emerald` | `#065f46` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#065f46"></span> | `#34d399` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#34d399"></span> | Резервный `success` accent token; в карточках чаще используются Tailwind-классы `emerald-*`. |
| `--plum` | `#374151` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#374151"></span> | `#c4b5fd` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#c4b5fd"></span> | Резервный `violet/plum` accent token; прямое применение сейчас минимальное. |
| `--primary` | `oklch(0.205 0 0)` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:oklch(0.205 0 0)"></span> | `oklch(0.922 0 0)` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:oklch(0.922 0 0)"></span> | Нейтральный shadcn `primary` для default `Button` и `Badge`. |
| `--secondary` | `oklch(0.97 0 0)` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:oklch(0.97 0 0)"></span> | `oklch(0.269 0 0)` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:oklch(0.269 0 0)"></span> | Нейтральный `secondary` для secondary `Button` и `Badge`. |
| `--destructive` | `oklch(0.577 0.245 27.325)` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:oklch(0.577 0.245 27.325)"></span> | `oklch(0.704 0.191 22.216)` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:oklch(0.704 0.191 22.216)"></span> | Разрушительное действие, `invalid state`, destructive badges/buttons. |
| `--ring` | `oklch(0.708 0 0)` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:oklch(0.708 0 0)"></span> | `oklch(0.556 0 0)` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:oklch(0.556 0 0)"></span> | `focus ring`. |

## 2. Семантические акценты event list

Эта палитра управляет цветом точки и заголовка `event card`. В колонке `Цвет` показан
репрезентативный цвет семейства.

| Семантика | Цвет | Где применяется |
| --- | --- | --- |
| `message` | `sky` (`#0ea5e9`) <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#0ea5e9"></span> | Все `message.*`, `message role chip`, часть встроенных `status`-чипов с активным состоянием. |
| `shell` | `amber` (`#f59e0b`) <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#f59e0b"></span> | Все `shell.*`, `shell search actions`, `unknown shell status`, предупреждения вокруг shell-процессов. |
| `plan` | `cyan` (`#06b6d4`) <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#06b6d4"></span> | `todo.update`, `plan/update` карточки, `running-like` служебные состояния. |
| `patch` | `rose` (`#f43f5e`) <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#f43f5e"></span> | `patch.apply`, `patch header tone`. |
| `tokens` | `emerald` (`#10b981`) <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#10b981"></span> | `info.tokens`, `completed/success-like` counters и маркеры. |
| `collab` | `violet` (`#8b5cf6`) <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#8b5cf6"></span> | `collab.*`, `user.input.request`, orchestration-related UI. |
| `meta` | `slate` (`#64748b`) <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#64748b"></span> | `agent.reasoning`, `runtime.context`, `agent.meta`, `agent.session*`, `context.compacted*`. |
| `task` | `blue` (`#3b82f6`) <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#3b82f6"></span> | `task.*`, `agent.failed`, `agent.aborted` как часть семейства `task lifecycle`. |
| `error` | `rose` (`#f43f5e`) <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#f43f5e"></span> | `error`, `stderr.line`, `failure-like event cards`. |
| `generic` | `foreground` (`#6b7280`) <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#6b7280"></span> | Остальные карточки без явной `semantic accent palette`. |

## 3. Карта режимов задач

Эта палитра используется для `TaskModeBadge`, `TaskLifecycleMarker`, линий `lifecycle` и `task
surface` в React viewer. HTML reference использует ту же семантику, но с отдельной мягкой
подложкой.

| Режим | Акцент | React surface | HTML soft fill | Для чего |
| --- | --- | --- | --- | --- |
| `default` | `#2563eb` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#2563eb"></span> | `rgba(37, 99, 235, 0.05)` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:rgba(37, 99, 235, 0.05)"></span> | `#dbeafe` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#dbeafe"></span> | Базовый режим задачи. |
| `plan` / `planning` | `#d97706` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#d97706"></span> | `rgba(217, 119, 6, 0.06)` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:rgba(217, 119, 6, 0.06)"></span> | `#fef3c7` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#fef3c7"></span> | Планирование. |
| `review` / `reviewer` | `#be123c` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#be123c"></span> | `rgba(190, 18, 60, 0.06)` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:rgba(190, 18, 60, 0.06)"></span> | `#ffe4e6` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#ffe4e6"></span> | Ревью и проверка. |
| `implementation` / `worker` | `#0f766e` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#0f766e"></span> | `rgba(15, 118, 110, 0.06)` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:rgba(15, 118, 110, 0.06)"></span> | `#ccfbf1` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#ccfbf1"></span> | Реализация. |
| `approval` | `#7c3aed` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#7c3aed"></span> | `rgba(124, 58, 237, 0.06)` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:rgba(124, 58, 237, 0.06)"></span> | `#ede9fe` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#ede9fe"></span> | Согласование и approval. |

### Fallback palette для неизвестного `task mode`

Если режим не входит в фиксированный список, цвет выбирается по hash из пула:

| Акцент | React surface | HTML soft fill |
| --- | --- | --- |
| `#2563eb` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#2563eb"></span> | `rgba(37, 99, 235, 0.05)` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:rgba(37, 99, 235, 0.05)"></span> | `#dbeafe` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#dbeafe"></span> |
| `#7c3aed` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#7c3aed"></span> | `rgba(124, 58, 237, 0.06)` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:rgba(124, 58, 237, 0.06)"></span> | `#ede9fe` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#ede9fe"></span> |
| `#0891b2` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#0891b2"></span> | `rgba(8, 145, 178, 0.06)` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:rgba(8, 145, 178, 0.06)"></span> | `#cffafe` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#cffafe"></span> |
| `#d97706` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#d97706"></span> | `rgba(217, 119, 6, 0.06)` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:rgba(217, 119, 6, 0.06)"></span> | `#fef3c7` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#fef3c7"></span> |
| `#16a34a` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#16a34a"></span> | `rgba(22, 163, 74, 0.06)` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:rgba(22, 163, 74, 0.06)"></span> | `#dcfce7` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#dcfce7"></span> |
| `#be123c` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#be123c"></span> | `rgba(190, 18, 60, 0.06)` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:rgba(190, 18, 60, 0.06)"></span> | `#ffe4e6` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#ffe4e6"></span> |

## 4. Локальные semantic colors внутри карточек

В колонке `Цвет` показан репрезентативный цвет семейства.

| Сценарий | Цвет | Значение |
| --- | --- | --- |
| `PlanStepStatusBadge: completed` | `emerald` (`#10b981`) <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#10b981"></span> | Шаг плана завершён. |
| `PlanStepStatusBadge: in_progress` | `sky` (`#0ea5e9`) <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#0ea5e9"></span> | Шаг плана выполняется сейчас. |
| `PlanStepStatusBadge: pending` | `amber` (`#f59e0b`) <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#f59e0b"></span> | Шаг плана ещё не начат. |
| `Shell/Patch status success` | `emerald` (`#10b981`) <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#10b981"></span> | Успешное завершение операции. |
| `Shell/Patch status failure` | `rose` (`#f43f5e`) <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#f43f5e"></span> | Ошибка или неуспешное завершение. |
| `Shell/Patch status unknown` | `amber` (`#f59e0b`) <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#f59e0b"></span> | Нет итогового результата или статус промежуточный. |
| `Shell parsed command read` | `sky` (`#0ea5e9`) <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#0ea5e9"></span> | Команда чтения. |
| `Shell parsed command search` | `amber` (`#f59e0b`) <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#f59e0b"></span> | Команда поиска. |
| `Shell parsed command list_files` | `emerald` (`#10b981`) <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#10b981"></span> | Команда листинга файлов. |
| `Shell parsed command write` | `rose` (`#f43f5e`) <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#f43f5e"></span> | Команда записи или изменения. |
| `Shell parsed command other` | `cyan` (`#06b6d4`) <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#06b6d4"></span> | Прочие команды. |
| `Patch diff hunk header @@` | `sky` (`#0ea5e9`) <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#0ea5e9"></span> | Заголовок `hunk`. |
| `Patch diff addition +` | `emerald` (`#10b981`) <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#10b981"></span> | Добавление строк. |
| `Patch diff deletion -` | `rose` (`#f43f5e`) <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#f43f5e"></span> | Удаление строк. |
| `Patch diff file header` | `amber` (`#f59e0b`) <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#f59e0b"></span> | Заголовок файла и patch-служебные строки. |
| `User input selected option / answer` | `emerald` (`#10b981`) <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#10b981"></span> | Выбранный пользователем вариант. |
| `Agent status completed` | `emerald` (`#10b981`) <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#10b981"></span> | Агент завершил работу. |
| `Agent status running` | `cyan` (`#06b6d4`) <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#06b6d4"></span> | Агент активен. |
| `Agent status waiting` | `indigo` (`#6366f1`) <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#6366f1"></span> | Агент ждёт внешний результат. |
| `Agent status closed` | `sky` (`#0ea5e9`) <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#0ea5e9"></span> | Агент штатно закрыт. |
| `Agent status aborted` | `amber` (`#f59e0b`) <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#f59e0b"></span> | Агент остановлен вручную или внешним сигналом. |
| `Agent status failed` | `rose` (`#f43f5e`) <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#f43f5e"></span> | Агент завершился с ошибкой. |

## 5. Консольная палитра Rust UI

### Категории `summary/event lines`

| Категория | ANSI-цвет | Для чего |
| --- | --- | --- |
| `Assistant` | `bold green` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#16a34a"></span> | Ответы ассистента. |
| `Command` | `bold blue` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#2563eb"></span> | Shell/tool-команды. |
| `Search` | `bold cyan` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#06b6d4"></span> | Web/search активности. |
| `Subagent` | `bold magenta` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#d946ef"></span> | Субагенты и их линии. |
| `File` | `bold yellow` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#ca8a04"></span> | Файловые изменения. |
| `Todo` | `bold cyan` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#06b6d4"></span> | Обновления плана. |
| `Error` | `bold red` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#dc2626"></span> | Ошибки. |

### Default `kind palette`

| Kind/status | ANSI-цвет |
| --- | --- |
| `claim`, `start`, `idle`, `info`, `default` | `cyan` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#06b6d4"></span> |
| `completed` | `green` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#16a34a"></span> |
| `failed` | `red` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#dc2626"></span> |
| `warning`, `file` | `yellow` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#ca8a04"></span> |
| `fallback` | `white` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#ffffff"></span> |

### `Status chip palette`

| Статус run/task | ANSI-цвет |
| --- | --- |
| `idle`, `claimed`, `running` | `bold cyan` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#06b6d4"></span> |
| `completed` | `bold green` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#16a34a"></span> |
| `failed` | `bold red` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#dc2626"></span> |
| `fallback` | `bold white` <span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#ffffff"></span> |

## 6. Палитра субагентов

Для субагентских thread в `EventProjector` назначается отдельный `hex color`. Сейчас этот цвет
напрямую используется прежде всего в консольном префиксе `subagent[...]`, чтобы разные thread
визуально не сливались.

Пул цветов:

<span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#e76f51"></span> `#e76f51`
<span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#f4a261"></span> `#f4a261`
<span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#e9c46a"></span> `#e9c46a`
<span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#90be6d"></span> `#90be6d`
<span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#43aa8b"></span> `#43aa8b`
<span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#4d908e"></span> `#4d908e`
<span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#577590"></span> `#577590`
<span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#277da1"></span> `#277da1`

<span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#9b5de5"></span> `#9b5de5`
<span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#f15bb5"></span> `#f15bb5`
<span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#ff006e"></span> `#ff006e`
<span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#fb5607"></span> `#fb5607`
<span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#3a86ff"></span> `#3a86ff`
<span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#06d6a0"></span> `#06d6a0`
<span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#8ecae6"></span> `#8ecae6`
<span style="display:inline-block;width:0.9em;height:0.9em;vertical-align:-0.08em;border:1px solid #94a3b8;border-radius:3px;background:#bc6c25"></span> `#bc6c25`

Правила применения:

- Один `thread` получает один цвет на время жизни агента.
- После закрытия агента цвет возвращается в пул.
- При повторном появлении того же активного `thread` используется уже закреплённый цвет.

## 7. Итог

Главный принцип палитры сейчас такой:

- обвязка интерфейса нейтральная и почти ахроматическая;
- яркие цвета включаются в основном для семантики событий и статусов;
- `task mode` и `subagent color` используются для навигации по orchestration-потоку, а не для
  декоративного оформления.
