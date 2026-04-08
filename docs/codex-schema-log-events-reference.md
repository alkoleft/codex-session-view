# Reference: `/tmp/codex-schema` для событий лога

Этот документ фиксирует схемы из `/tmp/codex-schema`, которые относятся к event/log surface или
могут использоваться как ближайший upstream reference для наших log events.

Важно:

- это не прямой контракт нашего persisted `rollout-*.jsonl`;
- raw envelopes `event_msg` и `session_meta` в схеме не найдены;
- legacy persisted names вроде `exec_command_end` и `patch_apply_end` в схеме не найдены;
- ниже перечислены все типы, найденные в рамках целевого поиска по event/log-related именам и
  поверхностям.

## 1. Наши события и ближайшие schema-типы

| Наш event / envelope | Ближайший тип в schema | Файл | Степень соответствия | Комментарий |
| --- | --- | --- | --- | --- |
| `response_item.message` | `ResponseItem(type="message")` | `/tmp/codex-schema/ResponseItem.ts` | `exact` | Прямой raw item-типа Response API. |
| `response_item.reasoning` | `ResponseItem(type="reasoning")` | `/tmp/codex-schema/ResponseItem.ts` | `exact` | Прямой raw reasoning item. |
| `response_item.function_call` | `ResponseItem(type="function_call")` | `/tmp/codex-schema/ResponseItem.ts` | `exact` | Содержит `call_id`. |
| `response_item.function_call_output` | `ResponseItem(type="function_call_output")` | `/tmp/codex-schema/ResponseItem.ts` | `exact` | Содержит `call_id` и `output`. |
| `response_item.custom_tool_call` | `ResponseItem(type="custom_tool_call")` | `/tmp/codex-schema/ResponseItem.ts` | `exact` | Ближайший raw-тип для custom tool start, включая patch-подобные вызовы. |
| `response_item.custom_tool_call_output` | `ResponseItem(type="custom_tool_call_output")` | `/tmp/codex-schema/ResponseItem.ts` | `exact` | Ближайший raw-тип для custom tool completion. |
| `response_item.local_shell_call` | `ResponseItem(type="local_shell_call")` | `/tmp/codex-schema/ResponseItem.ts` | `exact` | Есть `call_id`, `status`, `action`. |
| `message.user` | `ThreadItem(type="userMessage")` | `/tmp/codex-schema/v2/ThreadItem.ts` | `close` | Ближайшее каноническое item-представление. |
| `message.assistant` / `message.agent` | `ThreadItem(type="agentMessage")` | `/tmp/codex-schema/v2/ThreadItem.ts` | `close` | Для streaming ближе всего также `AgentMessageDeltaNotification`. |
| `message.plan` / `todo.update` | `ThreadItem(type="plan")` | `/tmp/codex-schema/v2/ThreadItem.ts` | `close` | Для дельт полезен `PlanDeltaNotification`; tool-shaped `todo.update` соответствует `update_plan`. |
| `agent.reasoning` | `ThreadItem(type="reasoning")` | `/tmp/codex-schema/v2/ThreadItem.ts` | `close` | Для streaming ближе `Reasoning*DeltaNotification`. |
| `shell.call` / `shell.result` | `ThreadItem(type="commandExecution")` | `/tmp/codex-schema/v2/ThreadItem.ts` | `close` | На typed item-слое shell сведён к `commandExecution`. |
| `patch.apply` | `ThreadItem(type="fileChange")` | `/tmp/codex-schema/v2/ThreadItem.ts` | `close` | Патч-представление выражено через file change и patch status. |
| `patch.apply` diff | `FileUpdateChange` | `/tmp/codex-schema/v2/FileUpdateChange.ts` | `close` | Содержит `path`, `kind`, `diff`. |
| `user.input.request` | `ToolRequestUserInputParams` | `/tmp/codex-schema/v2/ToolRequestUserInputParams.ts` | `close` | На typed schema есть отдельная модель tool request user input. |
| `collab.spawn_agent` | `ThreadItem(type="collabAgentToolCall")` | `/tmp/codex-schema/v2/ThreadItem.ts` | `close` | Канонический collab item с sender/receiver/model/agentsStates. |
| `collab.send_input` | `ThreadItem(type="collabAgentToolCall")` | `/tmp/codex-schema/v2/ThreadItem.ts` | `close` | Конкретный tool различается через `CollabAgentTool`. |
| `collab.wait` | `ThreadItem(type="collabAgentToolCall")` | `/tmp/codex-schema/v2/ThreadItem.ts` | `close` | Статус и tool вынесены в отдельные типы. |
| `collab.close_agent` | `ThreadItem(type="collabAgentToolCall")` | `/tmp/codex-schema/v2/ThreadItem.ts` | `close` | Ближайшая typed-модель collab lifecycle. |
| `collab.resume_agent` | `ThreadItem(type="collabAgentToolCall")` | `/tmp/codex-schema/v2/ThreadItem.ts` | `close` | Прямого event name нет, есть общая collab item-модель. |
| `mcp.call` / `mcp.result` | `ThreadItem(type="mcpToolCall")` | `/tmp/codex-schema/v2/ThreadItem.ts` | `close` | Каноническая typed-модель MCP tool call. |
| `web.search` | `ThreadItem(type="webSearch")` | `/tmp/codex-schema/v2/ThreadItem.ts` | `close` | Typed item-level web search. |
| `context.compacted` | `ThreadItem(type="contextCompaction")` | `/tmp/codex-schema/v2/ThreadItem.ts` | `close` | Более современная item-модель; `ContextCompactedNotification` отмечен deprecated. |
| `info.tokens` | `ThreadTokenUsageUpdatedNotification` | `/tmp/codex-schema/v2/ThreadTokenUsageUpdatedNotification.ts` | `loose` | Есть typed notification по token usage, но не raw rollout item. |
| `task.started` | `TurnStartedNotification` | `/tmp/codex-schema/v2/TurnStartedNotification.ts` | `loose` | Прямого legacy task-start item нет; ближе всего turn/item lifecycle. |
| `task.completed` | `TurnCompletedNotification` | `/tmp/codex-schema/v2/TurnCompletedNotification.ts` | `loose` | Прямого legacy task-complete item нет; ближе всего turn/item lifecycle. |
| `runtime.context` | отсутствует прямой typed event | — | `missing` | Непрямо пересекается с thread/turn metadata, но прямой item не найден. |
| `event_msg` | отсутствует | — | `missing` | Raw envelope в typed schema не найден. |
| `session_meta` | отсутствует | — | `missing` | Raw persisted envelope в typed schema не найден. |
| `exec_command_end` | отсутствует | — | `missing` | Legacy persisted-log name отсутствует; ближе `CommandExecResponse`. |
| `patch_apply_end` | отсутствует | — | `missing` | Legacy persisted-log name отсутствует; ближе `ThreadItem(fileChange)`. |

