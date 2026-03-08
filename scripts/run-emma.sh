#!/usr/bin/env bash
# Run a second FreeClawdia instance as Emma on port 4001.
#
# Usage: ./scripts/run-emma.sh
#
# Opens at http://localhost:4001 with token EmmaToken123

set -euo pipefail
cd "$(dirname "$0")/.."

# Load .env.emma, skipping comments and blanks
set -a
source <(grep -v '^\s*#' .env.emma | grep -v '^\s*$')
set +a

# Override the default .env so dotenv doesn't load Gary's config
export DOTENV_PATH=.env.emma

echo "Starting FreeClawdia as Emma on port ${GATEWAY_PORT:-3002}..."
echo "  Database: freeclawdia_emma"
echo "  Email:    emma@local.dev"
echo "  URL:      http://localhost:${GATEWAY_PORT:-3002}"
echo "  Token:    ${GATEWAY_AUTH_TOKEN}"
echo ""

cargo run
