# codex-worker-rs

Rust-порт `codex-worker` с поведенческим паритетом для `run-next` относительно Python-версии.

## Что уже есть

- file-based task engine с `claim/finalize`, heartbeat и stale recovery;
- fail-closed runner для `codex exec --json` с layout артефактов `.codex-worker`;
- нормализация root/subagent событий, импорт `CODEX_HOME/sessions`, event projector и console UI;
- runtime gate `acceptance-v1`, который генерирует отчёт в `target/acceptance-v1/report.json`.

## Быстрый старт

```bash
cargo test
cargo run -- run-next --help
python3 scripts/acceptance_v1.py
```

- `cargo test` прогоняет полный behavioural test suite.
- `cargo run -- run-next --help` проверяет CLI-контракт.
- `python3 scripts/acceptance_v1.py` валидирует `tests/acceptance/manifest.json`, сверяет ids с `docs/parity/acceptance-v1.md` и запускает runtime acceptance-кейсы.

## Devcontainer

- В `.devcontainer/devcontainer.json` зафиксировано воспроизводимое Rust-окружение с `CODEX_HOME=${containerWorkspaceFolder}/.codex-mount`.
- После открытия контейнера доступны те же команды: `cargo test`, `cargo run -- run-next --help`, `python3 scripts/acceptance_v1.py`.
- `@openai/codex` ставится в `postCreateCommand`; `auth.json` монтируется в `.codex-mount/auth.json`.

## Acceptance и CI

- Workflow `.github/workflows/acceptance-v1.yml` запускает тот же runtime harness, что и локальная команда `python3 scripts/acceptance_v1.py`.
- Машиночитаемый источник правды для acceptance-набора: `tests/acceptance/manifest.json`.
- На 2026-04-05 все 8 кейсов `acceptance-v1` проходят локально.
- Настройка branch protection и назначение `acceptance-v1` как `required status check` остаются внешней конфигурацией GitHub и не верифицируются из этого workspace.

## Базовый эталон исходников

Процедура создания воспроизводимого baseline-артефакта из staged tree Python-репозитория описана в `docs/parity/source-baseline.md`.
