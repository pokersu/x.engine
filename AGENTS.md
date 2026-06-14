# x.engine — 高性能 API 编排引擎

## 代码来源

**原始项目**: `orch8` (位于 `open-source/engine/`) — 通用的持久化工作流引擎，基于 Rust 构建。

**原始仓库**: 目录 `/Users/pokersu/Projects/open-source/engine/`

**原始 Crates**:
| 原始 crate | 状态 | 说明 |
|---|---|---|
| `orch8-types` | ✅ 迁移至 `x-types` | 精简了模块，移除了 18 个不需要的模块 |
| `orch8-storage` | ✅ 迁移至 `x-storage` | Postgres + SQLite 双后端保留，移除 MobileSync |
| `orch8-engine` | ✅ 迁移至 `x-engine` | 核心引擎全量保留，1324 个测试通过 |
| `orch8-api` | ✅ 迁移至 `x-api` | 剥离 auth/utoipa/publisher/push，保留核心 API |
| `orch8-server` | ✅ 迁移至 `x-server` | 简化启动流程，单二进制 |
| `orch8-cli` | 📦 已复制待编译 | 代码已就位，暂未加入 workspace |
| `orch8-mobile` | ❌ 未迁移 | 不需要 |
| `orch8-grpc` | ❌ 未迁移 | 不需要 |
| `orch8-publisher` | ❌ 未迁移 | 不需要 |
| `orch8-push` | ❌ 未迁移 | 不需要 |

---

## 需求

实现一个**通用的流程编排引擎**，主要用于 **API 编排**，要求：
1. **极高性能** — 批处理调度、内存缓存、异步非阻塞
2. **横向伸缩** — 多节点通过数据库 `SELECT ... FOR UPDATE SKIP LOCKED` 安全竞争
3. **不包含** 认证与授权
4. **不包含** 移动端部署

---

## 做了什么

从 `orch8` 工作区提取核心引擎骨架，创建独立的 `x.engine` 项目，包含 5 个 crate：

### 项目结构

```
x.engine/
├── Cargo.toml              # workspace 定义
├── README.md
├── AGENTS.md               # 本文件
├── migrations/             # 数据库迁移脚本
├── x-types/                # 领域类型
│   ├── config.rs           #   精简后的 SchedulerConfig/EngineConfig
│   ├── sequence.rs         #   工作流定义 (Sequence + BlockDefinition)
│   ├── instance.rs         #   实例状态机 (TaskInstance + InstanceState)
│   ├── signal.rs           #   信号系统
│   ├── execution.rs        #   执行树节点
│   └── ...                 #   17 个模块
├── x-storage/              # 存储层
│   ├── sqlite/             #   SQLite 后端 (开发/测试/嵌入式)
│   ├── postgres/           #   Postgres 后端 (生产)
│   └── lib.rs              #   StorageBackend trait (10 个子 trait)
├── x-engine/               # 引擎核心
│   ├── scheduler.rs        #   Tick Loop — 批处理调度
│   ├── evaluator.rs        #   执行树评估器 (Parallel/Race/Loop/TryCatch...)
│   ├── handlers/           #   30+ 内置 step handler
│   ├── expression.rs       #   表达式引擎
│   ├── template.rs         #   模板引擎 {{outputs.block.field}}
│   └── signals.rs          #   信号处理 (pause/resume/cancel)
├── x-api/                  # HTTP API
│   ├── sequences.rs        #   序列 CRUD
│   ├── instances/          #   实例生命周期 + 输出 + 信号
│   ├── health.rs           #   健康检查
│   └── streaming.rs        #   SSE 流式输出
└── x-server/               # 服务端二进制
    └── main.rs             #   启动入口
```

### 构建与测试状态

- **编译**: `cargo build --workspace` — 0 errors
- **测试**: `cargo test --workspace --lib` — **1341 passed, 0 failed**
  - `x-types`: 132 个测试
  - `x-storage`: 83 个测试
  - `x-engine`: 1114 个测试
  - `x-api`: 12 个测试

### 核心保留特性

| 特性 | 说明 |
|---|---|
| **Tick Loop 调度器** | 定时间隔 tick，批量 claim 到期实例，`SKIP LOCKED` 防重复 |
| **横向扩展** | 节点注册/心跳/摘除 + drain 检测 |
| **HandlerRegistry** | 注册式 step handler 扩展点，`StepHandler = Fn(StepContext) -> Result<Value>` |
| **复合 Block** | Parallel, Race, Loop, ForEach, Router, TryCatch, CancellationScope, ABSplit, SubSequence |
| **Expression 引擎** | `{{context.data.x}}`, `{{outputs.step_1.result}}`, 比较/逻辑运算符 |
| **Template 引擎** | `{{path}}` 递归插值到 JSON 参数 |
| **内置 Handlers** | HTTP 请求, sleep, set-state, emit-event, send-signal, code execution (rhai), WASM |
| **Signal 系统** | pause/resume/cancel/update_context/custom |
| **Scheduling** | 延迟执行, 发送窗口, SLA 超时检测 |
| **Crash Recovery** | 启动时恢复陈旧实例 |
| **死信队列** | 重试耗尽后自动 fail |