## 2. Все найденные schema-типы, которые относятся или могут относиться к log events

### 2.1. Верхнеуровневые агрегаторы и базовые модели

| Тип / сущность | Файл | Категория | Почему включён |
| --- | --- | --- | --- |
| `ResponseItem` | `/tmp/codex-schema/ResponseItem.ts` | raw item schema | Главная схема raw response items. |
| `ServerNotification` | `/tmp/codex-schema/ServerNotification.ts` | notification union | Центральный индекс серверных уведомлений. |
| `ClientRequest` | `/tmp/codex-schema/ClientRequest.ts` | request union | Полезен для связи request ↔ event/notification surface. |
| `ThreadItem` | `/tmp/codex-schema/v2/ThreadItem.ts` | canonical item schema | Самая полезная item-модель для typed event layer. |
| `Turn` | `/tmp/codex-schema/v2/Turn.ts` | turn model | Полезен для turn lifecycle и grouping. |
| `Thread` | `/tmp/codex-schema/v2/Thread.ts` | thread model | Полезен для thread lifecycle и metadata. |

### 2.2. Item lifecycle и raw item completion

| Тип / сущность | Файл | Категория | Почему включён |
| --- | --- | --- | --- |
| `ItemStartedNotification` | `/tmp/codex-schema/v2/ItemStartedNotification.ts` | item lifecycle | Start события typed item-слоя. |
| `ItemCompletedNotification` | `/tmp/codex-schema/v2/ItemCompletedNotification.ts` | item lifecycle | Complete события typed item-слоя. |
| `RawResponseItemCompletedNotification` | `/tmp/codex-schema/v2/RawResponseItemCompletedNotification.ts` | raw item lifecycle | Связывает `ResponseItem` с thread/turn envelope. |

