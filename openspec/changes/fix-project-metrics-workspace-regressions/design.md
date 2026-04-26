## Context

Change остаётся узким follow-up к уже принятому redesign экрана и не пересматривает основную
композицию `Project metrics`. По коду видно три корневые проблемы:

1. `ProjectMetricsScreen` заново создаёт `workspaceState` при каждом новом `queryModel`, из-за чего
   пользователь теряет chart context при смене `project / period / scope / includeSpawnAgents`.
2. Модель pin опирается на `SeriesDot` и transient hover-path, поэтому `primary` bar-layer не
   гарантирует такой же прямой `click -> pin`, как secondary series.
3. Правая панель визуально оформлена как независимый scroll-owner, но реальная flex/height-цепочка
   для tab content и `ScrollArea` недостаточно жёсткая, поэтому переполненный контент может
   перестать прокручиваться как отдельная область.
4. Вокруг skill-метрик есть semantic drift: `Enabled skills` должно отражать стартовый список
   available skills из первого сообщения сессии, а явные `<skill>...</skill>` сообщения означают
   уже факт загрузки/использования skill и не должны переписывать count подключённых skills.

Отдельно есть два UX-полиша, не меняющих архитектуру:

- compact controls с вариантами `secondary` / `outline` сейчас дают слабую визуальную дельту между
  включённым и выключенным состояниями;
- `primary` bar слишком плотный для режима, где secondary lines тоже должны читаться как важный
  аналитический слой.
- `Window pulse` как quick summary полезен, но ему не хватает более содержательных signals уровня
  текущего window, в частности `duration` и `token drift`.
- pinned session хорошо видна в summary и side panel, но на самом графике ей не хватает явного
  persistent affordance, который бы удерживал взгляд в chart viewport.

И ещё один проблемный слой лежит уже в математике chart rendering:

- `failures`, `tool calls` и другие sparse series визуально часто прижимаются к нижней границе
  графика не потому, что не проходят normalization, а потому, что после baseline-transform
  попадают в общий global `min-max` вместе с более размашистыми series;
- текущая shared normalization остаётся правильной по идее correlation chart, но недостаточно
  устойчива к extremes и из-за этого теряет читаемость на редких operational metrics.

## Goals / Non-Goals

**Goals:**

- Сохранить пользовательский workspace context при requery без возврата к старой mixed inspector
  модели.
- Сделать `click -> pin` одинаково надёжным для primary и secondary chart layers.
- Починить независимый scroll у tab content правой панели.
- Подтянуть читаемость active/inactive compact controls и прозрачность primary bar.
- Улучшить shared normalization для sparse series без перехода на per-series normalization.
- Сделать `Window pulse` более полезным для быстрого чтения текущего окна.
- Сделать pinned session визуально заметной прямо на chart viewport.

**Non-Goals:**

- Не менять backend contract `query_project_metrics`.
- Не вводить новый chart type, новые tabs или новые project-level filters.
- Не переходить на per-series normalization и не ломать shared chart-scale correlation workspace.
- Не делать ещё один redesign экрана вместо узкого regression fix.

## Decisions

### 1. Workspace state при requery reconciles, а не пересоздаётся целиком

Решение: на смену dataset экран должен не вызывать полный `createInitialWorkspaceState(...)`, а
строить новый state через reconcile:

- chart-global state (`activeTab`, `defaultMode`, `outlierMode`) сохраняется всегда;
- per-series config сохраняется по совпадающим `series.key`;
- `primarySeriesKey` сохраняется, если серия всё ещё доступна и видима, иначе выбирается ближайший
  валидный fallback;
- zoom-window сохраняется через clamp к новой длине dataset, а не сбрасывается в initial window;
- `pinnedSessionId` сохраняется, если такая сессия ещё есть в новом наборе, иначе очищается или
  переводится в стандартный fallback без сброса остальных пользовательских настроек.

Причина: замечание пользователя относится именно к потере рабочего контекста. Для chart-first
workspace reset на каждый requery разрушает сценарий сравнения и последовательного анализа.

### 2. Pinning строится на явном point activation, а не на hover-состоянии

Решение: клик по любой визуальной репрезентации точки должен напрямую активировать связанный
`sessionId`.

Практически это означает:

