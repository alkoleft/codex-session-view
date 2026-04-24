## Context

`ProjectMetricsScreen` уже умеет загружать project-level метрики, строить интерактивный summary
chart через `Recharts`, переключать серии и открывать contributing sessions. Проблема не в
недостатке данных, а в визуальной иерархии: экран одновременно показывает крупный header с
несколькими control-группами, project context banner, сетку summary cards, подробный блок
legend/toggles, zoom-toolbar, вспомогательные пояснения, отдельную active-point карточку и
самостоятельную колонку contributing sessions.

Из-за этого пользователь тратит внимание на чтение множества вторичных блоков до того, как добирает
главный сигнал из графика. В рамках этого change нужно улучшить информационную композицию без
смены backend contract и без изобретения нового dashboard framework. Визуальное направление:
сдержанный analytical workspace, где график становится главным полотном, а детали раскрываются по
мере надобности.

## Goals / Non-Goals

**Goals:**

- Сделать chart viewport главным фокусом экрана `Project metrics`.
- Уменьшить постоянное присутствие статической информации без потери функциональности.
- Свести secondary UI к понятным зонам: compact toolbar, optional summary, contextual inspector.
- Сохранить текущие сценарии: filters, coverage semantics, zoom, series toggles, session
  drill-down, empty/error/degraded states.
- Обеспечить понятный responsive layout, в котором график не уходит ниже длинной колонны статичных
  блоков.

**Non-Goals:**

- Не менять backend query `query_project_metrics` и shape `ProjectMetricsResponse`.
- Не добавлять новые типы метрик, новые filter dimensions или multi-project compare.
- Не заменять `Recharts` и не переписывать view-model с нуля.
- Не удалять supporting information полностью: задача в иерархии и progressive disclosure, а не в
  потере аналитического контекста.

## Decisions

### 1. Экран делится на три устойчивые зоны: toolbar, chart stage, inspector

Решение: перестроить `ProjectMetricsScreen` в три уровня:

- compact toolbar наверху для project/window/spawn controls и краткого project state;
- основной chart stage, занимающий большую часть ширины и высоты;
- contextual inspector, который показывает active point details, sessions list и вспомогательные
  breakdowns как связанную вторичную панель.

Причина: текущий экран смешивает control layer, summary layer и inspection layer в одном потоке,
из-за чего график визуально оказывается лишь одним из многих блоков. Разделение на зоны создаёт
устойчивую иерархию для чтения: сначала тренд, затем объяснение выбранной точки.

Альтернатива: оставить текущий поток карточек и только уменьшить отступы. Это косметически снизит
высоту, но не уберёт конкуренцию между графиком и вторичными блоками.

### 2. Summary и help-блоки переходят в progressive disclosure

Решение: summary cards, coverage help и часть controls отображать в компактном виде по умолчанию:
короткая KPI-лента, collapsible section `Details`, popover/sheet для series catalog или короткая
toolbar summary вместо большой сетки карточек и постоянных пояснительных баннеров.

Причина: эти данные нужны, но не должны постоянно занимать первую полосу экрана. Пользователь
должен видеть их по запросу или в сжатом виде, не теряя контекст.

Альтернатива: полностью удалить summary cards и подсказки. Это уменьшит clutter, но ухудшит
быструю ориентировку и понимание coverage semantics.

### 3. Series selection становится компактным control surface, а не каталогом карточек

Решение: заменить текущую grid из крупных toggle-cards на более плотный control surface:
segmented list, compact chips, grouped dropdown или отдельный expandable panel с категориями
`operational` / `derived`.

Причина: сейчас выбор серий сам по себе выглядит как отдельный экран и визуально спорит с
графиком. Для ежедневного анализа важнее быстро включить 1-3 метрики, чем перечитывать длинные
description-строки у каждой series.

Альтернатива: оставить карточки и скрывать только часть текста. Это уменьшит высоту, но сохранит
тяжёлый визуальный вес control-grid.

### 4. Active point details и contributing sessions объединяются в единый inspector flow

Решение: использовать общий inspector справа на desktop и снизу/в drawer на узких экранах.
Inspector показывает сначала активную сессию и ключевые значения выбранной точки, а ниже тот же
связанный contributing-session list, где выбранная строка синхронизирована с chart hover/click.

Причина: сейчас active point panel и contributing sessions живут как два почти независимых слоя.
Объединённый inspector делает их последовательными частями одного сценария расследования.

Альтернатива: сохранить отдельную широкую active-point карточку под графиком и независимую колонку
сессий справа. Это по-прежнему дробит внимание между несколькими областями.

### 5. Системные состояния остаются явными, но занимают полноэкранное место только когда активны

Решение: `loading`, `error`, `empty`, `degraded`, `unknown-only` состояния показывать как явные
alerts/banners в верхней зоне chart stage или toolbar. Когда состояние не активно, соответствующие
объяснения не держать постоянно в layout.

Причина: пользователю нужна честная диагностика, но не постоянная полоса служебного текста.
Состояние должно быть видно в момент проблемы, а не занимать место всегда.

Альтернатива: оставить постоянные explanatory bars под toolbar. Это сохраняет перегрузку, даже
когда экран работает нормально.

### 6. Responsive layout оптимизируется вокруг сохранения ширины графика

Решение: на desktop использовать широкую chart area и относительно узкий inspector rail. На
tablet/mobile secondary content уходит под график в accordion/drawer, а toolbar сворачивается в
несколько строк без огромных fixed-width controls.

Причина: пользователь прямо указывает на потерю фокуса на графиках. Значит breakpoint strategy
должна защищать chart area, а не только механически переносить карточки вниз.

Альтернатива: оставить текущий grid и позволить всем блокам просто stack-иться по вертикали. Это
делает график ещё менее заметным на узких экранах.

## Risks / Trade-offs

- [Risk] Слишком агрессивное уплотнение скроет полезный summary-контекст. -> Mitigation: у
  summary остаётся compact default state и явное раскрытие по требованию.
- [Risk] Новый inspector усложнит keyboard navigation и focus management. -> Mitigation: сохранять
  линейный tab order, aria-labels и явную синхронизацию active selection.
- [Risk] Перенос controls в compact toolbar ухудшит discoverability некоторых функций. ->
  Mitigation: держать частые действия видимыми, а редкие series/help controls выносить в раскрытие
  с понятными label.
- [Risk] На малых экранах drawer/accordion для inspector может скрывать связь с графиком. ->
  Mitigation: держать выбранную точку и кнопку открытия inspector рядом с chart stage.

## Migration Plan

1. Выделить в компоненте отдельные layout sections: toolbar, chart stage, inspector.
2. Упростить presentation слоя для summary cards, coverage help и series controls, не меняя
   project metrics data flow.
3. Пересобрать active-point и contributing-session UI в один synced inspector.
4. Обновить responsive breakpoints и проверить empty/error/degraded scenarios.
5. Зафиксировать новый UX через component tests и `openspec validate`.

Rollback: вернуть прежнюю композицию `ProjectMetricsScreen`; backend и view-model остаются
совместимыми, потому что change ограничен presentation/layout слоем.

## Open Questions

- Какой compact pattern лучше подходит текущему UI-стеку для выбора series: chips, dropdown или
  collapsible side panel?
- Нужно ли оставлять summary cards всегда видимыми одной строкой на desktop, или по умолчанию
  скрывать их за `Overview` toggle?
- Должен ли inspector по умолчанию быть открыт всегда, или на узких экранах лучше делать его
  явным secondary action после выбора точки?
