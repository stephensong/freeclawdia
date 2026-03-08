#!/usr/bin/env bash
# Run a third FreeClawdia instance as Oli on port 4002.
#
# Usage: ./scripts/run-oli.sh
#
# Opens at http://localhost:4002 with token OliToken123

set -euo pipefail
cd "$(dirname "$0")/.."

set -a
source <(grep -v '^\s*#' .env.oli | grep -v '^\s*$')
set +a

echo "Starting FreeClawdia as Oli on port ${GATEWAY_PORT:-3002}..."
echo "  Database: freeclawdia_oli"
echo "  Email:    oli@local.dev"
echo "  URL:      http://localhost:${GATEWAY_PORT:-3002}"
echo "  Token:    ${GATEWAY_AUTH_TOKEN}"
echo ""

cargo run