- `primary` bar-layer получает явный путь активации;
- secondary lines/dots продолжают поддерживать direct click;
- hover остаётся только tooltip/transient emphasis каналом;
- drag-selection для zoom не должен ломать single-click pin.

Причина: уже зафиксированный контракт `hover tooltip + pinned by click` должен работать одинаково
для всей visual grammar, а не только для части серий.

### 3. Side panel получает жёсткую scroll-chain

Решение: правая колонка должна иметь непрерывную цепочку `min-h-0 + flex + overflow-hidden` до
реального viewport scroll-area. Контракт надо закрепить не только class-assertions, но и тестом,
который не подменяет `ScrollArea` слишком упрощённым `div`.

Причина: текущий unit test проверяет в основном наличие классов, но не защищает реальный Radix
scroll contract.

### 4. Shared normalization сохраняется общей, но получает мягкое сжатие extremes

Решение: сохранить общую chart-level normalization для всех видимых series, но перед глобальным
`min-max` пропускать transformed values через мягкую compression-функцию, например `asinh(x)` или
эквивалентный monotonic compression layer.

Практически это означает:

- baseline-relative transform остаётся общей отправной точкой;
- shared scale `0..1` остаётся общей для всех видимых series;
- большие percent-delta swings сжимаются мягко, а не доминируют над всем global range;
- sparse operational metrics вроде `failures` и `tool calls` получают больше визуальной
  различимости без перехода к per-series scaling.

Причина: проблема пользователя не в самом факте общей normalization, а в том, что extremes у
отдельных series делают эту шкалу визуально бесполезной для редких рядов. Мягкая compression layer
исправляет это, не ломая идею correlation chart.

Альтернатива: перейти на per-series normalization. Минус: график перестанет быть честной общей
comparison surface между разными семействами метрик.

### 5. Compact controls усиливают state contrast без роста визуального шума

Решение: selected/on states у compact controls должны читаться по нескольким признакам сразу:

- более сильный fill/background;
- более контрастный border/text;
- сохранение `aria-pressed` и текстовой формулировки.

Это касается прежде всего actions и badges в `Series`, `Chart` и grouped anomaly UI.

Причина: пользователь жалуется не на отсутствие controls, а на слабую визуальную различимость
включённого и выключенного состояния.

### 6. `Window pulse` остаётся компактным, но показывает `duration` и `token drift`

Решение: `Window pulse` остаётся quick-summary tab без превращения в большой overview-блок, но его
набор сигналов расширяется как минимум до `duration` и `token drift`.

Практически это означает:

- в summary остаётся компактный формат;
- `duration` должен отражать полезную aggregate/window картину, а не только count сессий;
- `token drift` должен помогать быстро заметить смещение token usage в текущем окне без открытия
  deeper detail.

Причина: пользовательский сценарий quick triage требует видеть не только количество и completion,
но и более аналитические window-level signals.

### 7. Pinned session получает явный chart affordance

Решение: pinned session должна быть заметна прямо в chart viewport отдельным persistent visual
affordance, не сводящимся к tooltip hover-state.

Практически это означает:

- pinned point/bar остаётся визуально выделенным после клика;
- выделение должно работать и для `primary` bar, и для secondary point;
- affordance не должен превращаться в шумный overlay или новый anomaly marker слой.

Причина: иначе пользователь вынужден постоянно переводить взгляд между summary/side panel и
графиком, чтобы вспомнить, какая точка сейчас pinned.

### 8. Primary bar остаётся доминирующим, но становится менее непрозрачным

Решение: primary bar не теряет роль dominant layer, но его alpha снижается настолько, чтобы
secondary lines лучше читались поверх того же viewport.

Причина: это чистый visual polish в рамках уже принятой grammar `primary bar + muted secondary
lines`, а не изменение самой grammar.

### 9. `Enabled skills` и explicit `<skill>` markers фиксируются как разные semantic layers

Решение: `Enabled skills` и соответствующий `skills_count` должны вычисляться по стартовому списку
available skills из первого сообщения сессии, а не по более поздним usage markers.

Практически это означает:

- если первое сообщение сессии содержит блок `skills_instructions`/`Available skills` или
  эквивалентный стартовый context list, именно он задаёт count подключённых skills;
- явное сообщение вида `<skill><name>...</name><path>.../SKILL.md</path></skill>` означает, что
  skill был загружен/использован в сессии;
