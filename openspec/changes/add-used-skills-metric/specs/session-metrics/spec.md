## ADDED Requirements

### Requirement: Used skills expose numeric count alongside identifier list
Система SHALL публиковать `used_skills` как metric group, содержащую и список explicit
skill identifiers, и числовой `count`, вычисленный из тех же usage markers.

#### Scenario: Session contains explicit skill usage markers
- **WHEN** нормализованные события сессии содержат один или несколько explicit `skill_identifiers`
  markers
- **THEN** система SHALL дедуплицировать identifiers внутри этой сессии
- **THEN** `used_skills.identifiers` SHALL содержать unique identifiers в стабильном порядке
- **THEN** `used_skills.count` SHALL равняться количеству этих unique identifiers
- **AND** `used_skills.count` SHALL иметь тот же `coverage` и `source`, что и сама metric group

#### Scenario: Session does not contain explicit skill usage markers
- **WHEN** в сессии нет явных `skill_identifiers` markers
- **THEN** `used_skills.count` SHALL иметь `coverage = unknown`
- **AND** система MUST NOT подменять это значение нулём
- **AND** система MUST NOT выводить `used_skills.count` из `skills_count`, `runtime_context.skills`
  или quoted `skills_instructions`

#### Scenario: Project metrics aggregate used skills for a time window
- **WHEN** потребитель запрашивает project metrics для окна, в котором есть сессии с известным
  `used_skills`
- **THEN** project-level `used_skills.identifiers` SHALL содержать unique identifiers по всем
  contributing sessions
- **THEN** project-level `used_skills.count` SHALL равняться количеству unique identifiers в этом
  rollup
- **AND** `used_skills.count` SHALL сохранять `partial` или `unknown`, если coverage contributing
  sessions не полностью известен
