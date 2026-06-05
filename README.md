# x.engine — 高性能 API 编排引擎

基于 `orch8` 内核提取的高性能通用流程编排引擎，专为 **API 编排** 场景设计。
核心特性：Tick Loop 批处理调度、`SELECT ... FOR UPDATE SKIP LOCKED` 横向扩展、
HandlerRegistry 扩展点、丰富的组合 Block 类型。

## 快速开始

### 编译

```bash
# 进入项目目录
cd x.engine

# 编译全部 crate
cargo build --workspace

# 运行测试（1334 个）
cargo test --workspace --lib
```

### 启动服务

```bash
# 使用 SQLite 后端启动（零依赖）
cargo run -p x-server

# 服务监听 http://127.0.0.1:8080
# 数据文件存储在 ./x.engine.db
```

### 健康检查

```bash
# Liveness — 进程是否运行
curl http://127.0.0.1:8080/health/live

# Readiness — 数据库是否可达
curl http://127.0.0.1:8080/health/ready
```

## API 端点总览

| 方法 | 路径 | 说明 |
|---|---|---|
| `POST` | `/api/v1/sequences` | 创建序列 |
| `GET` | `/api/v1/sequences` | 列表序列 |
| `GET` | `/api/v1/sequences/{id}` | 获取序列详情 |
| `DELETE` | `/api/v1/sequences/{id}` | 删除序列 |
| `POST` | `/api/v1/sequences/{id}/deprecate` | 弃用序列版本 |
| `POST` | `/api/v1/sequences/{id}/status` | 更新序列状态 |
| `GET` | `/api/v1/sequences/by-name` | 按名称查询序列 |
| `GET` | `/api/v1/sequences/versions` | 列出版本 |
| `POST` | `/api/v1/instances` | 创建实例 |
| `POST` | `/api/v1/instances/batch` | 批量创建实例 |
| `GET` | `/api/v1/instances` | 列表实例 |
| `GET` | `/api/v1/instances/{id}` | 获取实例详情 |
| `PATCH` | `/api/v1/instances/{id}/state` | 更新实例状态 |
| `PATCH` | `/api/v1/instances/{id}/context` | 更新实例上下文 |
| `POST` | `/api/v1/instances/{id}/retry` | 重试实例 |
| `POST` | `/api/v1/instances/{id}/signals` | 发送信号 |
| `GET` | `/api/v1/instances/{id}/outputs` | 获取步骤输出 |
| `GET` | `/api/v1/instances/{id}/tree` | 获取执行树 |
| `GET` | `/health/live` | Liveness 探针 |
| `GET` | `/health/ready` | Readiness 探针 |

> 所有路径也同时挂载在根路径（例如 `/sequences` 等效于 `/api/v1/sequences`），
> 但推荐使用 `/api/v1/` 前缀。

## 核心概念

### 序列 (Sequence)

序列是一个工作流定义，由一系列 **块 (Block)** 组成。

### 块类型 (Block)

| 块类型 | JSON 标签 | 说明 |
|---|---|---|
| **Step** | `"step"` | 单个步骤，调用 handler |
| **Parallel** | `"parallel"` | 并行执行所有分支 |
| **Race** | `"race"` | 竞态执行，先完成者胜 |
| **Loop** | `"loop"` | 循环执行 |
| **ForEach** | `"for_each"` | 遍历集合并为每个元素执行 |
| **Router** | `"router"` | 条件路由到不同分支 |
| **TryCatch** | `"try_catch"` | 异常处理 |
| **SubSequence** | `"sub_sequence"` | 子流程调用 |
| **CancellationScope** | `"cancellation_scope"` | 不可取消的作用域 |

### 实例 (Instance)

实例是序列的**一次运行**。每个实例有状态机：

```
Scheduled → Running → Completed
                  ↘ Failed
                  ↗ Cancelled
                → Paused → Scheduled
                → Waiting → Scheduled
```

### Handler

Handler 是可注册的步骤处理器。内置 handler 包括：

