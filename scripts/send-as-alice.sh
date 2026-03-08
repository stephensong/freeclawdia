#!/usr/bin/env bash
# Send an email from alice@local.dev to gary@local.dev via JMAP.
#
# Usage:
#   ./scripts/send-as-alice.sh "Subject line" "Body text"
#   ./scripts/send-as-alice.sh                              # interactive prompts
#
# Requires: curl, Stalwart running on localhost:4010

set -euo pipefail

JMAP_URL="http://localhost:4010/jmap"
ALICE_USER="alice"
ALICE_PASS="password123"
ALICE_ACCOUNT="d"
ALICE_IDENTITY="b"

SUBJECT="${1:-}"
BODY="${2:-}"

if [ -z "$SUBJECT" ]; then
    read -rp "Subject: " SUBJECT
fi
if [ -z "$BODY" ]; then
    read -rp "Body: " BODY
fi

if [ -z "$SUBJECT" ] || [ -z "$BODY" ]; then
    echo "Error: subject and body are required" >&2
    exit 1
fi

# Escape JSON strings
json_escape() {
    python3 -c "import json,sys; print(json.dumps(sys.argv[1]))" "$1"
}

SUBJECT_JSON=$(json_escape "$SUBJECT")
BODY_JSON=$(json_escape "$BODY")

RESPONSE=$(curl -s -u "${ALICE_USER}:${ALICE_PASS}" "$JMAP_URL" \
    -H 'Content-Type: application/json' \
    -d "{
  \"using\": [
    \"urn:ietf:params:jmap:core\",
    \"urn:ietf:params:jmap:mail\",
    \"urn:ietf:params:jmap:submission\"
  ],
  \"methodCalls\": [
    [\"Email/set\", {
      \"accountId\": \"${ALICE_ACCOUNT}\",
      \"create\": {
        \"draft1\": {
          \"mailboxIds\": {\"e\": true},
          \"from\": [{\"email\": \"alice@local.dev\"}],
          \"to\": [{\"email\": \"gary@local.dev\"}],
          \"subject\": ${SUBJECT_JSON},
          \"textBody\": [{\"partId\": \"1\", \"type\": \"text/plain\"}],
          \"bodyValues\": {\"1\": {\"value\": ${BODY_JSON}}}
        }
      }
    }, \"e\"],
    [\"EmailSubmission/set\", {
      \"accountId\": \"${ALICE_ACCOUNT}\",
      \"create\": {
        \"sub1\": {
          \"emailId\": \"#draft1\",
          \"identityId\": \"${ALICE_IDENTITY}\"
        }
      }
    }, \"s\"]
  ]
}")

# Check for errors
if echo "$RESPONSE" | python3 -c "
import sys, json
d = json.load(sys.stdin)
for name, data, tag in d['methodResponses']:
    if name == 'error':
        print(f'Error: {data.get(\"description\", data)}', file=sys.stderr)
        sys.exit(1)
    if 'notCreated' in data:
        for k, v in data['notCreated'].items():
            print(f'Error creating {k}: {v}', file=sys.stderr)
            sys.exit(1)
print('OK')
" 2>&1 | grep -q "^OK$"; then
    echo "Sent from alice@local.dev to gary@local.dev: \"$SUBJECT\""
else
    echo "Failed to send email" >&2
    echo "$RESPONSE" | python3 -m json.tool 2>/dev/null || echo "$RESPONSE"
    exit 1
fi
