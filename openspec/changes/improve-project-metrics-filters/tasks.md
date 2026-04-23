## 1. Backend project catalog

- [ ] 1.1 Добавить backend helper/endpoint для полного project catalog экрана `Project metrics`, не зависящего от пагинации `listIndexedSessions`
- [ ] 1.2 Реализовать dedupe project rows и формирование display metadata/counts для selector на backend стороне

## 2. Session scope contract

- [ ] 2.1 Ввести model/classification для `main` / `subsession` / `unknown` на основе indexed session metadata
- [ ] 2.2 Расширить backend/frontend types, чтобы project catalog и `ProjectMetricsResponse.sessions[]` несли session scope metadata

## 3. Project metrics query and UI

- [ ] 3.1 Расширить `query_project_metrics` фильтром scope и обновить aggregation по отфильтрованному набору contributing sessions
- [ ] 3.2 Обновить materialized `SessionMetrics` и versioning stale-check, если для session scope нужен recompute stored metrics
- [ ] 3.3 Переключить `ProjectMetricsScreen` на новый project catalog source и добавить filter control `all` / `main` / `subsession`
- [ ] 3.4 Синхронизировать summary cards, charts, contributing sessions list и empty/degraded states с новым scope filter

## 4. Verification

- [ ] 4.1 Добавить backend tests для полного project catalog, scope classification и filtered project metrics query
- [ ] 4.2 Добавить frontend tests для selector, scope filter и empty/degraded rendering на экране `Project metrics`
