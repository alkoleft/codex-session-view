# План по доработке обработки событий

Этот документ фиксирует рабочий план по проблемам, которые не стоит решать локальными правками без согласования общей модели событий.

Сейчас план покрывает две темы:

- корректная иерархия для nested subagents;
- policy переходов статуса при асинхронном импорте session file.

## Контекст

В текущей модели уже есть два разных слоя:

- `control-plane` события из `stdout.jsonl`
- `session-detail` события из `subagents/<thread_id>.jsonl`

Для простого случая `root -> subagent` этого достаточно. Но при сценарии `subagent -> subagent` и при несинхронном приходе control-plane и session-detail событий появляются более сложные вопросы:

- кто считается непосредственным родителем дочерней сессии;
- какой источник истины отвечает за текущее состояние агента;
- можно ли поздним session-detail событием понизить уже достигнутый terminal state.

## Тема 1. Nested subagents

### Проблема

Для вложенных субагентов нужно надёжно различать:

- корневой thread текущего run;
- непосредственного родителя дочерней сессии;
- thread, к которому относится конкретная импортированная строка session file.

Риск текущей модели:

- часть поздних session-level событий может наследовать `parent_thread_id` от root run, а не от реального ближайшего parent.

### Цель

Нужно добиться, чтобы при сценарии `subagent -> subagent`:

- `agent.session` открывал правильный parent-child edge;
- все последующие tool/message/meta события этой же дочерней сессии использовали того же parent;
- live UI и post-mortem анализ видели одну и ту же иерархию.

### Предлагаемый подход

1. Ввести явное session-level состояние для imported subagent session:
   - `thread_id`
   - `resolved_parent_thread_id`
   - `forked_from_id`
   - `session_path`

2. Разрешать parent с приоритетом:
   - `payload.source.subagent.thread_spawn.parent_thread_id`
   - уже установленный `resolved_parent_thread_id` для этого `thread_id`
   - fallback на parent root run только как временную заглушку

3. После первого валидного `session_meta` закреплять parent за конкретной imported session.

4. Все последующие `tool.call`, `tool.result`, `agent.message`, `agent.meta` из этой session привязывать уже к закреплённому parent, а не к текущему root parent аргументу.

### Открытые вопросы

- Нужно ли разрешать переопределение parent, если поздний `session_meta` противоречит уже закреплённому parent.
- Нужно ли отдельно хранить `observed_parent_thread_id` и `resolved_parent_thread_id`.
- Нужен ли явный relation `spawned_by_subagent`, отличный от root-level `spawned_subagent`.

### Минимальный набор тестов

- импорт session file, где subagent запускает ещё одного subagent;
- проверка, что дочерний subagent получает непосредственного parent, а не root;
- проверка, что tool/message/meta события nested child продолжают использовать тот же parent;
- проверка, что foreign `session_meta` из forked history не ломает иерархию.

## Тема 2. Policy переходов статуса

### Проблема

Сейчас control-plane и session-detail события приходят асинхронно. Поэтому возможна ситуация:

1. control-plane уже дал terminal status;
2. позже импортируется `agent.session` или другая ранняя session-detail запись;
3. статус в UI или snapshot откатывается в менее точное или менее terminal состояние.

### Цель

Нужна явная policy, которая определяет:

- какие статусы terminal;
- может ли более позднее событие понижать статус;
- какой источник приоритетнее для каждого класса переходов.

### Предлагаемый подход

1. Разделить status update на два слоя:
   - `lifecycle_status`
   - `session_loaded_flag` или аналогичный orthogonal marker

2. Не использовать `session_loaded` как замену lifecycle-status.

3. Ввести частичный порядок статусов, например:
   - `idle < pending_init < session_loaded < running < working < completed/failed/cancelled`

4. Запретить downgrade из terminal-state в non-terminal-state.

5. Для `agent.session` обновлять метаданные агента:
   - nickname
   - role
   - cwd
   - parent
   - session_loaded_flag
   Но не затирать terminal lifecycle-status.

### Открытые вопросы

- Нужен ли отдельный статус `session_loaded`, если он фактически технический, а не пользовательский.
- Должен ли `working` иметь больший приоритет, чем `running`.
- Что делать, если control-plane говорит `completed`, а session-detail ещё продолжает приносить tool events.

### Минимальный набор тестов

- terminal status из control-plane не откатывается после позднего `agent.session`;
- `task_complete` закрепляет terminal state;
- поздний `tool.call` после terminal state не ломает статусную модель;
- metadata из `agent.session` обновляется даже без изменения lifecycle-status.

## Порядок работы

1. Зафиксировать session-level parent resolution model.
2. Реализовать закрепление parent на уровне imported session state.
3. Ввести policy переходов статуса без downgrade из terminal-state.
4. Добавить тесты на nested subagents и late session import.
5. После стабилизации иерархии вернуться к дедупликации message-layer и корректной семантике счётчиков.

## Явно не входит в текущий этап

- устранение всех потенциальных дублей сообщений;
- пересчёт `tool_counts` и `subagent_counts` в логические операции;
- редизайн timeline/UI beyond status correctness.