- простой показ или цитирование текста `skills_instructions` / `### Available skills` не означает
  загрузку skill сам по себе;
- такие `<skill>` markers должны учитываться отдельно как usage/load signal и не должны менять
  `Enabled skills` count;
- shell/tool чтение `.../SKILL.md` может оставаться вспомогательным signal usage, но не источником
  значения для `connected/enabled skills`.

Concrete evidence: текущая сессия `019dc925-2dad-7901-9118-628b4e3f08f9` показывает оба вида
сигналов одновременно:

- в [rollout-2026-04-26T12-35-50-019dc925-2dad-7901-9118-628b4e3f08f9.jsonl](/home/alko/.codex/sessions/2026/04/26/rollout-2026-04-26T12-35-50-019dc925-2dad-7901-9118-628b4e3f08f9.jsonl:3)
  есть блок `<skills_instructions> ... ### Available skills ... </skills_instructions>`, который
  описывает skills, подключённые к runtime context, и сам по себе не означает загрузку конкретного
  skill;
- в [rollout-2026-04-26T12-35-50-019dc925-2dad-7901-9118-628b4e3f08f9.jsonl](/home/alko/.codex/sessions/2026/04/26/rollout-2026-04-26T12-35-50-019dc925-2dad-7901-9118-628b4e3f08f9.jsonl:6)
  пользователь цитирует кусок `<skills_instructions> ... ### Available skills ...`, и такая цитата
  тоже не должна считаться загрузкой skill;
- в [rollout-2026-04-26T12-35-50-019dc925-2dad-7901-9118-628b4e3f08f9.jsonl](/home/alko/.codex/sessions/2026/04/26/rollout-2026-04-26T12-35-50-019dc925-2dad-7901-9118-628b4e3f08f9.jsonl:8)
  есть explicit `<skill>` marker для `openspec-explore`, и именно этот сигнал уже должен
  трактоваться как фактическая загрузка/использование skill.

Причина: иначе система смешивает два разных вопроса: «что было доступно агенту в этой сессии с
самого начала» и «какой skill реально активировали по ходу разговора». Это делает метрику
`Количество подключенных skills в сессии` недостоверной.

Альтернатива: продолжать выводить `Enabled skills` по любому позднему usage signal или по чтению
`SKILL.md`. Минус: это подменяет availability metric usage metric-ой и ломает интерпретацию
проектной аналитики.

## Risks / Trade-offs

- [Risk] Сохранение старого workspace state может протянуть невалидную конфигурацию в новый
  dataset. -> Mitigation: reconcile только по реально существующим `series.key` и clamp window/pin
  к новому набору данных.
- [Risk] Новый click handler для chart может конфликтовать с region-selection zoom. -> Mitigation:
  явно разделить single-click activation и drag-selection threshold.
- [Risk] Сжатие extremes может сделать normalized readings менее интуитивными для объяснения. ->
  Mitigation: обновить help text normalization и не менять сам факт общей `0..1` шкалы.
- [Risk] Добавление pinned affordance на chart может сделать viewport шумнее. -> Mitigation:
  использовать один компактный persistent marker без превращения его в отдельный marker system.
- [Risk] Усиление active states может сделать панель визуально тяжелее. -> Mitigation: усиливать
  contrast точечно, не меняя общий compact rhythm правой панели.
- [Risk] Исторические логи могут по-разному представлять стартовый список available skills и
  explicit `<skill>` markers. -> Mitigation: зафиксировать precedence и покрыть reader/metrics
  тестами как минимум кейсы `first message available skills` и `later explicit skill usage`.

## Migration Plan

1. Вынести reconcile helper для workspace state при смене query result.
2. Добавить единый explicit activation path для primary bar и сохранить direct pin на secondary.
3. Обновить shared normalization через мягкое сжатие extremes перед global `min-max`.
4. Уточнить `Window pulse` и pinned affordance в chart viewport.
5. Исправить layout/scroll chain правой панели и обновить tests.
6. Подкрутить visual states compact controls и opacity primary bar.
7. Проверить и при необходимости исправить extraction/aggregation semantics для `Enabled skills`
   против explicit `<skill>` markers.
8. Прогнать `openspec validate ... --strict`, frontend tests и Playwright UAT уже при выходе из
   explore mode и реализации.