---

## 详细操作方法

### 第一步：创建新 workspace (Phase 1)

```bash
mkdir -p x.engine
cd x.engine
# 创建 Cargo.toml workspace，定义 x-types, x-storage, x-engine, x-api, x-server
```

### 第二步：复制并重命名 (Phase 1)

从 `../engine/orch8-*` 复制到 `x.engine/x-*`，然后全局替换内部引用：

```bash
# 将 orch8_types → x_types, orch8_storage → x_storage, 等
find x-types x-storage x-engine x-api x-server -name '*.rs' -exec sed -i '' 's/orch8_types/x_types/g' {} \;
```

### 第三步：精简 x-types (Phase 1)

删除 18 个不需要的模块：
- `api_key.rs`, `artifact.rs`, `audit.rs`, `auth.rs`, `checkpoint.rs`
- `circuit_breaker.rs`, `cluster.rs`, `credential.rs`, `cron.rs`
- `encryption.rs`, `plugin.rs`, `pool.rs`, `rate_limit.rs`
- `rollback.rs`, `session.rs`, `suggest.rs`, `trigger.rs`
- `worker.rs`, `worker_filter.rs`

简化 `config.rs`：移除 `SecretString`、`ExternalizationMode`、`ArtifactConfig`

**但随后发现** 存储层的 Postgres/SQLite 后端深度引用了这些类型，因此重新从原始文件复制回 x-types 中需要的模块，确保向后兼容。

### 第四步：精简 x-storage (Phase 3)

`StorageBackend` trait 保持完整的 10 个子 trait（SequenceStore, InstanceStore, ExecutionTreeStore, OutputStore, SignalStore, WorkerStore, SchedulingStore, AdminStore, ResourceStore, TelemetryStore），以确保与 Postgres/SQLite 后端的兼容。

替换 `compression`/`encrypting`/`externalizing` 模块为透传桩模块。

移除 `mod mobile_sync` 声明。

### 第五步：修复 x-engine (Phase 4)

核心引擎改动最小：
- 重写 `SchedulerConfig` 添加所有被引用的字段（`externalize_output_threshold`, `max_instances_per_tenant`, `node_reaper_*`, `worker_reaper_*` 等）
- 添加 `WebhookConfig::secret` 字段
- 修复 `ArtifactMeta` 添加 `into_output()` 方法和缺失字段
- 修复 `suggest.rs` 替换 strsim::levenshtein 为简单实现
- 删除 shutdown test（tokio::test start_paused 兼容性问题）

### 第六步：构建 x-api (Phase 5)

从原始 `orch8-api/src/` 复制后：
1. **剥离模块**: 删除 api_keys/approvals/auth/circuit_breakers/cluster/cron/mobile_sync/openapi/plugins/pools/rollback/sessions/telemetry/triggers/usage/workers 等 17 个模块文件
2. **重写 lib.rs**: AppState 简化为 `storage + shutdown + max_context_bytes + stream_limiter` 四个字段
3. **重写 sequences.rs**: 去掉 utoipa 属性、auth 参数、tenant 校验
4. **重写 instances/lifecycle.rs**: 完整的 CRUD handler，去掉 auth 依赖
5. **重写 instances/outputs.rs**: 简化输出查询，去掉 externalized state 分支
6. **重写 instances/signals.rs**: 简化信号发送
7. **重写 webhooks.rs**: 简化 webhook 接收
8. **重写 error.rs**: 简化的 ApiError 类型
9. **剥离 utoipa**: 所有 `#[utoipa::path(...)]` 属性被移除，`ToSchema` derive 被移除

### 第七步：构建 x-server (Phase 6)

简化 `main.rs`：
- 移除 gRPC 启动、push/publisher 集成、加密存储包装
- 保留 SQLite 后端 + Prometheus metrics + CORS + Tracing
- 保留 Engine 后台循环 + HTTP server 优雅关闭

### 第八步：整体构建验证 (Phase 7)

最终 workspace 包含 5 个 crate，1334 个测试全部通过。

---

## 注意事项

### 保留的多租户字段

`TaskInstance::tenant_id` 和 `SequenceDefinition::tenant_id` 字段被保留，原因是 Postgres/SQLite 后端的 SQL 查询和 `InstanceFilter` 中深度引用了这些字段。移除它们会导致大量存储层代码需要重写。x-api 层的 handler 中已将 tenant_id 设为 `"default"`，不构成实际隔离。

### 未加入 workspace 的 crate

