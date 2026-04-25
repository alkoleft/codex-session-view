## Context

Экран `Project metrics` в `codex-session-explorer` уже прошёл несколько итераций: отдельный screen,
chart-first layout, аналитические chart modes, split-view с независимыми scroll-контейнерами и
inspector. При этом текущая композиция всё ещё смешивает несколько моделей взаимодействия:
отдельный screen header, stage header, overview cards, alerts, chart toolbar, side inspector,
`Used skills` и `Contributing sessions`.

В результате график формально доминирует по площади, но экран не даёт того ощущения лёгкого
аналитического инструмента, которое нужно для ежедневной работы. В обсуждении зафиксирован более
жёсткий UX-контракт:

- `grain` графика: одна точка = одна сессия;
- график используется как инструмент поиска корреляций, поэтому одновременно показывает несколько
  метрик из разных семейств;
- все значения на графике всегда нормализуются одним глобальным методом для всего chart state;
- основное взаимодействие: hover только для tooltip, click фиксирует `pinned` point;
- near-graph summary должен быть одной компактной строкой над графиком;
- справа остаётся только tabbed side panel c `Window pulse`, `Anomalies`, `Series`, `Chart`,
  `Pinned session`;
- `scope` и `includeSpawnAgents` живут в глобальном header рядом с `project` и `period`;
- zoom и navigation должны жить на самом графике, а не отдельными controls над ним;
- у графика есть фиксированная visual grammar: одна `primary` metric доминирует как bar-layer,
  а остальные `secondary` series рисуются приглушёнными lines;
- аналитический режим задаётся глобальным default, но может переопределяться по каждой метрике
  компактными badges; `raw values` также включаются по каждой метрике отдельно;
- режим обработки выбросов остаётся одним глобальным chart setting;
- аномалии должны классифицироваться по смысловым классам, а не смешиваться с quality flags в
  одном неразличимом списке;
- на самом chart не должно быть persistent anomaly markers; anomaly discovery живёт в side panel и
  compact indicators около графика;
- нельзя менять backend contract `query_project_metrics`.

Текущее ограничение данных: `ProjectMetricsResponse` уже даёт метрики по сессиям и `agent_role`,
но не несёт отдельного поля со стартовым пользовательским запросом в payload project metrics.
Пользователь дополнительно зафиксировал, что для `Pinned session` нужен отдельный lazy-fetch path.
Следовательно, change не расширяет `query_project_metrics`, а добавляет отдельный lightweight
backend endpoint с materialized detail storage в той же SQLite-базе `session-metrics.sqlite`.

## Goals / Non-Goals

**Goals:**

- Зафиксировать экран как быстрый аналитический workspace, где график — основной рабочий
  инструмент, а не один из нескольких равноправных блоков.
- Разрешить полную замену текущей реализации экрана без сохранения совместимости со старой
  внутренней UI/state-композицией, если это проще и чище по архитектуре.
- Зафиксировать график как correlation-first canvas, на котором пользователь всегда видит
  одновременно несколько нормализованных метрик из разных семейств.
- Свести верхнюю композицию к одному глобальному header без локальных screen/stage header-блоков.
- Сохранить split-view и независимые scroll owners для main area и side panel.
- Перевести point inspection на модель `hover tooltip + pinned by click`.
- Сделать near-graph summary компактной однострочной сводкой pinned session без длинного текста.
- Перенести управление видимостью рядов в tab `Series`, а аналитические chart controls — в tab
  `Chart`.
- Ввести фиксированную иерархию `primary` / `secondary`, где primary доминирует визуально, а
  secondary остаются на графике приглушёнными, но могут быть временно или явно усилены.
- Сохранить display type графика фиксированным и не добавлять отдельный switch типа графика.
- Разрешить per-metric override аналитического режима и `raw values`, не превращая их в тяжёлую
  матрицу настроек.
- Сохранить `Window pulse` как первый tab side panel для quick summary текущего видимого window и
  без возврата summary blocks в main area.
- Вынести anomaly exploration в отдельный tab `Anomalies` с группировкой, collapse и явным
  indicator в заголовке вкладки.
