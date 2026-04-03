# Базовый эталон Python-источника (Step 1)

Дата фиксации: 2026-04-04

Цель: получить воспроизводимый baseline-артефакт из **staged tree** исходного Python-репозитория без использования грязного worktree.

## Исходные данные

- Source repo: `/home/alko/develop/open-source/ai/infrastructure/codex-worker`
- Tree hash (staged tree): `c736a6bcb529fc042d70f79be92520e48242b38f`
- HEAD context: `2e18f3ab5b79d0b3cc7bccf7e0dde23ffd19507c`
- Целевой артефакт: `tests/fixtures/source-baseline/python-reference.tar`
- Manifest: `tests/fixtures/source-baseline/manifest.txt`

## Воспроизводимая процедура

Важно: baseline строится из tree hash через `git archive`, поэтому **untracked-файлы не попадают** в baseline.

Также важно: после распаковки выполняется явная нормализация прав, чтобы итоговый tar не зависел от локального `umask`.

```bash
set -euo pipefail

SRC_REPO=/home/alko/develop/open-source/ai/infrastructure/codex-worker
DST_REPO=/home/alko/develop/open-source/ai/infrastructure/codex-worker-rs
TREE_HASH=c736a6bcb529fc042d70f79be92520e48242b38f
HEAD_CONTEXT=2e18f3ab5b79d0b3cc7bccf7e0dde23ffd19507c

mkdir -p "$DST_REPO/tests/fixtures/source-baseline"
WORKDIR="$(mktemp -d)"
trap 'rm -rf "$WORKDIR"' EXIT

mkdir -p "$WORKDIR/python-reference"
git -C "$SRC_REPO" archive --format=tar "$TREE_HASH" \
  | tar -xf - -C "$WORKDIR/python-reference"

# Нормализация прав: устраняем зависимость от umask.
find "$WORKDIR/python-reference" -type d -print0 | xargs -0 chmod 755
find "$WORKDIR/python-reference" -type f -print0 | xargs -0 chmod 644

# Возвращаем исполняемый бит только для файлов 100755 из tree.
git -C "$SRC_REPO" ls-tree -r "$TREE_HASH" \
  | awk '$1 == "100755" {print $4}' \
  | while IFS= read -r relpath; do
      chmod 755 "$WORKDIR/python-reference/$relpath"
    done

tar --format=ustar \
  --sort=name \
  --mtime='UTC 1970-01-01' \
  --owner=0 \
  --group=0 \
  --numeric-owner \
  -cf "$DST_REPO/tests/fixtures/source-baseline/python-reference.tar" \
  -C "$WORKDIR" \
  python-reference

ARCHIVE_SHA256="$(sha256sum "$DST_REPO/tests/fixtures/source-baseline/python-reference.tar" | awk '{print $1}')"

cat > "$DST_REPO/tests/fixtures/source-baseline/manifest.txt" <<EOF
source_repo=/home/alko/develop/open-source/ai/infrastructure/codex-worker
tree_hash=$TREE_HASH
head_context=$HEAD_CONTEXT
archive_path=tests/fixtures/source-baseline/python-reference.tar
archive_sha256=$ARCHIVE_SHA256
untracked_excluded=true
generation_method=git-archive-tree-plus-permissions-normalization-deterministic-tar
EOF
```

## Проверка

```bash
set -euo pipefail

SRC_REPO=/home/alko/develop/open-source/ai/infrastructure/codex-worker
DST_REPO=/home/alko/develop/open-source/ai/infrastructure/codex-worker-rs
TREE_HASH=c736a6bcb529fc042d70f79be92520e48242b38f

git -C "$SRC_REPO" cat-file -t "$TREE_HASH"      # ожидается: tree
grep '^tree_hash=' "$DST_REPO/tests/fixtures/source-baseline/manifest.txt"
sha256sum "$DST_REPO/tests/fixtures/source-baseline/python-reference.tar"
ls -l "$DST_REPO/tests/fixtures/source-baseline/"
```
