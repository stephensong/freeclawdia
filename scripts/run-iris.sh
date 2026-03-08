#!/usr/bin/env bash
# Run a third FreeClawdia instance as Iris on port 4002.
#
# Usage: ./scripts/run-iris.sh
#
# Opens at http://localhost:4002 with token IrisToken123

set -euo pipefail
cd "$(dirname "$0")/.."

set -a
source <(grep -v '^\s*#' .env.iris | grep -v '^\s*$')
set +a

echo "Starting FreeClawdia as Iris on port ${GATEWAY_PORT:-3002}..."
echo "  Database: freeclawdia_iris"
echo "  Email:    iris@local.dev"
echo "  URL:      http://localhost:${GATEWAY_PORT:-3002}"
echo "  Token:    ${GATEWAY_AUTH_TOKEN}"
echo ""

cargo run