- Сделать `Pinned session` tab полным правдивым источником по выбранной pinned-сессии: метрики,
  `agent_role`, flags, идентификатор и стартовый пользовательский запрос.
- Разделить anomaly classes на `Metric`, `Baseline`, `Session`, `Data issue`, используя
  `change-point / baseline shift` как отдельный baseline-класс.
- Упростить zoom/navigation UX до выделения региона и overlay-кнопок поверх графика.
- Сохранить текущие chart modes, overlays, coverage semantics и backend contract.

**Non-Goals:**

- Не менять backend contract `query_project_metrics` и не добавлять новые project-level filters.
- Не возвращать на экран `Used skills`, `Contributing sessions`, summary cards или отдельные
  secondary blocks “для красоты”.
- Не превращать экран в BI-dashboard с несколькими панелями summary, большим explanatory copy или
  множеством независимых widgets.
- Не менять `grain` графика на time buckets или агрегированные интервалы: точка остаётся сессией.
- Не добавлять новые эвристики роли: в UI показывается только `agent_role`.
- Не добавлять новый routing flow или отдельный detail page вместо pinned side panel.
- Не размечать chart постоянными anomaly badges, pins или heat overlays поверх самих линий/точек.
- Не делать `chart type` отдельной пользовательской настройкой и не переключать график между
  несколькими независимыми визуальными режимами.
- Не вводить жёсткий лимит на число series с включёнными `raw values` или `emphasis`; читаемость
  обеспечивается visual hierarchy, а не запретами в UI.
- Не сохранять внутреннюю совместимость со старым `ProjectMetricsScreen`, его inspector/state
  структурой или текущей JSX-композицией только ради постепенной миграции.

## Decisions

### 1. Композиция экрана сводится к shell header + split-view

Решение: убрать локальный screen header и stage header, оставив один глобальный `app shell header`
как заголовок view и место для глобальных control-элементов `project`, `period`, `scope`,
`includeSpawnAgents`. Внутри `ProjectMetricsScreen` остаётся только split-view: main area с графиком
и side panel с tab navigation.

Причина: текущая верхняя композиция распределяет внимание между несколькими слоями chrome и
уменьшает perceived speed экрана. Пользовательский контракт теперь прямо требует одного компактного
control bar и максимального фокуса на графике.

Альтернатива: оставить локальный screen header и только “поджать” его. Минус: даже компактный
вложенный header продолжит конкурировать с графиком и дублировать shell-level context.

### 2. Selection model строится вокруг pinned session

Решение: hover больше не управляет inspector/summary. Hover используется только для tooltip на
графике. Click по точке фиксирует `pinned session`, и только она определяет содержимое near-graph
summary и `Pinned session` tab. Для навигации между соседними точками используется явная pinned
navigation, а не transient hover-sync.

Причина: при аналитической работе пользователь должен удерживать контекст выбранной точки, а не
терять его от любого движения мыши. Это особенно важно при сравнении значений графика и правой
панели.

Альтернатива: смешанный режим `hover previews + pinned details`. Минус: near-graph summary будет
“дрожать”, а side panel и график начнут жить в разных режимах внимания.

### 3. График всегда остаётся multi-metric correlation surface

Решение: график всегда показывает несколько метрик одновременно, включая метрики из разных
семейств. Экран не возвращается к модели “одна активная метрика + быстрый переключатель”, потому
что основной сценарий — видеть корреляции, совместные выбросы и смену baseline без прыжков между
несколькими графиками.

Причина: пользователь прямо зафиксировал, что `Project metrics` нужен для корреляционного анализа.
Следовательно, сравнение должно происходить внутри одного chart viewport, а не через быстрые
переключения одной серии.

Альтернатива: показывать только 1-2 метрики одновременно и заставлять пользователя переключать
остальные. Минус: это ломает основной сценарий поиска совместных движений между разными семействами
метрик.

### 4. Все series на графике нормализуются одним глобальным методом

Решение: все видимые metric series на графике проходят обязательную normalization одним глобальным
методом для всего текущего chart state. Нормализация не может переопределяться по отдельным
метрикам. Активная normalization должна быть явно видна в tab `Chart`, а tooltip и detail-UI
обязаны показывать raw absolute values рядом с normalized readings.

