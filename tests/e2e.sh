#!/usr/bin/env bash
# E2E test script for ChatAPI gateway
# Starts the server, runs curl-based tests, validates responses.
set -euo pipefail

PORT=${CHATAPI_PORT:-8090}
BASE="http://127.0.0.1:${PORT}"
PASS=0
FAIL=0
SERVER_PID=""

cleanup() {
    if [[ -n "$SERVER_PID" ]]; then
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
        echo "Server stopped (PID $SERVER_PID)"
    fi
}
trap cleanup EXIT

check() {
    local name="$1"
    local condition="$2"
    if eval "$condition"; then
        echo "  PASS: $name"
        PASS=$((PASS + 1))
    else
        echo "  FAIL: $name"
        FAIL=$((FAIL + 1))
    fi
}

echo "=== ChatAPI E2E Test Suite ==="
echo ""

# ── Build and start server ──────────────────────────────────────────
echo "Building gateway..."
cargo build -p chatapi-gateway --release 2>&1 | tail -2
echo "Starting server on port $PORT..."
./target/release/gateway &
SERVER_PID=$!
sleep 1

# Verify server is up
if ! curl -sf "${BASE}/health" > /dev/null 2>&1; then
    echo "ERROR: Server failed to start"
    exit 1
fi
echo "Server running (PID $SERVER_PID)"
echo ""

# ── Health endpoint ─────────────────────────────────────────────────
echo "--- Health Endpoint ---"
HEALTH=$(curl -sf "${BASE}/health")
check "health status is ok" 'echo "$HEALTH" | jq -e ".status == \"ok\"" > /dev/null'
check "browser_connected is false" 'echo "$HEALTH" | jq -e ".browser_connected == false" > /dev/null'
check "active_sessions is 0" 'echo "$HEALTH" | jq -e ".active_sessions == 0" > /dev/null'
echo ""

# ── Non-streaming chat ──────────────────────────────────────────────
echo "--- Non-Streaming Chat ---"
RESP=$(curl -sf -X POST "${BASE}/v1/chat/completions" \
    -H "Content-Type: application/json" \
    -d '{
        "model": "deepseek-chat",
        "messages": [{"role": "user", "content": "hello"}],
        "stream": false
    }')

check "response has chatcmpl ID" 'echo "$RESP" | jq -e ".id | startswith(\"chatcmpl-\")" > /dev/null'
check "object is chat.completion" 'echo "$RESP" | jq -e ".object == \"chat.completion\"" > /dev/null'
check "model is deepseek-chat" 'echo "$RESP" | jq -e ".model == \"deepseek-chat\"" > /dev/null'
check "role is assistant" 'echo "$RESP" | jq -e ".choices[0].message.role == \"assistant\"" > /dev/null'
check "content is not empty" 'echo "$RESP" | jq -e ".choices[0].message.content | length > 0" > /dev/null'
check "finish_reason is stop" 'echo "$RESP" | jq -e ".choices[0].finish_reason == \"stop\"" > /dev/null'
check "prompt_tokens > 0" 'echo "$RESP" | jq -e ".usage.prompt_tokens > 0" > /dev/null'
check "completion_tokens > 0" 'echo "$RESP" | jq -e ".usage.completion_tokens > 0" > /dev/null'
check "total_tokens = prompt + completion" \
    'echo "$RESP" | jq -e ".usage.total_tokens == .usage.prompt_tokens + .usage.completion_tokens" > /dev/null'
echo ""

# ── Streaming chat ──────────────────────────────────────────────────
echo "--- Streaming Chat ---"
STREAM=$(curl -sf -X POST "${BASE}/v1/chat/completions" \
    -H "Content-Type: application/json" \
    -d '{
        "model": "deepseek-chat",
        "messages": [{"role": "user", "content": "test"}],
        "stream": true
    }')

# Check content type
CONTENT_TYPE=$(curl -s -o /dev/null -w "%{content_type}" -X POST "${BASE}/v1/chat/completions" \
    -H "Content-Type: application/json" \
    -d '{
        "model": "deepseek-chat",
        "messages": [{"role": "user", "content": "test"}],
        "stream": true
    }')
check "content type is text/event-stream" 'echo "$CONTENT_TYPE" | grep -q "text/event-stream"'

# Check [DONE] marker
check "stream ends with [DONE]" 'echo "$STREAM" | grep -q "\[DONE\]"'

# Count data chunks
CHUNK_COUNT=$(echo "$STREAM" | grep -c "^data: " || true)
check "has multiple data chunks" "[[ $CHUNK_COUNT -gt 1 ]]"

# Validate each JSON chunk
JSON_VALID=true
while IFS= read -r line; do
    if [[ "$line" == data:* ]] && [[ "$line" != *"DONE"* ]]; then
        JSON_STR="${line#data: }"
        if ! echo "$JSON_STR" | jq -e ".object == \"chat.completion.chunk\"" > /dev/null 2>&1; then
            JSON_VALID=false
        fi
    fi
done <<< "$STREAM"
check "all chunks are valid chat.completion.chunk" "$JSON_VALID"

# Accumulate content
CONTENT=""
while IFS= read -r line; do
    if [[ "$line" == data:* ]] && [[ "$line" != *"DONE"* ]]; then
        JSON_STR="${line#data: }"
        TOKEN=$(echo "$JSON_STR" | jq -r '.choices[0].delta.content // empty' 2>/dev/null)
        CONTENT="${CONTENT}${TOKEN}"
    fi
done <<< "$STREAM"
check "accumulated content contains 'Test'" 'echo "$CONTENT" | grep -qi "test"'
echo ""

# ── Error handling ──────────────────────────────────────────────────
echo "--- Error Handling ---"
ERR_400=$(curl -s -o /dev/null -w "%{http_code}" -X POST "${BASE}/v1/chat/completions" \
    -H "Content-Type: application/json" \
    -d '{"model":"deepseek-chat","messages":[],"stream":false}')
check "empty messages returns 400" "[[ $ERR_400 == 400 ]]"

ERR_BODY=$(curl -s -X POST "${BASE}/v1/chat/completions" \
    -H "Content-Type: application/json" \
    -d '{"model":"deepseek-chat","messages":[],"stream":false}')
check "error has invalid_request_error type" 'echo "$ERR_BODY" | jq -e ".error.type == \"invalid_request_error\"" > /dev/null'
echo ""

# ── Summary ─────────────────────────────────────────────────────────
echo "=== Results ==="
echo "  Passed: $PASS"
echo "  Failed: $FAIL"
echo ""
if [[ $FAIL -eq 0 ]]; then
    echo "All tests passed!"
    exit 0
else
    echo "Some tests failed."
    exit 1
fi
