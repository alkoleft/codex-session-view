# Codex Log Viewer Tauri UI

Отдельное desktop-приложение для просмотра Codex sessions на базе паттернов `agmmnn/tauri-ui`.

## Что внутри

- React + Vite frontend с Tailwind-based shell.
- Tauri 2 shell с startup flash prevention.
- Read-only IPC к каталогу сессий, preview и live tail.
- Отдельный app identity, не заменяющий `apps/codex-log-viewer`.

## Локальные команды

```bash
npm install
npm run dev
npm run build
npm run tauri:dev
npm run tauri:build
```