### 2.3. Сообщения, план и reasoning

| Тип / сущность | Файл | Категория | Почему включён |
| --- | --- | --- | --- |
| `AgentMessageDeltaNotification` | `/tmp/codex-schema/v2/AgentMessageDeltaNotification.ts` | message streaming | Потоковые дельты agent message. |
| `PlanDeltaNotification` | `/tmp/codex-schema/v2/PlanDeltaNotification.ts` | plan streaming | Потоковые дельты плана. |
| `ReasoningSummaryTextDeltaNotification` | `/tmp/codex-schema/v2/ReasoningSummaryTextDeltaNotification.ts` | reasoning streaming | Дельты summary reasoning. |
| `ReasoningSummaryPartAddedNotification` | `/tmp/codex-schema/v2/ReasoningSummaryPartAddedNotification.ts` | reasoning streaming | Добавление частей reasoning summary. |
| `ReasoningTextDeltaNotification` | `/tmp/codex-schema/v2/ReasoningTextDeltaNotification.ts` | reasoning streaming | Потоковый reasoning text. |
| `TurnPlanUpdatedNotification` | `/tmp/codex-schema/v2/TurnPlanUpdatedNotification.ts` | turn state | Turn-level update плана. |
| `TurnDiffUpdatedNotification` | `/tmp/codex-schema/v2/TurnDiffUpdatedNotification.ts` | turn state | Turn-level diff update, потенциально полезен для change tracking. |

### 2.4. Command execution / shell / terminal

| Тип / сущность | Файл | Категория | Почему включён |
| --- | --- | --- | --- |
| `CommandExecParams` | `/tmp/codex-schema/v2/CommandExecParams.ts` | request schema | Typed запуск `command/exec`. |
| `CommandExecResponse` | `/tmp/codex-schema/v2/CommandExecResponse.ts` | response schema | Финальный buffered result `command/exec`. |
| `CommandExecOutputDeltaNotification` | `/tmp/codex-schema/v2/CommandExecOutputDeltaNotification.ts` | output streaming | Streaming stdout/stderr chunks для `command/exec`. |
| `CommandExecutionOutputDeltaNotification` | `/tmp/codex-schema/v2/CommandExecutionOutputDeltaNotification.ts` | item streaming | Item-level output delta для `commandExecution`. |
| `TerminalInteractionNotification` | `/tmp/codex-schema/v2/TerminalInteractionNotification.ts` | terminal interaction | PTY / interactive terminal surface. |
| `CommandExecWriteParams` | `/tmp/codex-schema/v2/CommandExecWriteParams.ts` | request schema | Ввод в запущенный exec session. |
| `CommandExecWriteResponse` | `/tmp/codex-schema/v2/CommandExecWriteResponse.ts` | response schema | Ответ на stdin write. |
| `CommandExecTerminateParams` | `/tmp/codex-schema/v2/CommandExecTerminateParams.ts` | request schema | Завершение exec session. |
| `CommandExecTerminateResponse` | `/tmp/codex-schema/v2/CommandExecTerminateResponse.ts` | response schema | Ответ на terminate. |
| `CommandExecResizeParams` | `/tmp/codex-schema/v2/CommandExecResizeParams.ts` | request schema | Resize PTY session. |
| `CommandExecResizeResponse` | `/tmp/codex-schema/v2/CommandExecResizeResponse.ts` | response schema | Ответ на resize. |
| `CommandExecOutputStream` | `/tmp/codex-schema/v2/CommandExecOutputStream.ts` | enum / helper | Метка output stream для `command/exec/outputDelta`. |
| `CommandExecTerminalSize` | `/tmp/codex-schema/v2/CommandExecTerminalSize.ts` | helper type | PTY terminal size. |
| `CommandExecutionSource` | `/tmp/codex-schema/v2/CommandExecutionSource.ts` | enum / helper | Источник `commandExecution` item. |
| `CommandExecutionStatus` | `/tmp/codex-schema/v2/CommandExecutionStatus.ts` | enum / helper | Статус `commandExecution`. |
| `ThreadShellCommandParams` | `/tmp/codex-schema/v2/ThreadShellCommandParams.ts` | request schema | Отдельный shell-related API на thread-уровне. |
| `ExecCommandApprovalParams` | `/tmp/codex-schema/ExecCommandApprovalParams.ts` | approval schema | Содержит `callId`, полезен как reference для correlation. |
| `CommandExecutionRequestApprovalParams` | `/tmp/codex-schema/v2/CommandExecutionRequestApprovalParams.ts` | approval schema | Approval-модель для command execution. |
| `CommandExecutionRequestApprovalResponse` | `/tmp/codex-schema/v2/CommandExecutionRequestApprovalResponse.ts` | approval schema | Response-модель approval command execution. |
| `CommandExecutionApprovalDecision` | `/tmp/codex-schema/v2/CommandExecutionApprovalDecision.ts` | enum / helper | Typed approval decision для command execution. |

