pub const THREAD_STARTED: &str = "thread.started";

pub const AGENT_STARTED: &str = "agent.started";
pub const AGENT_COMPLETED: &str = "agent.completed";
pub const AGENT_FAILED: &str = "agent.failed";

pub const MESSAGE_AGENT: &str = "message.agent";
pub const AGENT_REASONING: &str = "agent.reasoning";
pub const AGENT_SESSION: &str = "agent.session";
pub const AGENT_META: &str = "agent.meta";
pub const AGENT_ABORTED: &str = "agent.aborted";
pub const MESSAGE_COMMENTARY: &str = "message.commentary";
pub const MESSAGE_USER: &str = "message.user";
pub const TASK_STARTED: &str = "task.started";
pub const TASK_COMPLETED: &str = "task.completed";
pub const RUNTIME_CONTEXT: &str = "runtime.context";
pub const CONTEXT_COMPACTED: &str = "context.compacted";
pub const INFO_TOKENS: &str = "info.tokens";

pub const TOOL_CALL: &str = "tool.call";
pub const TOOL_RESULT: &str = "tool.result";
pub const SHELL_CALL: &str = "shell.call";
pub const SHELL_RESULT: &str = "shell.result";
pub const MCP_CALL: &str = "mcp.call";
pub const MCP_RESULT: &str = "mcp.result";
pub const STDIN_WRITE: &str = "stdin.write";
pub const WEB_SEARCH: &str = "web.search";
pub const PLAN_UPDATE: &str = "plan.update";
pub const PATCH_APPLY: &str = "patch.apply";
pub const COLLAB_SPAWN_AGENT: &str = "collab.spawn_agent";
pub const COLLAB_SEND_INPUT: &str = "collab.send_input";
pub const COLLAB_WAIT: &str = "collab.wait";
pub const COLLAB_CLOSE_AGENT: &str = "collab.close_agent";
pub const COLLAB_RESUME_AGENT: &str = "collab.resume_agent";

pub const FILE_CHANGE: &str = "file.change";
pub const TODO_UPDATE: &str = "todo.update";
pub const ERROR: &str = "error";
pub const RAW_UNPARSED: &str = "raw.unparsed";
pub const STDERR_LINE: &str = "stderr.line";
