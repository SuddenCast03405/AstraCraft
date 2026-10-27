#!/usr/bin/env bash
set -uo pipefail

HOST="${ASTRACRAFT_TEST_HOST:-15.235.28.64}"
PORT="${ASTRACRAFT_TEST_PORT:-3020}"
TARGET="${HOST}:${PORT}"
API_URL="${ASTRACRAFT_STATUS_API:-https://api.imlazy.ink/mcapi/?name=Minecraft%20%E6%9C%8D%E5%8A%A1%E5%99%A8&host=15.235.28.64%3A3020&type=json&getmotd=%0a%0a&getbg=1.jpg}"

echo "== AstraCraft server connectivity test =="
echo "Target: ${TARGET}"
echo

echo "== External status API =="
if command -v curl >/dev/null 2>&1; then
  if ! curl --fail --silent --show-error --location --max-time 12 "${API_URL}"; then
    echo
    echo "WARNING: status API request failed; continuing with AstraCraft's own protocol ping."
  else
    echo
  fi
else
  echo "WARNING: curl not found; skipping external status API."
fi

echo
echo "== AstraCraft protocol ping =="
if cargo run --release -- ping "${TARGET}"; then
  echo
  echo "RESULT: AstraCraft successfully completed the Minecraft status ping."
else
  status=$?
  echo
  echo "RESULT: AstraCraft could not complete the status ping (exit code ${status})."
  echo "This can mean the port is closed/refusing connections, the server is offline, or the network path is blocked."
  exit "${status}"
fi