### 2.5. File change / patch

| Тип / сущность | Файл | Категория | Почему включён |
| --- | --- | --- | --- |
| `FileChangeOutputDeltaNotification` | `/tmp/codex-schema/v2/FileChangeOutputDeltaNotification.ts` | file change streaming | Дельты file-change item. |
| `FileUpdateChange` | `/tmp/codex-schema/v2/FileUpdateChange.ts` | file change model | Содержит `path`, `kind`, `diff`. |
| `PatchChangeKind` | `/tmp/codex-schema/v2/PatchChangeKind.ts` | enum / helper | Тип изменения патча. |
| `PatchApplyStatus` | `/tmp/codex-schema/v2/PatchApplyStatus.ts` | enum / helper | Статус patch/file change item. |
| `FileChangeRequestApprovalParams` | `/tmp/codex-schema/v2/FileChangeRequestApprovalParams.ts` | approval schema | Approval-поверхность file change. |
| `FileChangeRequestApprovalResponse` | `/tmp/codex-schema/v2/FileChangeRequestApprovalResponse.ts` | approval schema | Response approval для file change. |
| `FileChangeApprovalDecision` | `/tmp/codex-schema/v2/FileChangeApprovalDecision.ts` | enum / helper | Typed decision для file change approval. |
| `ApplyPatchApprovalParams` | `/tmp/codex-schema/ApplyPatchApprovalParams.ts` | approval schema | Legacy-like approval surface с `callId`. |

### 2.6. MCP / dynamic tool

| Тип / сущность | Файл | Категория | Почему включён |
| --- | --- | --- | --- |
| `McpToolCallProgressNotification` | `/tmp/codex-schema/v2/McpToolCallProgressNotification.ts` | tool streaming | Streaming progress MCP tool call. |
| `McpToolCallResult` | `/tmp/codex-schema/v2/McpToolCallResult.ts` | tool result | Typed result MCP tool call. |
| `McpToolCallError` | `/tmp/codex-schema/v2/McpToolCallError.ts` | tool error | Typed error MCP tool call. |
| `McpToolCallStatus` | `/tmp/codex-schema/v2/McpToolCallStatus.ts` | enum / helper | Статус MCP tool call. |
| `DynamicToolCallParams` | `/tmp/codex-schema/v2/DynamicToolCallParams.ts` | request schema | Dynamic tool invocation, содержит `callId`. |
| `DynamicToolCallResponse` | `/tmp/codex-schema/v2/DynamicToolCallResponse.ts` | response schema | Финальный result dynamic tool call. |
| `DynamicToolCallOutputContentItem` | `/tmp/codex-schema/v2/DynamicToolCallOutputContentItem.ts` | output model | Структура output content dynamic tool call. |
| `DynamicToolCallStatus` | `/tmp/codex-schema/v2/DynamicToolCallStatus.ts` | enum / helper | Статус dynamic tool call. |

### 2.7. Collab / subagent