- **`http_request`** — HTTP 请求
- **`sleep`** — 等待指定时间
- **`noop`** — 空操作
- **`log`** — 日志输出
- **`fail`** — 强制失败
- **`set_state`** / **`get_state`** / **`delete_state`** — KV 状态管理
- **`merge_state`** — 合并状态
- **`transform`** — JSON 转换
- **`assert`** — 断言
- **`emit_event`** — 发射事件
- **`send_signal`** — 发送信号给另一个实例
- 以及更多...

你也可以注册自定义 handler。

## 完整示例

以下 3 个综合示例覆盖 x.engine **全部 10 种 Block 类型**和 **25 个内置 Handler**。

---

### 示例 1：API 编排综合流程

这个序列展示**经典工作流编排**的核心能力：HTTP 调用、并行/竞速、条件路由、循环、异常处理、KV 状态管理、数据转换、以及各种高级 step 配置。

```bash
curl -X POST http://127.0.0.1:8080/api/v1/sequences \
  -H 'Content-Type: application/json' \
  -d '{
    "namespace": "default",
    "name": "api-orchestration-comprehensive",
    "version": 1,
    "blocks": [

      │  1. 数据获取 — 带重试、超时、延迟  │
      {
        "type": "step",
        "id": "fetch_data",
        "handler": "http_request",
        "params": {
          "method": "GET",
          "url": "https://api.example.com/orders?date={{context.data.business_date}}",
          "headers": {"Accept": "application/json"},
          "timeout_ms": 10000,
          "max_response_bytes": 1048576
        },
        "delay": { "duration": 200, "jitter": 100 },
        "timeout": 15000,
        "retry": {
          "max_attempts": 3,
          "initial_backoff": 500,
          "max_backoff": 10000,
          "backoff_multiplier": 2.0
        },
        "send_window": {
          "start_hour": 6,
          "end_hour": 23
        },
        "cache_key": "orders_{{context.data.business_date}}"
      },

      │  2. 数据校验  │
      {
        "type": "step",
        "id": "validate_data",
        "handler": "assert",
        "params": {
          "condition": "len(outputs.fetch_data.body.items) > 0",
          "message": "当天没有订单数据"
        }
      },

      │  3. 路由分发 — 根据订单量走不同分支  │
      {
        "type": "router",
        "id": "route_by_volume",
        "routes": [
          {
            "condition": "len(outputs.fetch_data.body.items) > 1000",
            "blocks": [
              {
                "type": "step",
                "id": "log_high_volume",
                "handler": "log",
                "params": {
                  "message": "高流量告警：{{ len(outputs.fetch_data.body.items) }} 笔订单",
                  "level": "warn"
                }
              }
            ]
          },
          {
            "condition": "len(outputs.fetch_data.body.items) == 0",
            "blocks": [
              {
                "type": "step",
                "id": "log_empty",
                "handler": "log",
                "params": { "message": "无订单，跳过处理", "level": "info" }
              }
            ]
          }
        ],
        "default": [
          {
            "type": "step",
            "id": "log_normal",
            "handler": "log",
            "params": { "message": "正常量处理", "level": "info" }
          }
        ]
      },

      │  4. 并行调用下游服务 + 竞速  │
      {
        "type": "race",
        "id": "price_race",
        "semantics": "first_to_succeed",
        "branches": [
          [
            {
              "type": "step",
              "id": "price_provider_a",
              "handler": "http_request",
              "params": {
                "method": "POST",
                "url": "https://pricing-a.example.com/calculate",
                "body": { "order_ids": "{{outputs.fetch_data.body.items[*].id}}" }
              },
              "timeout": 3000
            }
          ],
          [
            {
              "type": "step",
              "id": "price_provider_b",
              "handler": "http_request",
              "params": {
                "method": "POST",
                "url": "https://pricing-b.example.com/calculate",
                "body": { "items": "{{outputs.fetch_data.body.items}}" }
              },
              "timeout": 5000
            }
          ]
        ]
      },

      │  5. 异常处理 — 计价失败走降级  │
      {
        "type": "try_catch",
        "id": "pricing_with_fallback",
        "try_block": [
          {
            "type": "step",
            "id": "apply_pricing",
            "handler": "http_request",
            "params": {
              "method": "POST",
              "url": "https://billing.example.com/apply",
              "body": { "prices": "{{outputs.price_race.result}}" }
            }
          }
        ],
        "catch_block": [
          {
            "type": "step",
            "id": "manual_fallback",
            "handler": "http_request",
            "params": {
              "method": "POST",
              "url": "https://billing.example.com/apply-fallback",
              "body": { "price": 0, "reason": "计价服务不可用" }
            }
          }
        ],
        "finally_block": [
          {
            "type": "step",
            "id": "audit_pricing",
            "handler": "noop",
            "params": {}
          }
        ]
      },

      │  6. 遍历处理每个订单  │
      {
        "type": "for_each",
        "id": "process_orders",
        "collection": "outputs.fetch_data.body.items",
        "item_var": "order",
        "body": [
          {
            "type": "step",
            "id": "fulfill_order",
            "handler": "http_request",
            "params": {
              "method": "POST",
              "url": "https://fulfillment.example.com/fulfill",
              "body": {
                "order_id": "{{data.order.id}}",
                "price": "{{outputs.price_race.result.total}}"
              }
            },
            "retry": {
              "max_attempts": 2,
              "initial_backoff": 1000,
              "max_backoff": 5000,
              "backoff_multiplier": 2.0
            },
            "cancellable": false,
            "fallback_handler": "noop"
          }
        ],
        "max_iterations": 500
      },

      │  7. 轮询等待外部系统完成  │
      {
        "type": "loop",
        "id": "poll_fulfillment",
        "condition": "outputs.check_status.body.state !== 'completed'",
        "body": [
          {
            "type": "step",
            "id": "check_status",
            "handler": "http_request",
            "params": {
              "method": "GET",
              "url": "https://fulfillment.example.com/batch/{{context.data.batch_id}}/status"
            }
          },
          {
            "type": "step",
            "id": "wait_before_retry",
            "handler": "sleep",
            "params": { "duration_ms": 2000 }
          }
        ],
        "max_iterations": 30,
        "break_on": "outputs.check_status.body.state === 'failed'",
        "continue_on_error": false,
        "poll_interval": 2000
      },

      │  8. KV 状态管理  │
      {
        "type": "parallel",
        "id": "state_ops",
        "branches": [
          [
            {
              "type": "step",
              "id": "save_summary",
              "handler": "set_state",
              "params": {
                "key": "batch_summary",
                "value": {
                  "batch_id": "{{context.data.batch_id}}",
                  "total_orders": "{{ len(outputs.fetch_data.body.items) }}",
                  "fulfilled": "{{ len(outputs.process_orders) }}",
                  "status": "{{outputs.check_status.body.state}}"
                }
              }
            }
          ],
          [
            {
              "type": "step",
              "id": "save_metrics",
              "handler": "merge_state",
              "params": {
                "values": {
                  "metrics": {
                    "pricing_time_ms": "{{ outputs.apply_pricing.runtime.duration_ms }}",
                    "total_items": "{{ len(outputs.fetch_data.body.items) }}"
                  }
                }
              }
            }
          ],
          [
            {
              "type": "step",
              "id": "read_config",
              "handler": "get_state",
              "params": { "key": "batch_summary" }
            }
          ]
        ]
      },

      │  9. 查询另一个实例（跨实例通信）  │
      {
        "type": "step",
        "id": "check_upstream",
        "handler": "query_instance",
        "params": {
          "instance_id": "{{context.data.upstream_instance_id}}"
        }
      },

      │  10. 数据转换 + 最终输出  │
      {
        "type": "step",
        "id": "generate_report",
        "handler": "transform",
        "params": {
          "batch_id": "{{context.data.batch_id}}",
          "status": "{{outputs.fulfill_order | default('skipped')}}",
          "total_orders": "{{ len(outputs.fetch_data.body.items) }}",
          "price_provider": "{{outputs.price_race._winner}}"
        }
      },

      {
        "type": "step",
        "id": "cleanup_temp",
        "handler": "delete_state",
        "params": { "key": "temp_cache" }
      },

      {
        "type": "step",
        "id": "final_log",
        "handler": "log",
        "params": {
          "message": "批次 {{context.data.batch_id}} 处理完成",
          "level": "info"
        }
      }
    ]
  }'
```

