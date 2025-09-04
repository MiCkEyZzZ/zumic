#!/usr/bin/env bash
set -euo pipefail

TARGET=${1:-decode_value}
MINUTES=${2:-10}   # по умолчанию 10 минут
KEEP_GOING=${3:-1}  # передаётся в libFuzzer (1 = не останавливать на первом crash)

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
RESULTS_DIR="${ROOT}/results/${TIMESTAMP}"
LOG_FILE="${RESULTS_DIR}/${TARGET}.log"
PID_FILE="${RESULTS_DIR}/${TARGET}.pid"

mkdir -p "${RESULTS_DIR}"
mkdir -p "${ROOT}/fuzz/artifacts/${TARGET}"
mkdir -p "${ROOT}/fuzz/corpus/${TARGET}"

echo "Starting fuzz target: ${TARGET} for ${MINUTES} minutes"
echo "Results -> ${RESULTS_DIR}"
echo "Log -> ${LOG_FILE}"

export CARGO_BUILD_JOBS=1

# Преобразуем минуты в секунды
SECONDS=$((MINUTES * 60))

# Запускаем fuzz в фоне через nightly Rust без лишних RUSTFLAGS
nohup bash -lc "cargo +nightly fuzz run ${TARGET} -- -max_total_time=${SECONDS} -keep_going=${KEEP_GOING}" > "${LOG_FILE}" 2>&1 &
FZ_PID=$!
echo ${FZ_PID} > "${PID_FILE}"
echo "Fuzz pid: ${FZ_PID}"

# Ждём завершения процесса
wait ${FZ_PID} || true

echo "Fuzz finished (or was stopped). Gathering artifacts..."

# Копируем артефакты (crashes/minimized) в results
if [ -d "${ROOT}/fuzz/artifacts/${TARGET}" ]; then
    cp -r "${ROOT}/fuzz/artifacts/${TARGET}" "${RESULTS_DIR}/artifacts" || true
fi

# Слить найденные рабочие тесты в corpus (если есть новые)
if [ -d "${ROOT}/fuzz/artifacts/${TARGET}/crashes" ]; then
    mkdir -p "${ROOT}/fuzz/corpus/${TARGET}"
    find "${ROOT}/fuzz/artifacts/${TARGET}" -type f -exec cp -n {} "${ROOT}/fuzz/corpus/${TARGET}/" \; || true
fi

echo "Results saved to ${RESULTS_DIR}"
echo "You can inspect ${LOG_FILE} or ${RESULTS_DIR}/artifacts for crashes."

exit 0