Причина: на correlation chart нельзя честно сравнивать разноразмерные `duration`, `tokens`,
`failures`, `quality` и другие показатели без единой шкалы. При этом per-series normalization rules
сделали бы график труднообъяснимым.

Альтернатива: normalization only on demand. Минус: основной режим снова превратится в смесь
несопоставимых шкал и потеряет ценность для корреляционного анализа.

### 5. Visual hierarchy фиксируется как `primary bar + muted secondary lines`

Решение: одна метрика всегда назначается `primary` и получает доминирующий визуальный слой как
bar-series. Все остальные видимые series остаются `secondary` и рисуются линиями с уменьшенным
stroke/opacity без равного визуального веса. Тип графика не переключается пользователем и не
становится самостоятельной настройкой.

Secondary series могут усиливаться двумя способами:

- transient emphasis на hover по элементу списка series или related chart affordance;
- persistent emphasis по явному действию пользователя в side panel.

Система не вводит жёсткий numeric cap на число emphasized series или включённых `raw values`; за
читаемость отвечает visual hierarchy, muted defaults и z-order, а не hard limit.

Причина: correlation chartу нужна устойчивая visual grammar. Если дать пользователю свободно
переключать chart type или рисовать несколько равновесных dominant layers, экран быстро станет
шумным.

Альтернатива: area/line/bar как свободно переключаемые режимы графика. Минус: это добавляет
сложность в UI и разрушает узнаваемый ритм чтения графика.

### 6. Analytical mode задаётся global default с per-series overrides

Решение: для аналитических режимов действует модель `global default + per-metric override`.

- tab `Chart` задаёт default analytical mode для серий без override;
- в tab `Series` каждая серия может получить собственный compact badge override:
  `trend`, `moving average`, `moving median`;
- `raw values` включаются тоже по каждой метрике отдельно;
- для неoverride-нутых серий пользователь читает chart как согласованную группу, а overrides
  используются точечно там, где действительно нужно выделить специфику ряда.

Причина: пользователь согласился на вариант `B`, потому что он быстрее в настройке: сначала
задаётся общий режим для графика, потом нужные метрики точечно получают override без тяжёлой
control matrix.

Альтернатива: только global mode или полностью независимый mode по каждой series. Минус первого —
недостаточная гибкость; минус второго — когнитивная перегрузка и разрастание control surface.

### 7. Near-graph summary делается одной строкой с двумя блоками

Решение: над графиком остаётся только одна строка pinned summary:

- слева: `agent_role`, flags `anomaly / partial / unknown / degraded`, время точки, короткий
  `session id`;
- справа: `duration`, `tokens`, `calls`, `failures`.

Причина: пользователю нужна постоянная минимальная truth-layer сводка по pinned point, но без
карточек, абзацев и explanatory copy. Однострочная структура даёт быстрый контекст и не ломает
ритм анализа.

Альтернатива: две строки или мини-карточки. Минус: они увеличивают вертикальный шум и снова
превращают верх графика в secondary panel.

### 8. Side panel состоит из пяти tabs с жёстким разделением ответственности

Решение: правая панель становится tabbed side panel:

- `Window pulse`: quick summary текущего window без отдельного крупного overview-блока в main area;
- `Anomalies`: отдельная anomaly-вкладка со сгруппированным списком и состоянием collapse по
  классам аномалий;
- `Series`: состав видимых рядов, counters `known / partial / unknown`, назначение `primary`,
  actions `emphasize`, per-series override badge и per-series toggle `raw values`;
- `Chart`: только глобальные chart settings — active normalization description, global default
  analytical mode и `outlier mode`;
- `Pinned session`: вся информация по pinned session, включая метрики и стартовый запрос.

Причина: текущий inspector смешивает разные уровни смысла. Tabs позволяют убрать постоянный шум и
сделать вторичную панель компактной, не забирая у пользователя доступ к нужным control-surface.

Альтернатива: секционный inspector без tabs. Минус: при полном наборе данных он снова превращается
в длинный scrollable поток блоков.

### 9. Аномалии разделяются на четыре класса и живут вне chart viewport