---

### 示例 2：AI Agent + 人工审批流程

这个序列展示 **AI 原生能力**：LLM 多供应商故障转移、带工具的 Agent 循环、MCP 协议调用、向量记忆、人工审核（Human-in-the-Loop）、Agent 自修改、以及事件触发。

```bash
curl -X POST http://127.0.0.1:8080/api/v1/sequences \
  -H 'Content-Type: application/json' \
  -d '{
    "namespace": "default",
    "name": "ai-customer-support",
    "version": 1,
    "blocks": [

      │  1. 语义搜索历史记忆  │
      {
        "type": "step",
        "id": "search_memory",
        "handler": "memory_search",
        "params": {
          "query": "{{context.data.customer_query}}",
          "top_k": 3,
          "model": "text-embedding-3-small"
        }
      },

      │  2. LLM 调用 — 多供应商故障转移  │
      {
        "type": "step",
        "id": "classify_intent",
        "handler": "llm_call",
        "params": {
          "system": "你是一个客服意图分类器。返回 JSON：{ \"category\": \"refund|complaint|inquiry|technical\", \"priority\": \"high|medium|low\", \"summary\": \"一句话描述\" }。只返回 JSON。",
          "messages": [
            { "role": "user", "content": "{{context.data.customer_query}} （相关历史：{{outputs.search_memory | json}}）" }
          ],
          "providers": [
            { "provider": "openai", "model": "gpt-4o", "api_key_env": "OPENAI_KEY" },
            { "provider": "anthropic", "model": "claude-sonnet-4-20250514", "api_key_env": "ANTHROPIC_KEY" },
            { "provider": "deepseek", "model": "deepseek-chat", "api_key_env": "DEEPSEEK_KEY" }
          ],
          "temperature": 0.1,
          "max_tokens": 512,
          "response_format": { "type": "json_object" },
          "total_timeout_secs": 30
        }
      },

      │  3. 条件路由 — 根据意图走不同分支  │
      {
        "type": "router",
        "id": "route_by_intent",
        "routes": [
          {
            "condition": "outputs.classify_intent.category === 'refund'",
            "blocks": [
              {
                "type": "step",
                "id": "lookup_order",
                "handler": "tool_call",
                "params": {
                  "url": "https://internal-api/orders/lookup",
                  "tool_name": "lookup_order",
                  "arguments": { "order_id": "{{context.data.order_id}}" },
                  "timeout_ms": 5000
                }
              }
            ]
          },
          {
            "condition": "outputs.classify_intent.category === 'technical'",
            "blocks": [
              {
                "type": "step",
                "id": "mcp_diagnose",
                "handler": "mcp_call",
                "params": {
                  "url": "https://mcp.internal.example.com",
                  "action": "call",
                  "tool_name": "diagnose_system",
                  "arguments": { "user_id": "{{context.data.user_id}}" }
                }
              }
            ]
          }
        ],
        "default": [
          {
            "type": "step",
            "id": "default_handling",
            "handler": "noop",
            "params": {}
          }
        ]
      },

      │  4. ReAct Agent 循环 — 带 MCP 工具发现  │
      {
        "type": "step",
        "id": "resolve_issue",
        "handler": "agent",
        "params": {
          "goal": "解决客户问题：{{context.data.customer_query}}。订单信息：{{outputs.lookup_order | default('无')}}。诊断结果：{{outputs.mcp_diagnose | default('无')}}。",
          "system": "你是一个专业客服 Agent。你有以下工具可用。一步步思考，每次返回工具调用或最终答案。",
          "max_iterations": 10,
          "tool_dispatch": {
            "type": "mcp",
            "url": "https://mcp.internal.example.com"
          },
          "auto_memory": {
            "recall_k": 5,
            "store_outcome": true
          },
          "provider": "openai",
          "model": "gpt-4o",
          "temperature": 0.3
        }
      },

      │  5. 人工审核 — Human-in-the-Loop  │
      {
        "type": "step",
        "id": "human_approval",
        "handler": "human_review",
        "params": {
          "review_data": {
            "customer": "{{context.data.user_id}}",
            "query": "{{context.data.customer_query}}",
            "intent": "{{outputs.classify_intent.category}}",
            "priority": "{{outputs.classify_intent.priority}}",
            "agent_solution": "{{outputs.resolve_issue}}"
          },
          "instructions": "请审核 Agent 的解决方案，确认是否可以直接执行",
          "reviewer": "supervisor",
          "notify_url": "https://slack-webhook.example.com/approvals"
        },
        "wait_for_input": {
          "prompt": "客服 Agent 建议如下方案，请审核：\n{{outputs.resolve_issue}}",
          "timeout": 86400000,
          "choices": [
            { "label": "✅ 批准执行", "value": "approve" },
            { "label": "❌ 拒绝", "value": "reject" },
            { "label": "⏫ 升级主管", "value": "escalate" }
          ],
          "store_as": "approval",
          "allow_comment": true,
          "escalation_handler": "notify_manager"
        }
      },

      │  6. Agent 自修改 — 运行时注入新步骤  │
      {
        "type": "step",
        "id": "inject_followup",
        "handler": "self_modify",
        "params": {
          "blocks": [
            {
              "type": "step",
              "id": "send_followup_email",
              "handler": "http_request",
              "params": {
                "method": "POST",
                "url": "https://notifications.example.com/email",
                "body": {
                  "to": "{{context.data.user_id}}",
                  "template": "support-followup",
                  "data": {
                    "resolution": "{{outputs.resolve_issue}}",
                    "approved_by": "{{context.data.approval.value}}"
                  }
                }
              }
            }
          ],
          "position": "end"
        }
      },

      │  7. 存储本次交互到记忆  │
      {
        "type": "step",
        "id": "store_interaction",
        "handler": "memory_store",
        "params": {
          "text": "客户 {{context.data.user_id}} 的问题：{{context.data.customer_query}}。解决方案：{{outputs.resolve_issue}}。审批结果：{{context.data.approval.value}}。",
          "metadata": {
            "user_id": "{{context.data.user_id}}",
            "intent": "{{outputs.classify_intent.category}}",
            "priority": "{{outputs.classify_intent.priority}}",
            "timestamp": "{{ now() }}"
          },
          "model": "text-embedding-3-small"
        }
      },

      │  8. 生成用量事件、触发下游流程  │
      {
        "type": "step",
        "id": "emit_ticket",
        "handler": "emit_event",
        "params": {
          "trigger_slug": "support-ticket-resolved",
          "data": {
            "user_id": "{{context.data.user_id}}",
            "ticket_id": "{{context.data.ticket_id}}",
            "resolution": "{{outputs.resolve_issue}}",
            "approved": "{{context.data.approval.value === 'approve'}}"
          },
          "dedupe_key": "ticket_{{context.data.ticket_id}}",
          "dedupe_scope": "parent"
        }
      },

      │  9. 查询关联实例进度  │
      {
        "type": "step",
        "id": "track_downstream",
        "handler": "query_instance",
        "params": {
          "instance_id": "{{outputs.emit_ticket.instance_id}}"
        }
      },

      │  10. 向量化最终报告  │
      {
        "type": "step",
        "id": "embed_report",
        "handler": "embed",
        "params": {
          "input": "客服处理报告：{{outputs.resolve_issue}}",
          "model": "text-embedding-3-small"
        }
      }
    ]
  }'
```

