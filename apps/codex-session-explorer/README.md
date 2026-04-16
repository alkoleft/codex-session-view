# codex-session-explorer

`codex-session-explorer` это desktop-приложение для интерактивного разбора логов Codex-сессий. Оно помогает открывать сессии из локального `CODEX_HOME`, просматривать timeline событий, исследовать агентские ветки и читать детали shell/patch/user-input событий без ручного парсинга сырых логов.

## Что внутри

- React + Vite frontend с Tailwind/shadcn shell.
- Tauri 2 shell с startup flash prevention.
- `tauri-ui` batteries: external link guard и dev-only debug panel по `Cmd/Ctrl + D`.
- Read-only IPC к каталогу сессий, preview и live tail в том же `src-tauri` crate.
- Продуктовое имя приложения: `codex-session-explorer`.

## Скриншоты

### Основной экран

![Окно выбора сессии codex-session-explorer](../../docs/screenshots/codex-session-explorer-real-session-picker.png)

### Выбранная сессия

![Выбранная сессия codex-session-explorer](../../docs/screenshots/codex-session-explorer-real-selected-session.png)

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
