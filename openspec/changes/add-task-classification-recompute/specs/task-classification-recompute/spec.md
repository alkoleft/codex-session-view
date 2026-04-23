## ADDED Requirements

### Requirement: Task Metrics Carry Semantic Classification
Система SHALL сохранять semantic classification task metrics в виде устойчивого поля `task_class`.

#### Scenario: Task class is confidently known
- **WHEN** raw task signals достаточно сильны для однозначной classification
- **THEN** система SHALL сохранить `task_class`
- **AND** classification SHALL быть пригодна для downstream analytics без повторного вычисления

#### Scenario: Task class is ambiguous
- **WHEN** raw task signals недостаточны для уверенной classification
- **THEN** система SHALL использовать `unknown` или `partial` classification semantics
- **AND** система MUST NOT подменять ambiguity произвольным task class

### Requirement: Classification Carries Source And Confidence
Система SHALL сохранять рядом с `task_class` поля происхождения и достоверности classification.

#### Scenario: Classification is materialized
- **WHEN** task class рассчитан из raw task signals
- **THEN** система SHALL сохранить `task_class_source` и `task_class_confidence`
- **AND** эти поля SHALL быть доступны downstream analytics и diagnostics

### Requirement: Metrics Recompute Is Explicit
Система SHALL предоставлять отдельную команду пересчёта materialized task/session metrics после изменения правил classification.

#### Scenario: Classification rules changed
- **WHEN** правила classification были изменены
- **THEN** пользователь или automation SHALL иметь возможность запустить отдельную recompute command
- **AND** recompute SHALL обновить materialized metrics по новым правилам

#### Scenario: Read path uses materialized task classes
- **WHEN** потребитель читает task или session metrics
- **THEN** read path SHALL использовать уже materialized classification
- **AND** система MUST NOT требовать classifier version negotiation для обычного чтения