Решение: anomaly model делится на четыре независимых класса:

- `Metric`: локальные статистические выбросы, spikes, drops и другие point-level anomalies по
  отдельным series;
- `Baseline`: `change-point`, `baseline shift`, sustained degradation/uplift, то есть window/range
  level anomalies, описывающие смену режима ряда;
- `Session`: необычное поведение самой сессии, например `failed / interrupted`, abnormal failures,
  abnormal duration/tokens/tool profile;
- `Data issue`: `partial`, `unknown`, `degraded`, missing chart values и другие сигналы неполноты
  или недостоверности данных.

Эти классы не рисуются persistent markers прямо на chart. Вместо этого:

- tab `Anomalies` получает badge/count в заголовке;
- near-graph summary показывает только компактные indicators по pinned point/window;
- подробные anomaly items доступны только в side panel.

Причина: текущая реализация смешивает metric outliers, data quality и identity issues в один список,
из-за чего пользователю трудно отличить поведенческую аномалию от проблемы данных. Чистый chart без
дополнительных маркеров лучше соответствует требованию “график — основной рабочий инструмент”.

Альтернатива: показывать anomaly markers прямо на chart и дублировать их в inspector. Минус:
график быстро превращается в шумный diagnostic canvas и теряет роль чистого аналитического
инструмента.

### 10. Tab `Anomalies` использует группировку, collapse и severity-first сортировку

Решение: `Anomalies` tab должен:

- группировать anomaly items по классам `Metric`, `Baseline`, `Session`, `Data issue`;
- позволять сворачивать и разворачивать каждую группу;
- показывать в заголовке группы count и старший severity level;
- сортировать внутри группы по `severity -> recency`;
- давать для item action вида `focus point`, `focus range` или `pin session`, в зависимости от
  типа anomaly.

`Baseline` anomalies трактуются как range-level сущности: они фокусируют окно графика на интервале,
а не пытаются притворяться обычной point/session anomaly.

Причина: пользователю нужен быстрый, но не шумный путь к разбору аномалий. Grouped collapsible list
даёт хороший баланс между плотностью и управляемостью.

Альтернатива: один flat list по всем anomaly items. Минус: при большом окне он быстро теряет смысл,
а baseline/data issues начинают визуально смешиваться с point-level incidents.

### 11. `Pinned session` tab становится полным источником правды по выбранной сессии

Решение: `Pinned session` tab должен показывать все доступные значения метрик, `agent_role`,
полный `session id`, flags, время и стартовый пользовательский запрос. Near-graph summary при этом
остаётся только компактной выжимкой.

Причина: пользователь явно запретил трактовать pinned panel как “дополнительные детали”. Она
должна быть полноценным session detail без ухода со страницы и без потери контекста графика.

Альтернатива: краткая карточка справа и переход в отдельный session screen за полными данными.
Минус: нарушает основной UX-цель “анализировать конкретные сессии без потери контекста графика”.

### 12. Стартовый пользовательский запрос грузится через отдельный materialized lazy-fetch endpoint

Решение: change не расширяет project metrics backend contract. Для `Pinned session` добавляется
отдельный lightweight endpoint `load_project_metrics_session_detail_by_id`, который возвращает
materialized session-detail payload для текущего `session_id`. Этот payload хранится в
`session-metrics.sqlite` рядом с materialized session metrics и version-gated так же, как текущие
метрики. Materialization использует state/index metadata (`title`, `first_user_message`) и fallback
по исходным event records только в момент пересчёта detail, а не на каждом UI-запросе.

Причина: пользователю нужен отдельный lazy-fetch path без потери быстроты интерфейса и без
раздувания `ProjectMetricsResponse`. Хранение detail summary в materialized store делает pinned
detail дешёвым по latency и не заставляет каждый hover/pin перечитывать rollout целиком.

Альтернатива: всегда читать сырой session preview / event tree по `session_id`. Минус: это делает
latency detail-запроса зависящим от размера rollout и не использует уже существующую materialized
архитектуру.

### 13. Zoom/navigation controls живут поверх графика

