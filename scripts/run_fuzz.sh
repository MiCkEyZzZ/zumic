#!/usr/bin/env bash
set -euo pipefail

# Zumic Fuzz Runner
# Usage: ./scripts/run_fuzz.sh [target] [minutes] [keep_going]
# Example: ./scripts/run_fuzz.sh decode_value 10 1

TARGET=${1:-decode_value}
MINUTES=${2:-10}
KEEP_GOING=${3:-1}

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
RESULTS_DIR="${ROOT}/results/${TIMESTAMP}"
LOG_FILE="${RESULTS_DIR}/${TARGET}.log"
PID_FILE="${RESULTS_DIR}/${TARGET}.pid"

# Создаём каталоги
mkdir -p "${RESULTS_DIR}"
mkdir -p "${ROOT}/fuzz/artifacts/${TARGET}"
mkdir -p "${ROOT}/fuzz/corpus/${TARGET}"

echo "╔═══════════════════════════════════════════════════════════════╗"
echo "║               Zumic Fuzz Test Runner                          ║"
echo "╚═══════════════════════════════════════════════════════════════╝"
echo ""
echo "Target:       ${TARGET}"
echo "Duration:     ${MINUTES} minutes"
echo "Keep going:   ${KEEP_GOING}"
echo "Results:      ${RESULTS_DIR}"
echo "Log:          ${LOG_FILE}"
echo ""

# Проверяем, установлен ли Cargo-Fuzz
if ! command -v cargo-fuzz &> /dev/null; then
    echo "⚠️  cargo-fuzz not found. Installing..."
    cargo +nightly install cargo-fuzz
fi

# Конвертируем минуты в секунды
SECONDS=$((MINUTES * 60))

# Устанавливаем среду
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-1}

echo "Starting fuzzer..."
echo ""

# Запускаем fuzz в фоновом режиме
cd "${ROOT}/fuzz"
nohup cargo +nightly fuzz run "${TARGET}" -- \
    -max_total_time=${SECONDS} \
    -keep_going=${KEEP_GOING} \
    -print_final_stats=1 \
    > "${LOG_FILE}" 2>&1 &

FZ_PID=$!
echo ${FZ_PID} > "${PID_FILE}"
echo "Fuzzer PID: ${FZ_PID}"
echo ""

# Дождаемся завершения
if wait ${FZ_PID}; then
    echo "✅ Fuzzer completed successfully"
    EXIT_CODE=0
else
    EXIT_CODE=$?
    echo "⚠️  Fuzzer exited with code ${EXIT_CODE}"
fi

echo ""
echo "Gathering artifacts..."

# Копируем артефакты
if [ -d "${ROOT}/fuzz/artifacts/${TARGET}" ]; then
    ARTIFACT_COUNT=$(find "${ROOT}/fuzz/artifacts/${TARGET}" -type f | wc -l)
    if [ ${ARTIFACT_COUNT} -gt 0 ]; then
        cp -r "${ROOT}/fuzz/artifacts/${TARGET}" "${RESULTS_DIR}/artifacts" || true
        echo "Found ${ARTIFACT_COUNT} artifact(s)"
    else
        echo "No artifacts found"
    fi
fi

# Объединяем новые тестовые случаи в корпус
if [ -d "${ROOT}/fuzz/artifacts/${TARGET}/crashes" ]; then
    CRASH_COUNT=$(find "${ROOT}/fuzz/artifacts/${TARGET}/crashes" -type f | wc -l)
    if [ ${CRASH_COUNT} -gt 0 ]; then
        echo "⚠️  Found ${CRASH_COUNT} crash(es)!"
        mkdir -p "${ROOT}/fuzz/corpus/${TARGET}"
        find "${ROOT}/fuzz/artifacts/${TARGET}" -type f -exec cp -n {} "${ROOT}/fuzz/corpus/${TARGET}/" \; || true
    fi
fi

# Краткое содержание
echo ""
echo "╔═══════════════════════════════════════════════════════════════╗"
echo "║                     Fuzz Summary                              ║"
echo "╚═══════════════════════════════════════════════════════════════╝"
echo ""
echo "Duration:     ${MINUTES} minutes"
echo "Results:      ${RESULTS_DIR}"
echo "Log file:     ${LOG_FILE}"
echo ""

# Отображаем последние строки журнала
if [ -f "${LOG_FILE}" ]; then
    echo "Last 10 lines of log:"
    echo "─────────────────────────────────────────────────────────────"
    tail -n 10 "${LOG_FILE}"
    echo "─────────────────────────────────────────────────────────────"
fi

echo ""
echo "To view full log: cat ${LOG_FILE}"
echo "To view artifacts: ls -la ${RESULTS_DIR}/artifacts"
echo ""

exit ${EXIT_CODE}
