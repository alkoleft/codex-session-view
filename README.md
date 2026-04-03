# codex-worker-rs

Rust-порт `codex-worker` с поэтапным переносом поведения Python-версии.

## CI-проверка `acceptance-v1` (Этап 0.3, временная репозиторная)

В репозитории добавлен workflow `.github/workflows/acceptance-v1.yml` с job `acceptance-v1`.

Текущий объём проверки (Этап 0.3):

- валидность JSON в `tests/acceptance/manifest.json`;
- непустой список кейсов и корректный формат id `ACPT-###`;
- обязательные поля кейсов (`id`, `title`, `priority`, `covers`, `rust_stage`, `status`);
- синхронизация id между `tests/acceptance/manifest.json` и `docs/parity/acceptance-v1.md`.

Ограничение текущего этапа:

- Полноценный запуск runtime acceptance-сценариев (выполнение самих кейсов) не реализуется в Этапе 0.3 и добавляется позже, на этапах реализации runtime и финальной parity-проверки.

Важно:

- Этап 0.3 фиксирует только репозиторную часть проверки (`.github/workflows/acceptance-v1.yml` + проверка артефактов).
- Настройка branch protection и назначение `acceptance-v1` как `required status check` выполняются вне репозитория, в настройках GitHub.
- Проверить required-check policy из этого workspace нельзя: нет доступа к remote/config GitHub.