Решение: удалить отдельные controls `window size` и `window position` вне графика. Основной zoom —
выделением региона на графике. Быстрая навигация — overlay-кнопками `+`, `-`, `←`, `→` в левом
верхнем углу chart viewport.

Причина: отдельные window sliders и toolbar-кнопки сейчас добавляют верхний шум и визуально
разрывают chart-first composition. Overlay controls сохраняют функциональность, но не забирают
пространство у графика.

Альтернатива: оставить текущие window controls и просто визуально уменьшить их. Минус: они всё
равно остаются отдельным layer chrome, конкурирующим с графиком.

### 14. Data quality states показываются как compact flags, а не как большие alerts

Решение: состояния `partial`, `unknown`, `degraded` и summary-level anomaly indicators больше не оформляются
крупными поясняющими блоками над графиком в loaded-state. Вместо этого используются компактные
flags в near-graph summary, tooltip, заголовке tab `Anomalies` и `Pinned session` tab.
Полноразмерные empty/error/loading state остаются только для действительно блокирующих состояний.

Причина: для аналитического экрана важно видеть data quality постоянно, но без превращения main
area в поток предупреждений. Flags дают постоянный truth-signal без захвата внимания.

Альтернатива: оставить informational alerts над графиком. Минус: они ломают вертикальный ритм и
снова превращают main area в смесь графика и текста.

### 15. `Used skills` и `Contributing sessions` удаляются с этого экрана

Решение: удалить `Used skills` и `Contributing sessions` из project metrics screen. Эти блоки не
участвуют в основном аналитическом сценарии и будут только мешать восприятию графика и pinned
session detail.

Причина: пользователь явно зафиксировал, что `Used skills` относится к отдельному view, а список
contributing sessions здесь не нужен. Для `grain=session` достаточно pinned selection на графике.

Альтернатива: спрятать их в скрытый tab или collapsible section. Минус: это сохраняет scope creep
и отвлекает от основного контракта экрана.

### 16. Экран пересобирается как новая реализация без compatibility-слоя

Решение: change трактуется как полный redesign `Project metrics`, а не как бережная адаптация
текущего `ProjectMetricsScreen`. Если по факту проще удалить существующий screen/container,
пересобрать state/model границы и заново собрать UI под новый контракт, это считается правильным
путём реализации. Сохранять старую JSX-структуру, inspector-композицию или общий state только ради
внутренней совместимости запрещено.

Причина: текущий экран уже накопил несколько конкурирующих моделей взаимодействия
(`overview + alerts + chart + inspector + sessions list`). Попытка сохранить их как basis для
нового workspace почти наверняка приведёт к transitional коду, флагам совместимости и спутанным
state boundaries.

Альтернатива: делать поэтапный refactor с сохранением старой screen-структуры и временными мостами
между старым и новым поведением. Минус: это оставляет в коде ложные ограничения, усложняет
selection model и мешает честно перейти на pinned-first workspace.

## State/Model Architecture

### Проблема текущей реализации

Текущий экран смешивает в одном слое несколько разных типов состояния:

- query/input state (`project`, `period`, `scope`, `includeSpawnAgents`, loading/error);
- derived read model (`chartRows`, `chartSeries`, `sessions`, `usedSkills`, summary cards);
- user workspace state (`visibleSeries`, chart mode, overlays, zoom);
- transient interaction state (hover, drag selection, popovers);
- source-of-truth выбранной сессии.

Главный симптом этого смешения: один `activeSessionId` фактически пытается одновременно играть роль
hovered point, pinned selection и внешне открытой session context. Для нового UX-контракта это
неправильная ось истины.

### Целевое разделение слоёв

Новая архитектура должна разделять state/model минимум на пять слоёв.

1. `Query/Input State`
   Живёт на уровне shell/app и отвечает только за пользовательский запрос:
   `selectedProjectKey`, `range`, `scopeFilter`, `includeSpawnAgents`, loading/error, raw
   `ProjectMetricsResponse`.
2. `Workspace Domain Model`
   Строится из `ProjectMetricsResponse` и хранит стабильную truth-layer модель workspace без
   привязки к текущей раскладке UI: sessions, series catalog, coverage facts, anomaly source facts,
   window/session summaries.
