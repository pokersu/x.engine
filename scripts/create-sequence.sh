#!/usr/bin/env bash
set -euo pipefail
# ────────────────────────────────────────────────────────
# x.engine — 创建序列
# 编辑下方的 blocks JSON，然后直接运行即可。
# 用法: ./scripts/create-sequence.sh [api_url]
# ────────────────────────────────────────────────────────

API="${1:-http://127.0.0.1:8080}"

curl -s -S -X POST "${API}/api/v1/sequences" \
  -H 'Content-Type: application/json' \
  -d '{
    "namespace": "default",
    "name": "my-sequence",
    "version": 1,
    "blocks": [
      {
        "type": "step",
        "id": "hello",
        "handler": "noop",
        "params": {}
      }
    ]
  }'
