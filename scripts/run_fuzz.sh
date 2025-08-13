#!/bin/bash

# Скрипт для запуска fuzzing тустов
# Цель: 0 паник за 24 часа

set -e

# Цвета для вывода
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

FUZZ_TIME_MINUTES=${1:-60}  # По умолчанию 1 час, для полного теста 1440 минут (24 часа)
CORPUS_DIR="fuzz/corpus"
CRASHES_DIR="fuzz/artifacts"

echo -e "${GREEN}Starting fuzzing for ${FUZZ_TIME_MINUTES} minutes...${NC}"

# Проверяем, что cargo-fuzz установлен.
if ! command -v cargo-fuzz &> /dev/null; then
    echo -e "${RED}cargo-fuzz not found. Installing...${NC}"
    cargo install cargo-fuzz
fi

# Инициализируем физзинг если нужно
if [ ! -d "fuzz" ]; then
	echo -e "${YELLOW}Initializing fuzz directory...${NC}"
	cargo fuzz init
fi

# Создаём директорию для результатов
mkdir -p results/$(date +%Y%m%d_%H%M%S)
RESULTS_DIR="results/$(date +%Y%m%d_%H%M%S)"

echo -e "${YELLOW}Available fuzz targets:${NC}"
cargo fuzz list

# Ф-я для запуска фаззинга одного таргета
run_target() {}