| Тип / сущность | Файл | Категория | Почему включён |
| --- | --- | --- | --- |
| `CollabAgentTool` | `/tmp/codex-schema/v2/CollabAgentTool.ts` | enum / helper | Конкретный тип collab tool. |
| `CollabAgentToolCallStatus` | `/tmp/codex-schema/v2/CollabAgentToolCallStatus.ts` | enum / helper | Статус collab tool call. |
| `CollabAgentState` | `/tmp/codex-schema/v2/CollabAgentState.ts` | collab state model | Typed состояние subagent/collab agent. |
| `CollabAgentStatus` | `/tmp/codex-schema/v2/CollabAgentStatus.ts` | enum / helper | Дополнительный статус collab agent. |

### 2.8. User input / questionnaire

| Тип / сущность | Файл | Категория | Почему включён |
| --- | --- | --- | --- |
| `ToolRequestUserInputParams` | `/tmp/codex-schema/v2/ToolRequestUserInputParams.ts` | request schema | Прямой typed аналог `request_user_input`. |
| `ToolRequestUserInputQuestion` | `/tmp/codex-schema/v2/ToolRequestUserInputQuestion.ts` | input model | Структура вопроса. |
| `ToolRequestUserInputOption` | `/tmp/codex-schema/v2/ToolRequestUserInputOption.ts` | input model | Структура варианта ответа. |
| `ToolRequestUserInputAnswer` | `/tmp/codex-schema/v2/ToolRequestUserInputAnswer.ts` | input model | Структура ответа. |
| `ToolRequestUserInputResponse` | `/tmp/codex-schema/v2/ToolRequestUserInputResponse.ts` | response schema | Typed response к user input request. |
| `UserInput` | `/tmp/codex-schema/v2/UserInput.ts` | content model | Общая input-модель, полезна для user message / item content. |

### 2.9. Thread / turn lifecycle и realtime

| Тип / сущность | Файл | Категория | Почему включён |
| --- | --- | --- | --- |
| `ThreadStartedNotification` | `/tmp/codex-schema/v2/ThreadStartedNotification.ts` | thread lifecycle | Start thread. |
| `ThreadStatusChangedNotification` | `/tmp/codex-schema/v2/ThreadStatusChangedNotification.ts` | thread lifecycle | Status change thread. |
| `ThreadArchivedNotification` | `/tmp/codex-schema/v2/ThreadArchivedNotification.ts` | thread lifecycle | Archive thread. |
| `ThreadUnarchivedNotification` | `/tmp/codex-schema/v2/ThreadUnarchivedNotification.ts` | thread lifecycle | Unarchive thread. |
| `ThreadClosedNotification` | `/tmp/codex-schema/v2/ThreadClosedNotification.ts` | thread lifecycle | Close thread. |
| `ThreadNameUpdatedNotification` | `/tmp/codex-schema/v2/ThreadNameUpdatedNotification.ts` | thread metadata | Имя thread. |
| `ThreadTokenUsageUpdatedNotification` | `/tmp/codex-schema/v2/ThreadTokenUsageUpdatedNotification.ts` | thread metadata | Token usage update. |
| `TurnStartedNotification` | `/tmp/codex-schema/v2/TurnStartedNotification.ts` | turn lifecycle | Start turn. |
| `TurnCompletedNotification` | `/tmp/codex-schema/v2/TurnCompletedNotification.ts` | turn lifecycle | Complete turn. |
| `ThreadRealtimeStartedNotification` | `/tmp/codex-schema/v2/ThreadRealtimeStartedNotification.ts` | realtime | Realtime thread start. |
| `ThreadRealtimeItemAddedNotification` | `/tmp/codex-schema/v2/ThreadRealtimeItemAddedNotification.ts` | realtime | Добавление realtime item. |
| `ThreadRealtimeTranscriptUpdatedNotification` | `/tmp/codex-schema/v2/ThreadRealtimeTranscriptUpdatedNotification.ts` | realtime | Transcript updates. |
| `ThreadRealtimeOutputAudioDeltaNotification` | `/tmp/codex-schema/v2/ThreadRealtimeOutputAudioDeltaNotification.ts` | realtime | Audio delta. |
| `ThreadRealtimeErrorNotification` | `/tmp/codex-schema/v2/ThreadRealtimeErrorNotification.ts` | realtime | Realtime error. |
| `ThreadRealtimeClosedNotification` | `/tmp/codex-schema/v2/ThreadRealtimeClosedNotification.ts` | realtime | Realtime close. |