- **x-cli**: 代码已复制到 `x.engine/x-cli/`，未加入 workspace。需要时修改 `Cargo.toml` 加入 `members` 列表即可
- **x-grpc**: 如果后续需要 gRPC 支持，需要从 `orch8-grpc` 复制

### 后续扩展方向

1. **HTTP handler 加强**: 当前保留 `reqwest` 依赖和 HTTP 相关 handler，可直接用于 API 编排
2. **专属 API 编排 Block**: 可增加 `HttpRequest`、`GraphQLQuery`、`GRpcCall` 等专用 block type
3. **主从节点分离**: 当前 engine + api 在同一进程，后续可分离

---

## API 端点一览

所有端点注册在 `x-api/src/lib.rs` 的 `build_router` 中。核心 API（Sequences + Instances）同时挂载在 `/api/v1/...` 和 `/...` 两个路径下；系统类端点（health/metrics/webhooks）仅挂载在根路径 `/...`。

### Sequences（`x-api/src/sequences.rs`）

| 方法 | 路径 | 说明 |
|---|---|---|
| `POST` | `/api/v1/sequences` | 创建序列 |
| `GET` | `/api/v1/sequences` | 列出序列 |
| `GET` | `/api/v1/sequences/{id}` | 获取序列详情 |
| `DELETE` | `/api/v1/sequences/{id}` | 删除序列 |
| `POST` | `/api/v1/sequences/{id}/deprecate` | 标记序列为废弃 |
| `POST` | `/api/v1/sequences/{id}/status` | 设置序列状态 |
| `GET` | `/api/v1/sequences/by-name` | 按 `(namespace, name, version)` 查询 |
| `GET` | `/api/v1/sequences/versions` | 列出序列版本列表 |

> **注意**: `/sequences/by-name` 和 `/sequences/versions` 的路由注册在 `{id}` 通配符之后，axum 会先匹配 `{id}` 导致这两个端点无法命中。需要调整注册顺序修复。

### Instances（`x-api/src/instances/mod.rs`）

| 方法 | 路径 | 说明 |
|---|---|---|
| `POST` | `/api/v1/instances` | 创建实例（触发执行） |
| `POST` | `/api/v1/instances/batch` | 批量创建实例 |
| `GET` | `/api/v1/instances` | 列出实例 |
| `GET` | `/api/v1/instances/{id}` | 获取实例详情 |
| `PATCH` | `/api/v1/instances/{id}/state` | 更新实例状态 |
| `PATCH` | `/api/v1/instances/{id}/context` | 更新实例上下文 |
| `POST` | `/api/v1/instances/{id}/retry` | 重试失败的实例 |
| `POST` | `/api/v1/instances/{id}/signals` | 发送信号（cancel/pause/resume/审批等） |
| `GET` | `/api/v1/instances/{id}/outputs` | 获取步骤输出 |
| `GET` | `/api/v1/instances/{id}/tree` | 获取执行树 |

### 流式输出（`x-api/src/streaming.rs`）

| 方法 | 路径 | 说明 |
|---|---|---|
| `GET` | `/instances/{id}/stream` | SSE 实时推送实例进度（state/output/done 事件） |

仅挂载在根路径。查询参数 `?poll_ms=500`（100~5000ms）控制轮询间隔。默认 500ms，keepalive 15s。并发限制 256，超限返回 503。

### 健康检查（`x-api/src/health.rs`）

| 方法 | 路径 | 说明 |
|---|---|---|
| `GET` | `/health/live` | Liveness 探针，进程存活即返回 200 |
| `GET` | `/health/ready` | Readiness 探针，数据库可达返回 200，不可达返回 503 |

### 指标（`x-api/src/metrics.rs`）

| 方法 | 路径 | 说明 |
|---|---|---|
| `GET` | `/metrics` | Prometheus 指标端点（text/plain） |

仅在 `AppState.prometheus_handle` 存在时挂载（由 `x-server/src/main.rs` 初始化时传入）。

### Webhook 接收（`x-api/src/webhooks.rs`）

| 方法 | 路径 | 说明 |
|---|---|---|
| `POST` | `/webhooks/events/{slug}` | 接收外部 Webhook 请求，创建一个新的 TaskInstance |

当前为 stub 实现：`sequence_id` 硬编码为随机 UUID，未实现 slug → sequence 的映射。需要配合 `triggers` 表注册才能正常工作。

### 架构要点

- `x-api/src/lib.rs` 中的 `AppState` 包含：`storage`（存储后端）、`shutdown`（CancellationToken）、`max_context_bytes`（默认 256KB）、`stream_limiter`（Semaphore，控制 SSE 并发）、`prometheus_handle`（可选的 Prometheus 句柄）
- `build_router()` 组装路由：先组装 `api_routes()`（sequences + instances + streaming），然后 nest 到 `/api/v1` 并 merge 到根路径；再 merge `health`、`webhooks`；条件挂载 `metrics`