---

### 示例 3：高级编排模式

这个序列展示 **高级工作流模式**：A/B 分流测试、不可取消作用域、子流程调用、竞速优先成功策略、复杂循环控制、以及 SLA 超时等 step 级高级配置。

```bash
curl -X POST http://127.0.0.1:8080/api/v1/sequences \
  -H 'Content-Type: application/json' \
  -d '{
    "namespace": "default",
    "name": "advanced-patterns",
    "version": 1,
    "blocks": [

      │  1. A/B 分流测试  │
      {
        "type": "ab_split",
        "id": "recommendation_abtest",
        "variants": [
          {
            "name": "control",
            "weight": 50,
            "blocks": [
              {
                "type": "step",
                "id": "legacy_recommend",
                "handler": "http_request",
                "params": {
                  "method": "POST",
                  "url": "https://recommend-legacy.example.com/predict",
                  "body": { "user_id": "{{context.data.user_id}}" }
                },
                "timeout": 2000
              }
            ]
          },
          {
            "name": "variant_a",
            "weight": 50,
            "blocks": [
              {
                "type": "step",
                "id": "ml_recommend",
                "handler": "http_request",
                "params": {
                  "method": "POST",
                  "url": "https://recommend-ml.example.com/v2/predict",
                  "body": { "user_id": "{{context.data.user_id}}" }
                },
                "timeout": 5000,
                "rate_limit_key": "ml_inference"
              }
            ]
          }
        ]
      },

      │  2. 不可取消作用域（即使实例收到 Cancel 信号也执行）  │
      {
        "type": "cancellation_scope",
        "id": "charge_and_notify",
        "blocks": [
          {
            "type": "step",
            "id": "charge_payment",
            "handler": "http_request",
            "params": {
              "method": "POST",
              "url": "https://payment.example.com/charge",
              "body": {
                "user_id": "{{context.data.user_id}}",
                "amount": "{{context.data.amount}}",
                "idempotency_key": "{{context.data.tx_id}}"
              }
            },
            "timeout": 10000,
            "retry": {
              "max_attempts": 3,
              "initial_backoff": 1000,
              "max_backoff": 5000,
              "backoff_multiplier": 2.0
            },
            "cancellable": false
          },
          {
            "type": "step",
            "id": "send_receipt",
            "handler": "http_request",
            "params": {
              "method": "POST",
              "url": "https://notifications.example.com/receipt",
              "body": { "user_id": "{{context.data.user_id}}", "amount": "{{context.data.amount}}" }
            },
            "cancellable": false
          }
        ]
      },

      │  3. 调用子流程（SubSequence）  │
      {
        "type": "sub_sequence",
        "id": "run_compliance_check",
        "sequence_name": "kyc-verification",
        "version": 2,
        "input": {
          "user_id": "{{context.data.user_id}}",
          "amount": "{{context.data.amount}}",
          "source": "advanced-patterns-parent"
        }
      },

      │  4. 竞速：多个提供商，FirstToSucceed  │
      {
        "type": "race",
        "id": "fraud_check_race",
        "semantics": "first_to_succeed",
        "branches": [
          [
            {
              "type": "step",
              "id": "fraud_provider_1",
              "handler": "http_request",
              "params": {
                "method": "POST",
                "url": "https://fraud-v1.example.com/check",
                "body": { "transaction": "{{context.data.tx_id}}" }
              },
              "timeout": 2000
            }
          ],
          [
            {
              "type": "step",
              "id": "fraud_provider_2",
              "handler": "http_request",
              "params": {
                "method": "POST",
                "url": "https://fraud-v2.example.com/check",
                "body": { "tx": "{{context.data.tx_id}}" }
              },
              "timeout": 2000
            }
          ],
          [
            {
              "type": "step",
              "id": "fraud_provider_3",
              "handler": "http_request",
              "params": {
                "method": "POST",
                "url": "https://fraud-ml.example.com/analyze",
                "body": { "transaction_id": "{{context.data.tx_id}}" }
              },
              "timeout": 5000
            }
          ]
        ]
      },

      │  5. 复杂路由 + 表达式 & 三元运算  │
      {
        "type": "router",
        "id": "complex_routing",
        "routes": [
          {
            "condition": "outputs.fraud_check_race.result.risk_score > 0.8 && outputs.run_compliance_check.verified === false",
            "blocks": [
              {
                "type": "step",
                "id": "auto_decline",
                "handler": "http_request",
                "params": {
                  "method": "POST",
                  "url": "https://decision.example.com/decline",
                  "body": { "tx_id": "{{context.data.tx_id}}", "reason": "高风险 + KYC 未通过" }
                }
              }
            ]
          },
          {
            "condition": "outputs.fraud_check_race.result.risk_score >= 0.5 && outputs.fraud_check_race.result.risk_score <= 0.8",
            "blocks": [
              {
                "type": "step",
                "id": "manual_review_needed",
                "handler": "log",
                "params": { "message": "交易 {{context.data.tx_id}} 需要人工审核", "level": "warn" }
              }
            ]
          }
        ],
        "default": [
          {
            "type": "step",
            "id": "auto_approve",
            "handler": "log",
            "params": { "message": "交易 {{context.data.tx_id}} 自动通过", "level": "info" }
          }
        ]
      },

      │  6. 复杂 Loop — 带 break_on + continue_on_error  │
      {
        "type": "loop",
        "id": "retry_queue",
        "condition": "outputs.process_item.result.status !== 'success'",
        "body": [
          {
            "type": "step",
            "id": "process_item",
            "handler": "http_request",
            "params": {
              "method": "POST",
              "url": "https://queue.example.com/process/{{context.data.item_id}}",
              "body": { "attempt": "{{ outputs.process_item | default(0) | length + 1 }}" }
            }
          }
        ],
        "max_iterations": 5,
        "break_on": "outputs.process_item.result.status === 'terminal_failure'",
        "continue_on_error": true,
        "poll_interval": 1000
      },

      │  7. ForEach + 嵌套 Step 配置  │
      {
        "type": "for_each",
        "id": "batch_notify",
        "collection": "context.data.recipients",
        "item_var": "recipient",
        "body": [
          {
            "type": "step",
            "id": "send_notification",
            "handler": "http_request",
            "params": {
              "method": "POST",
              "url": "https://notify.example.com/send",
              "body": {
                "to": "{{data.recipient.email}}",
                "template": "{{data.recipient.template}}",
                "ctx": {
                  "name": "{{data.recipient.name}}",
                  "tx_id": "{{context.data.tx_id}}",
                  "amount": "{{context.data.amount}}"
                }
              }
            },
            "timeout": 5000,
            "context_access": {
              "data": { "fields": ["recipient"] },
              "config": false
            },
            "deadline": 30000,
            "on_deadline_breach": {
              "handler": "log",
              "params": { "message": "通知 {{data.recipient.email}} 发送超时", "level": "error" }
            }
          }
        ],
        "max_iterations": 100
      },

      │  8. 并行执行 + 异常隔离  │
      {
        "type": "parallel",
        "id": "post_processing",
        "branches": [
          [
            {
              "type": "try_catch",
              "id": "try_export",
              "try_block": [
                {
                  "type": "step",
                  "id": "export_to_s3",
                  "handler": "http_request",
                  "params": {
                    "method": "PUT",
                    "url": "https://storage.example.com/exports/{{context.data.tx_id}}.json",
                    "body": "{{outputs}}",
                    "headers": { "Content-Type": "application/json" }
                  }
                }
              ],
              "catch_block": [
                {
                  "type": "step",
                  "id": "export_fallback",
                  "handler": "http_request",
                  "params": {
                    "method": "POST",
                    "url": "https://storage.example.com/exports-fallback",
                    "body": { "tx_id": "{{context.data.tx_id}}", "data": "{{outputs}}" }
                  }
                }
              ]
            }
          ],
          [
            {
              "type": "step",
              "id": "push_analytics",
              "handler": "http_request",
              "params": {
                "method": "POST",
                "url": "https://analytics.example.com/events",
                "body": {
                  "event": "transaction_completed",
                  "tx_id": "{{context.data.tx_id}}",
                  "amount": "{{context.data.amount}}",
                  "variant": "{{outputs.recommendation_abtest._variant}}"
                }
              }
            }
          ],
          [
            {
              "type": "step",
              "id": "blob_backup",
              "handler": "blob_put",
              "params": {
                "text": "{{outputs | json}}",
                "content_type": "application/json",
                "max_size_bytes": 10485760
              }
            }
          ]
        ]
      },

      │  9. 读取 blob 制品  │
      {
        "type": "step",
        "id": "verify_backup",
        "handler": "blob_get",
        "params": {
          "ref": "{{outputs.blob_backup.ref}}",
          "encoding": "utf8",
          "max_size_bytes": 10485760
        }
      },

      │  10. 跨实例信号发送  │
      {
        "type": "step",
        "id": "notify_orchestrator",
        "handler": "send_signal",
        "params": {
          "instance_id": "{{context.data.orchestrator_instance_id}}",
          "signal_type": "Custom",
          "payload": {
            "type": "child_completed",
            "child_instance_id": "{{context.runtime.instance_id}}",
            "result": "completed"
          }
        }
      }
    ]
  }'
```