### 2.10. Thread / rollout access params

| Тип / сущность | Файл | Категория | Почему включён |
| --- | --- | --- | --- |
| `ThreadStartParams` | `/tmp/codex-schema/v2/ThreadStartParams.ts` | request schema | Содержит `experimentalRawEvents` и `persistExtendedHistory`. |
| `ThreadResumeParams` | `/tmp/codex-schema/v2/ThreadResumeParams.ts` | request schema | Resume surface с опцией persist/history. |
| `ThreadForkParams` | `/tmp/codex-schema/v2/ThreadForkParams.ts` | request schema | Fork surface с опцией rollout/history. |
| `ThreadReadParams` | `/tmp/codex-schema/v2/ThreadReadParams.ts` | request schema | Чтение thread с turns/items из rollout history. |
| `GetConversationSummaryParams` | `/tmp/codex-schema/GetConversationSummaryParams.ts` | request schema | Содержит `rolloutPath` как явную точку входа к persisted history. |

### 2.11. Context compaction

| Тип / сущность | Файл | Категория | Почему включён |
| --- | --- | --- | --- |
| `ContextCompactedNotification` | `/tmp/codex-schema/v2/ContextCompactedNotification.ts` | context lifecycle | Deprecated typed notification для compaction. |

## 3. Где найден `call_id` / `callId`

| Поле | Тип / сущность | Файл | Комментарий |
| --- | --- | --- | --- |
| `call_id` | `ResponseItem(type="local_shell_call")` | `/tmp/codex-schema/ResponseItem.ts` | Snake_case, nullable. |
| `call_id` | `ResponseItem(type="function_call")` | `/tmp/codex-schema/ResponseItem.ts` | Snake_case, обязательный. |
| `call_id` | `ResponseItem(type="tool_search_call")` | `/tmp/codex-schema/ResponseItem.ts` | Snake_case, nullable. |
| `call_id` | `ResponseItem(type="function_call_output")` | `/tmp/codex-schema/ResponseItem.ts` | Snake_case, обязательный. |
| `call_id` | `ResponseItem(type="custom_tool_call")` | `/tmp/codex-schema/ResponseItem.ts` | Snake_case, обязательный. |
| `call_id` | `ResponseItem(type="custom_tool_call_output")` | `/tmp/codex-schema/ResponseItem.ts` | Snake_case, обязательный. |
| `call_id` | `ResponseItem(type="tool_search_output")` | `/tmp/codex-schema/ResponseItem.ts` | Snake_case, nullable. |
| `callId` | `ExecCommandApprovalParams` | `/tmp/codex-schema/ExecCommandApprovalParams.ts` | CamelCase approval surface. |
| `callId` | `ApplyPatchApprovalParams` | `/tmp/codex-schema/ApplyPatchApprovalParams.ts` | CamelCase approval surface. |
| `callId` | `DynamicToolCallParams` | `/tmp/codex-schema/v2/DynamicToolCallParams.ts` | CamelCase typed request surface. |

## 4. Что не найдено в схеме

| Raw / legacy имя | Статус | Комментарий |
| --- | --- | --- |
| `event_msg` | `missing` | Raw envelope отсутствует в typed schema. |
| `session_meta` | `missing` | Raw persisted envelope отсутствует в typed schema. |
| `exec_command_end` | `missing` | Похоже, это legacy persisted-log событие, а не typed schema name. |
| `patch_apply_end` | `missing` | Похоже, это legacy persisted-log событие, а не typed schema name. |

## 5. Короткий вывод

Для log events полезнее всего смотреть в таком порядке:

1. `ResponseItem`
2. `ThreadItem`
3. `ItemStartedNotification`
4. `ItemCompletedNotification`
5. `RawResponseItemCompletedNotification`
6. `ServerNotification`
7. `CommandExecResponse`
8. `CommandExecutionOutputDeltaNotification`
9. `AgentMessageDeltaNotification`
10. `PlanDeltaNotification`

Эти схемы не описывают наш persisted `rollout-*.jsonl` напрямую, но дают достаточно хороший
upstream reference для item-level semantics, delta-streaming и correlation по `call_id`.
