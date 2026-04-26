## ADDED Requirements

### Requirement: Side-panel tab content keeps a real independent scroll owner
Экран `Project metrics` SHALL обеспечивать для tab content правой панели рабочий независимый
vertical scroll-owner при переполнении контента.

#### Scenario: Scrolling long side-panel content
- **WHEN** пользователь открывает вкладку правой панели с контентом длиннее доступной высоты
- **THEN** эта вкладка MUST прокручиваться внутри side panel
- **THEN** main area и chart viewport MUST не перехватывать этот scroll вместо side-panel content

### Requirement: Compact side-panel controls keep explicit on-off readability
Экран `Project metrics` SHALL делать включённые и выключенные состояния compact controls визуально
различимыми без опоры только на текстовое значение.

#### Scenario: Reading compact control state
- **WHEN** пользователь смотрит на compact badges, pills или action buttons в `Series`, `Chart` или
  `Anomalies`
- **THEN** включённое и выключенное состояние MUST различаться по заметному сочетанию fill, border
  или text contrast
- **THEN** это различие MUST оставаться читаемым без необходимости сначала кликать по контролу