---

### 各示例覆盖的功能节点总表

| 功能 | 示例 1 | 示例 2 | 示例 3 |
|---|---|---|---|
| **Block 类型** | | | |
| Step | ✅ | ✅ | ✅ |
| Parallel | ✅ | — | ✅ |
| Race | ✅ (FirstToSucceed) | — | ✅ (FirstToSucceed) |
| Loop | ✅ (poll + break_on) | — | ✅ (continue_on_error) |
| ForEach | ✅ | — | ✅ |
| Router | ✅ | ✅ | ✅ (三元运算) |
| TryCatch | ✅ (带 finally) | — | ✅ (嵌套在 Parallel 内) |
| SubSequence | — | — | ✅ |
| ABSplit | — | — | ✅ |
| CancellationScope | — | — | ✅ |
| **Handler** | | | |
| http_request | ✅ | ✅ | ✅ |
| http_request 响应流 | — | — | ✅ |
| noop | ✅ | ✅ | — |
| log | ✅ | — | ✅ |
| sleep | ✅ | — | — |
| fail | — | — | — |
| transform | ✅ | — | — |
| assert | ✅ | — | — |
| set_state / get_state / delete_state / merge_state | ✅ | — | — |
| llm_call (故障转移) | — | ✅ | — |
| agent (ReAct + MCP 工具) | — | ✅ | — |
| tool_call | — | ✅ | — |
| mcp_call | — | ✅ | — |
| embed | — | ✅ | — |
| memory_store / memory_search | — | ✅ | — |
| human_review (wait_for_input) | — | ✅ | — |
| self_modify | — | ✅ | — |
| emit_event (去重) | — | ✅ | — |
| send_signal | — | — | ✅ |
| query_instance | ✅ | ✅ | — |
| blob_put / blob_get | — | — | ✅ |
| **高级 Step 配置** | | | |
| retry (指数退避) | ✅ | — | ✅ |
| delay (jitter) | ✅ | — | — |
| timeout | ✅ | — | ✅ |
| send_window | ✅ | — | — |
| cache_key | ✅ | — | — |
| rate_limit_key | — | — | ✅ |
| context_access | — | — | ✅ |
| deadline + escalation | — | — | ✅ |
| fallback_handler | ✅ | — | — |
| cancellable: false | ✅ | — | ✅ |
| **引擎特性** | | | |
| 表达式 `{{ }}` | ✅ | ✅ | ✅ |
| 模板管道 `\| default()` | ✅ | ✅ | — |
| 函数 `now()` `len()` `uuid()` | ✅ | ✅ | ✅ |