3. `Workspace State`
   Описывает то, что пользователь действительно настраивает внутри analytics workspace:
   `pinnedSessionId`, active tab, zoom window, visible series, `primary` metric, emphasized series,
   global chart mode, per-series overrides, per-series raw-values toggles, normalization mode,
   outlier mode.
4. `Transient Interaction State`
   Хранит только краткоживущие UI-сигналы: `hoveredSessionId`, drag-zoom selection, hover emphasis,
   popover/tabs transient state. Этот слой не имеет права менять source-of-truth side panel.
5. `Lazy Detail State`
   Отдельный cache/load state для `Pinned session` detail по `session_id`, потому что detail
   грузится отдельным lazy-fetch endpoint и не является частью базового project-level query.

### Обязательные state boundaries

С архитектурной точки зрения change обязан зафиксировать следующие границы:

- `hoveredSessionId` и `pinnedSessionId` - разные сущности;
- hover влияет только на tooltip и transient emphasis;
- `pinnedSessionId` определяет near-graph summary и `Pinned session` tab;
- внешний открытый session flow не должен переиспользовать pinned/hover state как общий mutable
  флаг;
- metadata каталога series и пользовательская конфигурация series не должны жить в одном объекте.

### Domain model против presentation model

Нельзя продолжать использовать один крупный view-model как смесь domain truth и layout-специфичных
блоков старого экрана. Вместо этого нужны разные уровни:

- stable workspace model: sessions, series definitions, anomaly facts, compact flags, detail keys;
- derived selectors for UI surfaces: pinned summary, `Window pulse`, grouped `Anomalies`, `Series`,
  `Chart`, `Pinned session`;
- presentation blocks старого экрана (`summaryCards`, `Used skills`, `Contributing sessions`,
  large alerts) не считаются обязательной частью нового model layer и могут быть удалены целиком.

### Минимальный blueprint типов

На уровне архитектуры целевая схема выглядит так:

```text
ProjectMetricsQueryState
  -> raw backend response

ProjectMetricsWorkspaceModel
  -> stable domain truth from response

ProjectMetricsWorkspaceState
  -> pinned session, zoom, series config, chart config, active tab

ProjectMetricsInteractionState
  -> hover, drag selection, transient menus

ProjectMetricsDetailState
  -> lazy detail cache by session id

Selectors
  -> pinned summary model
  -> window pulse model
  -> anomalies groups model
  -> chart render model
  -> pinned session detail model
```

Это означает, что будущий `chart render model` должен строиться не напрямую из "visible rows +
global chart mode", а из полноценного workspace/chart config, где уже выражены `primary` /
`secondary`, global default, per-series override и raw-values toggles.

### Практическое следствие для реализации

Правильная последовательность реализации должна идти не от вёрстки tabs, а от state/model
границ:

1. разрезать `hover` и `pinned` selection;
2. отделить stable workspace model от старой layout-oriented view-model структуры;
3. ввести самостоятельный workspace state для chart/series configuration;
4. только после этого пересобрать UI вокруг `Window pulse`, `Anomalies`, `Series`, `Chart`,
   `Pinned session`.

Именно поэтому экран допускается удалить и собрать заново: новая архитектура ценнее сохранения
частей старой реализации.

## Risks / Trade-offs

- [Risk] Удаление `Used skills` и `Contributing sessions` сократит количество доступной информации
  на экране для части сценариев. -> Mitigation: зафиксировать, что эти сценарии должны жить в
  отдельных view, а не раздувать `Project metrics`.
- [Risk] Для части сессий стартовый пользовательский запрос не удастся извлечь даже при
  materialization detail summary. -> Mitigation: хранить `request_text_source` и явно показывать
  `unavailable`, не подменяя данные эвристикой.
- [Risk] Дополнительный materialized payload устареет после изменения правил извлечения detail. ->
  Mitigation: version-gate detail table теми же `metrics_schema_version` /
  `source_projection_version` и перестраивать payload при несовпадении версий.
- [Risk] Overlay-кнопки поверх графика могут конфликтовать с hover/selection. -> Mitigation:
  держать их компактными, привязать к левому верхнему углу и не перекрывать near-graph summary.
