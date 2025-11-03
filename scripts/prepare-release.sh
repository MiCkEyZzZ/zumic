#!/usr/bin/env bash
set -euo pipefail

# Script to prepare a new release
# Usage: ./scripts/prepare-release.sh v1.2.3

VERSION="${1:-}"

if [ -z "$VERSION" ]; then
  echo "Usage: $0 v1.2.3"
  exit 1
fi

# Удаление префикса "v" для Cargo.toml
VERSION_NUMBER="${VERSION#v}"

echo "Preparing release $VERSION..."

# 1. Обновление версии в Cargo.toml
echo "Updating Cargo.toml version to $VERSION_NUMBER..."
sed -i.bak "s/^version = \".*\"/version = \"$VERSION_NUMBER\"/" Cargo.toml
rm Cargo.toml.bak

# 2. Обновление Cargo.lock
echo "Updating Cargo.lock..."
cargo update -p zumic

# 3. Создание записи в журнале изменений (если она не существует).
if [ -f CHANGELOG.md ]; then
  if ! grep -q "## \[$VERSION\]" CHANGELOG.md; then
    echo "Adding changelog entry for $VERSION..."
    DATE=$(date +%Y-%m-%d)
    sed -i.bak "1i\\
## [$VERSION] - $DATE\\
\\
### Added\\
- New features here\\
\\
### Changed\\
- Changes here\\
\\
### Fixed\\
- Fixes here\\
\\
" CHANGELOG.md
    rm CHANGELOG.md.bak
    echo "⚠️  Please edit CHANGELOG.md to add release notes"
  fi
fi

# 4. Запуск тестов
echo "Running tests..."
cargo test

# 5. Локальная сборка релиза (проверка работоспособности)
echo "Building release (sanity check)..."
cargo build --release

echo ""
echo "✅ Release preparation complete!"
echo ""
echo "Next steps:"
echo "  1. Review and edit CHANGELOG.md"
echo "  2. Commit changes: git add -A && git commit -m 'chore: prepare release $VERSION'"
echo "  3. Create tag: git tag -a $VERSION -m 'Release $VERSION'"
echo "  4. Push: git push origin main && git push origin $VERSION"
echo ""
echo "Release workflow will run automatically on tag push."
