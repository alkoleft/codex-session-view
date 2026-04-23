## ADDED Requirements

### Requirement: Project metrics selector exposes the full project catalog
Экран `Project metrics` SHALL показывать все проекты, известные backend catalog для метрик, а не
только проекты из текущей загруженной страницы session list.

#### Scenario: Project is missing from the current session list page
- **WHEN** проект присутствует в индексированном catalog backend, но его сессии не попали в текущую страницу `listIndexedSessions`
- **THEN** этот проект MUST оставаться доступным в selector экрана `Project metrics`

### Requirement: System classifies project-metrics sessions by scope
Система SHALL различать project-metrics сессии как минимум по scope `main` и `subsession`, опираясь
на session metadata, пригодный для одинакового использования в catalog и project metrics query.

#### Scenario: Indexed summary contains subagent spawn metadata
- **WHEN** indexed session metadata указывает, что сессия была запущена как subagent/subsession
- **THEN** project catalog и project metrics response MUST классифицировать такую сессию как `subsession`

#### Scenario: Indexed summary does not contain subagent spawn metadata
- **WHEN** indexed session metadata не содержит признаков subagent/subsession запуска
- **THEN** система MUST классифицировать такую сессию как `main` или явно пометить как degraded/unknown, но не смешивать это молча

### Requirement: User can filter project metrics by session scope
Экран `Project metrics` SHALL предоставлять фильтр scope со значениями `all`, `main` и
`subsession`.

#### Scenario: Filtering only main sessions
- **WHEN** пользователь выбирает фильтр `main`
- **THEN** экран MUST загружать и показывать только основные сессии выбранного проекта

#### Scenario: Filtering only subsessions
- **WHEN** пользователь выбирает фильтр `subsession`
- **THEN** экран MUST загружать и показывать только дочерние subagent/subsession сессии выбранного проекта

### Requirement: Scope filter drives the whole project metrics payload
Фильтр scope SHALL одинаково влиять на summary cards, charts и contributing sessions list, чтобы
все секции экрана отражали один и тот же набор сессий.

#### Scenario: Switching the scope filter after data was loaded
- **WHEN** пользователь меняет фильтр между `all`, `main` и `subsession`
- **THEN** backend query MUST использовать выбранный scope
- **THEN** summary cards, chart series и contributing sessions list MUST обновляться согласованно по одному и тому же filtered dataset

### Requirement: Filtered project metrics preserve explicit empty and degraded states
Экран `Project metrics` SHALL явно показывать, когда после применения scope filter данных нет или
когда часть сессий не может быть надёжно классифицирована.

#### Scenario: No sessions for selected project and scope
- **WHEN** project metrics query завершается успешно, но после применения выбранного scope не остаётся ни одной сессии
- **THEN** экран MUST показать empty state с объяснением, что данных для выбранного project/scope нет

#### Scenario: Some sessions remain unclassified
- **WHEN** backend знает, что часть contributing sessions не может быть надёжно отнесена к `main` или `subsession`
- **THEN** экран MUST явно сообщить об этом пользователю
- **THEN** такие записи MUST оставаться видимыми в режиме `all` и не должны молча искажать узкие фильтры