- [Risk] Пользователь включит много raw-series и several emphasized secondaries одновременно, и
  график станет шумным. -> Mitigation: не вводить hard cap, но держать non-active series
  приглушёнными по умолчанию, усиливать только выбранные series и явно показывать visual hierarchy.
- [Risk] Всегда включённая normalization может скрыть ощущение абсолютного масштаба. ->
  Mitigation: в tooltip, pinned detail и других inspection surfaces всегда показывать raw
  absolute values рядом с normalized reading.
- [Risk] Per-series overrides mode могут сделать sidebar похожим на матрицу настроек. ->
  Mitigation: использовать `global default + compact override badges`, а не полные отдельные формы
  для каждой series.
- [Risk] `Anomalies` tab может стать слишком длинным на больших окнах. -> Mitigation: group by
  class, collapse по умолчанию для не-critical групп, severity-first sorting и range/session focus
  actions вместо длинных описаний.
- [Risk] `change-point / baseline shift` легко превратить в псевдо-эвристику с низким доверием. ->
  Mitigation: выводить baseline anomalies как отдельный класс с явным типом и диапазоном действия,
  не смешивая их с обычными point outliers.
- [Risk] При большом числе visible series side panel tab `Series` может стать длинным. ->
  Mitigation: добавить группировку и quick actions, а при необходимости локальный поиск по имени.
- [Risk] Удаление hover-sync с side panel может показаться шагом назад для быстрых просмотров. ->
  Mitigation: сохранить информативный tooltip на hover и сделать click-to-pin максимально
  лёгким и очевидным.
- [Risk] Полная пересборка экрана без compatibility-слоя увеличит объём frontend diff и может
  временно убрать часть старых affordance. -> Mitigation: оценивать change по соответствию новому
  контракту, а не по сохранённым фрагментам старого UI; удалять мёртвые блоки сразу, а не держать
  их как fallback.

## Migration Plan

1. Принять full-rebuild path: не сохранять старую screen-композицию и при необходимости удалить
   текущую реализацию `ProjectMetricsScreen` перед сборкой нового workspace.
2. Упростить shell/header composition и убрать локальные header-блоки `ProjectMetricsScreen`.
3. Перевести layout main area на near-graph summary + chart viewport + overlay controls.
4. Реализовать pinned selection model и отвязать side panel от hover behavior.
5. Разделить state/model на query layer, stable workspace model, workspace state, transient
   interaction state и lazy detail state.
6. Пересобрать chart state под correlation model: mandatory normalization, fixed `primary` /
   `secondary` visual hierarchy, `global default + per-series override` analytical modes и
   per-series `raw values`.
7. Пересобрать правую панель в tabs `Window pulse`, `Anomalies`, `Series`, `Chart`,
   `Pinned session`, перераспределив в них controls согласно новой chart model.
8. Добавить materialized lazy-fetch endpoint для pinned session detail и storage в
   `session-metrics.sqlite`, не расширяя `query_project_metrics`.
9. Зафиксировать anomaly taxonomy, grouped/collapsible `Anomalies` tab, tab badge и compact
   indicators в near-graph summary.
10. Перенести/удалить устаревшие blocks (`Used skills`, `Contributing sessions`, summary cards,
   top-level alerts loaded-state, window sliders, standalone anomaly panels).
11. Обновить UI tests и delta specs, затем выполнить OpenSpec validation.

Rollback: change не предусматривает внутренний compatibility-layer rollback. Откат возможен только
через обычный git/OpenSpec revert всего redesign-среза. Изменение не требует миграции данных и не
затрагивает backend persistence.

## Open Questions

- Нужно ли оставлять явные `prev / next / unpin` controls в near-graph summary, или первичная
  навигация между pinned points будет достаточно удобна через overlay-кнопки и chart click?
- Нужно ли будет позже расширять materialized detail payload дополнительными полями beyond
  `start_user_request` / `task summary`, или текущего lightweight summary достаточно для pinned tab?
- Какой именно global normalization method фиксируется для первой реализации correlation chart:
  baseline index, percent delta или другая единая форма, если per-series normalization rules
  запрещены?
