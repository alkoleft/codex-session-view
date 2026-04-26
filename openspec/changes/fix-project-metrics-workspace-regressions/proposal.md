## Why

После завершения `redesign-project-metrics-analytics-screen` в реальном использовании всплыли
несколько регрессий и недошлифованных мест в `Project metrics`:

- при смене `project`, `period`, `scope` или `includeSpawnAgents` экран сбрасывает chart/window и
  per-series настройки вместо сохранения рабочего контекста;
- pin выбранной сессии на графике работает ненадёжно: secondary dots можно активировать, а primary
  bar-layer не даёт такого же прямого `click -> pin` контракта;
- состояния `active / inactive` у compact badges и actions читаются слабо;
- primary bar-layer визуально слишком тяжёлый и мешает считыванию secondary lines;
- содержимое вкладок правой панели не получает надёжный независимый scroll-owner при переполнении.
- семантика skill-метрик читается неоднозначно: `Enabled skills` должно считаться по стартовому
  списку skills из первого сообщения сессии, тогда как явные `<skill>...</skill>` сообщения
  означают фактическую загрузку/использование skill и не должны подменять count подключённых
  skills.

Эти замечания не требуют нового redesign и не меняют продуктовый scope экрана, но нарушают уже
принятый UX-контракт chart-first workspace.

## What Changes

- Убрать полный reset workspace state при обновлении project-metrics query и заменить его на
  reconcile-модель: сохранять `activeTab`, `defaultMode`, `outlierMode`, `series visibility`,
  `primary`, `emphasis`, `raw values` и zoom/window там, где новый dataset это допускает.
- Зафиксировать единый и детерминированный `click -> pin` путь для всех chart layers, включая
  `primary` bar.
- Усилить визуальную разницу между включёнными и выключенными compact controls в правой панели без
  превращения UI в тяжёлую control matrix.
- Снизить непрозрачность primary bar так, чтобы он оставался доминирующим слоем, но не забивал
  secondary lines.
- Скорректировать shared normalization для sparse series через мягкое сжатие extremes перед
  глобальным `min-max`, чтобы `failures`, `tool calls` и похожие метрики не прилипали к нижней
  границе chart-scale при наличии более размашистых series.
- Уточнить состав `Window pulse`: в quick summary обязательно показывать `duration` и `token drift`
  как отдельные полезные сигналы текущего visible window.
- Добавить явное визуальное отражение pinned session на самом графике, чтобы текущий pinned context
  читался не только в summary и side panel, но и прямо в chart viewport.
- Восстановить рабочий независимый scroll внутри tab-content правой панели и закрыть это
  контрактом в OpenSpec и тестах.
- Зафиксировать и проверить semantics skill metrics: `Enabled skills` считать по стартовому списку
  available skills из первого сообщения сессии, а явные `<skill>` markers трактовать как отдельный
  сигнал фактического usage/load без влияния на count подключённых skills.

## Capabilities

### Modified Capabilities

- `project-metrics-screen`: уточняется поведение workspace state при requery и поведение pinned
  selection после обновления данных, а также semantics `Enabled skills` vs explicit skill usage.
- `project-metrics-charts`: уточняется контракт `click -> pin` для всех визуальных слоёв графика и
  читаемость primary/secondary visual hierarchy, а также shared normalization для sparse series.
- `project-metrics-layout-focus`: уточняются требования к независимому scroll правой панели и к
  читаемости compact active/inactive controls.

## Impact

- `apps/codex-session-explorer/src/components/project-metrics-screen.tsx`: reconcile workspace
  state, явный point activation для primary bar, scroll/flex chain правой панели и визуальные
  правки controls/bar opacity.
- `apps/codex-session-explorer/src/components/project-metrics-chart.ts`: обновление shared
  normalization без отказа от общей chart-scale.
- `apps/codex-session-explorer/src/components/project-metrics-screen.test.tsx`: контрактные тесты
  на сохранение workspace state, прямой pin по графику и независимый scroll side tabs.
- `crates/codex-log/src/session_metrics.rs`,
  `crates/codex-log/src/events/readers/session.rs`,
  `crates/codex-log/src/events/readers.rs`: проверка и при необходимости исправление различения
  `connected/enabled skills` и `used/loaded skills`.
- `apps/codex-session-explorer/src/components/ui/scroll-area.tsx`,
  `apps/codex-session-explorer/src/components/ui/button.tsx`,
  `apps/codex-session-explorer/src/components/ui/badge.tsx`: при необходимости точечные UI-правки
  для рабочего scroll и более читаемых active/inactive states.
- `openspec/specs/project-metrics-screen/spec.md`,
  `openspec/specs/project-metrics-charts/spec.md`,
  `openspec/specs/project-metrics-layout-focus/spec.md`: delta specs для регрессионного follow-up.
