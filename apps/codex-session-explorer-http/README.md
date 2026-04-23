# codex-session-explorer-http

`codex-session-explorer-http` это отдельное локальное HTTP приложение для browser-based запуска
`codex-session-explorer` без Tauri WebView.

## Что делает binary

- поднимает локальный HTTP server;
- определяет `CODEX_HOME` автоматически или принимает его через `--codex-home`;
- раздаёт встроенные frontend assets из `apps/codex-session-explorer/dist`;
- обслуживает same-origin viewer API `/api/viewer/*` поверх общего crate
  `codex-session-explorer-backend`.

## Сборка и запуск

```bash
npm --prefix ../codex-session-explorer run build
cargo run -p codex-session-explorer-http -- --port 4321
```

Если `CODEX_HOME` не находится автоматически, передайте его явно:

```bash
cargo run -p codex-session-explorer-http -- --codex-home ~/.codex --port 4321
```

После старта приложение печатает локальный URL, который можно открыть в браузере.
