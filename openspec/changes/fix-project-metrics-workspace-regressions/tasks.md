## 1. Workspace state reconcile

- [x] 1.1 Убрать полный reset `workspaceState` при каждом новом `queryModel` и ввести reconcile для
      совместимых chart-global и per-series настроек
- [x] 1.2 Сохранять `activeTab`, `defaultMode`, `outlierMode`, `visible`, `primary`, `emphasis`,
      per-series override и `raw values`, если новая конфигурация series это допускает
- [x] 1.3 Сохранять zoom-window через clamp к новому dataset и не откатывать его в initial window
      без необходимости
- [x] 1.4 Сохранять pinned session при наличии той же `sessionId` в новом наборе, а при отсутствии
      делать явный fallback без сброса остального workspace context

## 2. Chart click and visual hierarchy

- [x] 2.1 Реализовать детерминированный `click -> pin` для `primary` bar-layer
- [x] 2.2 Убедиться, что secondary series сохраняют direct pin по клику и не зависят от hover как
      единственного источника выбора
- [x] 2.3 Развести single-click activation и drag-selection zoom так, чтобы одно не ломало другое
- [x] 2.4 Добавить мягкое сжатие extremes перед global `min-max`, сохранив shared chart-scale для
      всех видимых series
- [x] 2.5 Обновить normalization help text и тестовые ожидания так, чтобы sparse series вроде
      `failures` и `tool calls` оставались читаемыми без перехода на per-series normalization
- [ ] 2.6 Добавить persistent visual affordance для pinned session прямо на графике
- [ ] 2.7 Снизить opacity primary bar до более читаемого уровня, сохранив его как dominant layer

## 3. Window pulse and side panel readability

- [ ] 3.1 Расширить `Window pulse`, добавив как минимум `duration` и `token drift` в compact
      window-level summary
- [ ] 3.2 Исправить flex/height/overflow chain правой панели так, чтобы переполненный tab content
      имел независимый рабочий scroll-owner
- [ ] 3.3 Усилить читаемость active/inactive состояний у compact badges и actions в side panel
- [ ] 3.4 Обновить или дополнить unit tests так, чтобы они ловили регрессии scroll, pinned
      affordance и state readability-contract, а не только проверяли наличие CSS-классов

## 4. Validation

- [x] 4.1 Добавить frontend tests на сохранение workspace state при смене dataset
- [x] 4.2 Добавить frontend test на pin выбранной сессии кликом по `primary` bar
- [x] 4.3 Добавить frontend test или chart-analysis test на improved shared normalization для sparse
      series
- [ ] 4.4 Добавить frontend test на `Window pulse` с `duration` и `token drift`
- [ ] 4.5 Добавить frontend/Playwright check на видимое отражение pinned session прямо на графике
- [ ] 4.6 Прогнать `npm --prefix apps/codex-session-explorer test`
- [ ] 4.7 Прогнать `npm --prefix apps/codex-session-explorer run build`
- [ ] 4.8 Выполнить Playwright UAT экрана `Project metrics`, исправить найденные замечания и
      повторить проверку до зелёного результата
- [ ] 4.9 Прогнать `openspec validate fix-project-metrics-workspace-regressions --strict`
