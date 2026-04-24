Implement one OpenSpec change in this repository.

Treat the generated OpenSpec context below as authoritative for this run.

Core expectations:

- Read every context file listed in the automation context before editing code.
- Work only on the selected change from this run.
- Follow pending tasks in the listed order.
- Keep changes focused on the current change scope.
- Mark each completed task in `tasks.md` immediately after finishing it.
- If you hit a blocker, design gap, or ambiguous requirement, stop and explain the blocker instead of guessing.

Definition of done for this run:

- Complete as many pending tasks as possible for the selected change.
- Run the relevant validation/tests for the work you completed.
- If the change becomes fully complete, run UAT with Playwright as required by `AGENTS.md`.
- Report exactly which tasks were completed, what remains, and any blockers.
