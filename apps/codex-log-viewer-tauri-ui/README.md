# Codex Log Viewer Tauri UI

Отдельное desktop-приложение для просмотра Codex sessions на базе `agmmnn/tauri-ui`.

## Что внутри

- React + Vite frontend с Tailwind/shadcn shell.
- Tauri 2 shell с startup flash prevention.
- `tauri-ui` batteries: external link guard и dev-only debug panel по `Cmd/Ctrl + D`.
- Read-only IPC к каталогу сессий, preview и live tail.
- Отдельный app identity, не заменяющий `apps/codex-log-viewer`.

## Примечание по scaffold

- Upstream `create-tauri-ui` требует `bun`.
- Этот app адаптирован под текущий repo и npm-based workflow, но использует ключевые runtime-паттерны и батарейки из upstream `tauri-ui`.

## Локальные команды

```bash
npm install
npm run dev
npm run build
npm run tauri:dev
npm run tauri:build
```