## 横向扩展

x.engine 支持多节点水平扩展。多个引擎实例共享同一 Postgres 数据库，
通过 `SELECT ... FOR UPDATE SKIP LOCKED` 安全地竞争领取到期实例：

```bash
# 节点 1（默认端口）
cargo run -p x-server

# 节点 2（指定不同端口）
ORCH8_API__HTTP_ADDR=127.0.0.1:8081 cargo run -p x-server
```

节点自动注册到 `cluster_nodes` 表，每 10 秒心跳上报。
失败的节点会在 120 秒后被自动清理，其上的实例会被其他节点重新调度。

## 配置

服务端目前使用默认配置启动。可通过环境变量或 TOML 文件自定义：

```bash
# 调整 Tick 间隔（毫秒，默认 100）
export X_ENGINE_TICK_INTERVAL_MS=50

# 调整批处理大小（默认 256）
export X_ENGINE_BATCH_SIZE=512

# 调整最大并发步骤数（默认 128）
export X_ENGINE_MAX_CONCURRENT_STEPS=256
```

## 项目结构

```
x.engine/
├── Cargo.toml          # workspace 定义
├── migrations/         # 数据库迁移脚本
├── x-types/            # 领域类型
├── x-storage/          # 存储层（Postgres + SQLite）
├── x-engine/           # 引擎核心
├── x-api/              # HTTP API
└── x-server/           # 服务端二进制
```

详细说明请参见 [AGENTS.md](AGENTS.md)。

## 项目来源

x.engine 改编自 [orch8](https://github.com/cool-japan/oxify) 工作流引擎核心，
专为 API 编排场景进行了精简和优化。原始项目采用 BUSL-1.1 许可证，
x.engine 采用 Apache-2.0 许可证。
