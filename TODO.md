# x.engine TODO

## 1. Redis 存储后端支持

### 现状
当前存储层支持 **Postgres**（生产）和 **SQLite**（开发/测试/嵌入式），
通过 `StorageBackend` trait 抽象（10 个子 trait）。

### 实现思路

Redis 不适合作为**唯一的**持久化存储（缺乏关系查询、事务范围有限），
最适合作为 **Postgres 的缓存层**或**轻量独立后端**（使用 RedisJSON / RedisStack）。

#### 方案 A：Redis 作为缓存层（推荐）

```
┌─────────────┐    ┌──────────────┐    ┌─────────────┐
│  Engine      │───▶│  RedisCache   │───▶│  Postgres    │
│  (tick loop) │    │  (moka +      │    │  (持久化)    │
└─────────────┘    │   Redis)      │    └─────────────┘
                   └──────────────┘
```

- **不需要修改 StorageBackend trait**
- 在 `x-storage` 中新增 `caching.rs` 模块
- 实现一个 `CachingStorage` 包装器，实现 `StorageBackend`
- 优先读 Redis，miss 后回源 Postgres
- 写操作：先写 Postgres，再失效/更新 Redis 缓存
- 使用 `redis-rs` crate + `fred`（高性能异步连接池）
- 缓存策略：
  - `SequenceDefinition` → 缓存 5 分钟（LRU）
  - `TaskInstance` → 缓存 30 秒（TTL）
  - `ExecutionNode` → 不缓存（频繁写）
  - `BlockOutput` → 不缓存或短 TTL

**文件变更**：
```
x-storage/src/
├── caching.rs          # NEW — CachingStorage 包装器
├── lib.rs              # MOD — 导出 CachingStorage
└── Cargo.toml          # MOD — 添加 redis/fred 依赖
```

**关键代码**：
```rust
pub struct CachingStorage {
    inner: Arc<dyn StorageBackend>,
    redis: fred::pool::RedisPool,
    sequence_cache: moka::sync::Cache<SequenceId, Arc<SequenceDefinition>>,
}
```

#### 方案 B：Redis 作为独立后端（轻量场景）

- 需要 `x-storage/src/redis/` 模块
- 用 `RedisJSON` 模块存储序列定义（JSON 文档）
- 用 Redis Stream 做信号队列
- 用 Redis Sets 做执行树节点
- **局限性**：缺乏 `claim_due_instances` 所需的 `SKIP LOCKED` 语义，
  需要额外实现乐观锁（`WATCH/MULTI/EXEC` 或 Lua 脚本）

**优先级**：方案 A > 方案 B

---

## 2. 分布式横向扩展测试

### 现状
多节点通过 Postgres `SELECT ... FOR UPDATE SKIP LOCKED` 竞争 claim 实例。
节点注册/心跳/摘除机制已就绪（`cluster_nodes` 表），但**缺乏集成测试验证**。

### 需要验证的场景

#### 2.1 双节点无冲突调度

```
场景: 2 个引擎共享同一 Postgres 数据库
步骤:
  1. 启动引擎 A（端口 8080）
  2. 启动引擎 B（端口 8081）
  3. 批量创建 1000 个实例
  4. 验证: 两个引擎各自 claim 了部分实例，无实例被重复 claim
  5. 验证: 全部实例都最终完成
```

**实现方式**：集成测试放在 `x-engine/tests/distributed.rs`

```rust
#[tokio::test]
async fn two_nodes_share_workload() {
    // 1. 创建 Postgres 数据库（Docker compose）
    // 2. 创建两个 Engine 实例，共享同一 storage
    // 3. 批量创建 100 个实例
    // 4. 并发运行两个 tick loop
    // 5. 验证: 每个实例只被处理一次
}
```

#### 2.2 节点故障恢复

```
场景: 节点崩溃后，其实例被其他节点接管
步骤:
  1. 引擎 A 创建并开始处理一个耗时很长的实例（如 sleep 30s）
  2. 引擎 A 进程 kill
  3. 等待 stale_instance_threshold（300s, 测试中调整为 5s）
  4. 验证: 引擎 B 接管了这些实例并完成处理
```

#### 2.3 无重复执行保证

```
场景: 即使多个节点同时 claim，每个实例的每一步只执行一次
关键机制:
  - claim_due_instances 使用 FOR UPDATE SKIP LOCKED
  - 状态机使用 CAS（conditional_update_instance_state）
  - 节点心跳超时后其他节点才可重新 claim
```

#### 2.4 测试工具

```rust
// 辅助函数：创建一个共享存储的分布式测试环境
async fn distributed_test_env() -> (Engine, Engine, Arc<dyn StorageBackend>) {
    let storage = Arc::new(SqliteStorage::in_memory().await.unwrap());
    let e1 = Engine::new(Arc::clone(&storage), config.clone(), handlers.clone(), ct1);
    let e2 = Engine::new(Arc::clone(&storage), config, handlers, ct2);
    (e1, e2, storage)
}
```

---

## 3. 变量上下文跨节点可见性

### 现状
- 上下文（`ExecutionContext`）存储在数据库的 `task_instances.context` 列
- 所有节点读取同一数据库，因此上下文**天然对所有节点可见**
- 实例的状态变更通过 `update_instance_state` / `conditional_update_instance_state` 持久化

### 需要验证的场景

#### 3.1 单实例跨节点上下文可见

```
场景: 实例在节点 A 执行 step1 并写入 context，step2 在节点 B 执行
步骤:
  1. 创建实例的 step1 写入 context.data 中一个值
  2. step1 完成后实例回到 Scheduled 状态
  3. 节点 B claim 该实例，执行 step2
  4. step2 读取 {{context.data.value}}
  5. 验证: 节点 B 能正确读取节点 A 写入的值
```

#### 3.2 信号跨节点可见

```
场景: 实例 A 在节点 A 上执行，实例 B 在节点 B 上执行
步骤:
  1. 实例 A 调用 send_signal handler 发送信号给实例 B
  2. 实例 B 收到信号后继续执行
  3. 验证: 跨节点的信号可达
```

#### 3.3 验证方法

```rust
#[tokio::test]
async fn context_visible_across_nodes() {
    let storage = Arc::new(PostgresStorage::new(&url, 10, None).await.unwrap());
    let (engine_a, engine_b, _) = distributed_test_env(storage).await;
    
    // 创建实例在引擎 A 上运行
    let instance = create_instance_with_sequence(sequence_with_set_state).await;
    engine_a.tick_once().await;
    
    // 让引擎 B claim 并继续
    engine_b.tick_once().await;
    
    // 验证
    let final_state = storage.get_instance(instance.id).await.unwrap();
    assert_eq!(final_state.state, InstanceState::Completed);
    assert_eq!(final_state.context.data["key"], "value");
}
```

---

## 4. API CALL 节点

### 现状
`http_request` handler 已存在（`x-engine/src/handlers/builtin.rs:575`），
支持 GET/POST/PUT/DELETE/PATCH/HEAD 方法。

### 需要完善的方向

#### 4.1 专用 API Call Block 类型

当前 API 调用是通过泛型 Step + handler `http_request` 实现的。
未来可以增加一个专用的 `ApiCall` Block 类型：

```rust
pub struct ApiCallDef {
    pub id: BlockId,
    pub method: ApiMethod,       // GET/POST/PUT/DELETE/PATCH/HEAD
    pub url: String,             // 支持 {{template}}
    pub headers: Vec<(String, String)>,
    pub query: Vec<(String, String)>,
    pub body: Option<serde_json::Value>,
    pub timeout: Option<Duration>,
    pub retry: Option<RetryPolicy>,
    pub cache_ttl: Option<Duration>,  // 响应缓存
}
```

```json
{
  "type": "api_call",
  "id": "get_user",
  "method": "GET",
  "url": "https://api.example.com/users/{{context.data.user_id}}",
  "headers": {
    "Authorization": "Bearer {{context.config.api_key}}"
  },
  "timeout_ms": 10000,
  "retry": {
    "max_attempts": 3,
    "initial_interval_ms": 1000
  }
}
```

#### 4.2 响应处理增强

当前 `http_request` handler 返回完整响应（status + headers + body）。
以下是可增强的方向：

| 功能 | 说明 |
|---|---|
| **响应校验** | 断言 status code 范围、body 包含特定字段 |
| **响应映射** | `{{outputs.get_user.body.id}}` 已在模板中支持 |
| **分页** | 自动遍历分页 API 并合并结果 |
| **条件重试** | 仅对 5xx 重试，4xx 不重试 |
| **超时控制** | 已有，可细化到连接超时/读取超时 |
| **缓存** | 相同请求的去重与缓存（幂等 GET） |

#### 4.3 文件变更

```
x-types/src/
├── sequence.rs    # MOD — 添加 ApiCallDef / ApiMethod
x-engine/src/
└── handlers/
    ├── builtin.rs          # MOD — 增强 handle_http_request
    └── api_call.rs         # NEW — 专用 API Call 处理器
```

#### 4.4 GraphQL / gRPC 支持

```json
{
  "type": "api_call",
  "id": "graphql_query",
  "protocol": "graphql",
  "url": "https://api.example.com/graphql",
  "query": "query User($id: ID!) { user(id: $id) { name email } }",
  "variables": {
    "id": "{{context.data.user_id}}"
  }
}
```

---

## 5. 表达式引擎：写法与支持范围

### 概述
表达式引擎位于 `x-engine/src/expression.rs`，用于**条件判断**（Router、Loop、IfElse）。
支持前缀 `context.data.` / `outputs.` / `steps.` 路径引用。

### 支持的表达式

#### 5.1 字面量

| 类型 | 写法 | 示例 |
|---|---|---|
| 字符串 | `"hello"` / `'hello'` | `"status" == "ok"` |
| 数值 | `123`, `3.14` | `outputs.step1.status_code == 200` |
| 布尔 | `true`, `false` | `context.data.flag == true` |
| 空值 | `null` | `outputs.step1 == null` |

#### 5.2 路径引用

| 路径 | 说明 | 示例 |
|---|---|---|
| `context.data.x` | 运行时数据 | `context.data.user_id == 42` |
| `context.config.x` | 初始化配置 | `context.config.env == "prod"` |
| `context.runtime.x` | 引擎运行时 | `context.runtime.attempt < 3` |
| `outputs.block_id` | 步骤输出（完整 JSON） | `outputs.fetch_user != null` |
| `outputs.block_id.body` | 嵌套路径 | `outputs.fetch_user.body.id == 1` |
| `steps.x.y` | 与 outputs 同义的别名 | `steps.step1.result == "ok"` |
| `state.my_key` | KV 状态 | `state.counter > 0` |

#### 5.3 比较运算符

| 运算符 | 说明 | 示例 |
|---|---|---|
| `==` | 等于 | `outputs.step1.status == 200` |
| `!=` | 不等于 | `context.data.status != "error"` |
| `>` | 大于 | `outputs.step1.count > 10` |
| `>=` | 大于等于 | `context.runtime.attempt >= 3` |
| `<` | 小于 | `outputs.step1.latency_ms < 1000` |
| `<=` | 小于等于 | `context.data.retry_count <= 5` |

#### 5.4 逻辑运算符

| 运算符 | 说明 | 示例 |
|---|---|---|
| `&&` | 与 | `outputs.a != null && outputs.b != null` |
| `\|\|` | 或 | `outputs.a != null \|\| outputs.b != null` |
| `!` | 非 | `!(outputs.step1.status == 200)` |

#### 5.5 成员运算符

| 运算符 | 说明 | 示例 |
|---|---|---|
| `in` | 属于集合 | `context.data.role in ["admin","manager"]` |

#### 5.6 三元运算符

| 写法 | 说明 | 示例 |
|---|---|---|
| `cond ? a : b` | 条件表达式 | `outputs.a != null ? outputs.a : outputs.b` |

#### 5.7 数学运算符（基础）

| 运算符 | 说明 | 示例 |
|---|---|---|
| `+` | 加法 | `outputs.a + outputs.b` |
| `-` | 减法 | `outputs.a - 1` |
| `*` | 乘法 | `outputs.price * outputs.qty` |
| `/` | 除法 | `outputs.total / outputs.count` |

### 模板引擎（独立于表达式引擎）

**位置**: `x-engine/src/template.rs`

用于在 Step 的 `params` JSON 中做值替换，写法 `{{path}}`。

#### 5.8 Pipe 过滤器

| 过滤器 | 说明 | 示例 |
|---|---|---|
| `upper` | 转大写 | `{{outputs.name \| upper}}` |
| `lower` | 转小写 | `{{outputs.email \| lower}}` |
| `trim` | 去空格 | `{{context.data.input \| trim}}` |
| `abs` | 绝对值 | `{{outputs.amount \| abs}}` |
| `url_encode` | URL 编码 | `{{outputs.query \| url_encode}}` |
| `base64` | Base64 编码 | `{{outputs.binary \| base64}}` |
| `replace(a,b)` | 字符串替换 | `{{outputs.text \| replace("foo","bar")}}` |
| `default(val)` | 默认值 | `{{outputs.maybe_null \| default("fallback")}}` |
| `truncate(n)` | 截断 | `{{outputs.long_text \| truncate(100)}}` |
| `round()` | 四舍五入 | `{{outputs.price \| round()}}` |
| `join(sep)` | 数组合并为字符串 | `{{outputs.items \| join(",")}}` |
| `split(sep)` | 字符串分割为数组 | `{{outputs.csv \| split(",")}}` |
| `hash(alg)` | 哈希（sha256/md5） | `{{outputs.secret \| hash("sha256")}}` |

### 条件求值

`evaluate_condition()` 用于 Router 和循环块的决策：

```json
{
  "type": "router",
  "id": "status_router",
  "routes": [
    {
      "condition": "outputs.check_status.status == 200",
      "blocks": [...]
    },
    {
      "condition": "outputs.check_status.status >= 400 && outputs.check_status.status < 500",
      "blocks": [...]
    }
  ],
  "default": [...]
}
```

### 表达式缓存

- Tokenization 结果缓存在 `moka::sync::Cache<String, Arc<[Token]>>`（10K 条目，TTL 10 分钟）
- 频繁使用的表达式（如 Router 条件）无需重复解析

### 未来增强

| 需求 | 说明 |
|---|---|
| **JSON Path** | 支持 `$.store.book[0].title` 语法 |
| **正则匹配** | `=~` 运算符 |
| **日期函数** | `now()`, `parse_date()`, `format_date()` |
| **聚合函数** | `sum()`, `avg()`, `count()` |
| **自定义函数注册** | handler 可注册自定义表达式函数 |

---

## 6. 外部 Worker HTTP API

### 背景

引擎中已经具备 Worker 调度的**数据层基础设施**：

1. 当 step handler 未在内置 handler 列表中注册时，引擎自动创建 `WorkerTask` 记录写入 DB（`step_dispatch.rs` / `step_exec.rs`）
2. 执行树节点状态被设为 `Waiting`，等待外部 Worker 完成
3. 后台 Worker Reaper 会清理超时/僵死的任务

**但缺少消费者**：没有传输层把任务投递给外部 Worker，也收不回结果。WorkerTask 会一直 stuck 在 `Pending` 状态，直到被 Reaper 回收。

### 目标

在 `x-api` 中增加一组轻量 HTTP 端点，替代原项目中 gRPC 的角色，让外部 Worker 可以通过 HTTP JSON 协议与引擎交互。

### API 设计

```
POST /api/v1/workers/poll       — Worker 拉取待处理任务
POST /api/v1/workers/complete   — Worker 上报任务完成
POST /api/v1/workers/fail       — Worker 上报任务失败
POST /api/v1/workers/heartbeat  — Worker 保活心跳
```

#### 请求/响应

```json
// POST /api/v1/workers/poll
// Request
{
  "handler_name": "my_custom_handler",
  "worker_id": "worker-1",
  "limit": 10
}
// Response
{
  "tasks": [
    {
      "id": "uuid",
      "instance_id": "uuid",
      "block_id": "step_1",
      "handler_name": "my_custom_handler",
      "params": {},
      "context": {},
      "attempt": 1,
      "timeout_ms": 30000
    }
  ]
}
```

```json
// POST /api/v1/workers/complete
// Request
{
  "task_id": "uuid",
  "worker_id": "worker-1",
  "output": {"result": "success"}
}
// Response: 200 OK
```

```json
// POST /api/v1/workers/fail
// Request
{
  "task_id": "uuid",
  "worker_id": "worker-1",
  "message": "connection refused",
  "retryable": true
}
// Response: 200 OK
```

```json
// POST /api/v1/workers/heartbeat
// Request
{
  "task_id": "uuid",
  "worker_id": "worker-1"
}
// Response: 200 OK
```

### 服务端实现

```
x-api/src/
├── lib.rs          # MOD — 注册 worker routes
├── workers.rs      # NEW — 4 个 HTTP handler
└── error.rs        # MOD — 新增 ApiError 变体
```

**关键逻辑**：

- `poll` → 调用 `storage.claim_worker_tasks(handler_name, worker_id, limit)`，返回任务列表
- `complete` → 保存输出、标记执行节点 Completed、将实例转回 Scheduled（原子操作）
- `fail` → 根据 `retryable` 判断自动重试或标记节点为 Failed
- `heartbeat` → 调用 `storage.heartbeat_worker_task(task_id, worker_id)`

### Worker 客户端 SDK（可选）

```
x-worker-sdk/
├── Cargo.toml
└── src/
    ├── lib.rs       # Worker client 封装
    ├── poll.rs      # 轮询任务循环
    └── exec.rs      # 任务执行 + 结果回传
```

一个轻量的 SDK，封装 HTTP 调用 + 心跳循环：

```rust
pub struct WorkerClient {
    engine_url: String,
    worker_id: String,
    handler_name: String,
}

impl WorkerClient {
    pub async fn run<F>(&self, exec: F)
    where
        F: Fn(WorkerTask) -> Result<Value, WorkerError>,
    {
        loop {
            let tasks = self.poll().await?;
            for task in tasks {
                match exec(task).await {
                    Ok(output) => self.complete(task.id, output).await?,
                    Err(e) => self.fail(task.id, e).await?,
                }
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
}
```

### 与原始 gRPC 方案的对比

| 维度 | gRPC（原项目） | HTTP API（本方案） |
|---|---|---|
| 协议 | Protobuf over HTTP/2 | JSON over HTTP |
| 实现成本 | 需要 proto 编译、tonic 服务器 | 复用现有 axum 基础设施 |
| Worker 兼容性 | 需要 protobuf 生成客户端 | curl / 任意 HTTP 客户端即可 |
| 性能 | 二进制编码，略高 | JSON 编码，足够 |
| 流式支持 | 支持 Server Streaming | 暂不需要（poll 模型是 request-response） |
| 代码量 | ~2500 行（proto + service + auth） | ~300 行（4 个 handler + 存储层调用） |

### 为什么不直接用消息队列？

消息队列（Redis / RabbitMQ / NATS）是另一个方向，但与 HTTP API 方案不冲突：

| 方案 | 优点 | 缺点 | 适用场景 |
|---|---|---|---|
| **HTTP Polling** | 零依赖，Worker 只需 HTTP 客户端 | 延迟取决于 poll 间隔 | 简单、跨语言、快速落地 |
| **消息队列** | 实时推送、缓冲、削峰填谷 | 引入中间件依赖 | 高吞吐、大规模分布式 Worker |
| **WebSocket** | 实时双向通信 | 状态管理复杂 | 需要实时推送任务进度的场景 |

**推荐路线**：先实现 HTTP API（快速可用），后续可按需扩展消息队列。

---

## 7. 大变量处理（PDF OCR 等大数据场景）

### 场景描述

应用中有大 JSON 变量，例如一个 PDF 文件的完整 OCR 结果（数 MB）。这些变量需要在工作流中的多个 step 之间传递和引用。

### 当前限制

#### 1. context.data — 256 KiB 硬上限

```rust
// x-types/src/context.rs:27
pub const DEFAULT_MAX_CONTEXT_BYTES: u32 = 256 * 1024;  // 256 KiB
```

整个 `ExecutionContext` 序列化后不能超过此值，否则返回 `ContextTooLarge` 错误。

#### 2. 大字段导致 DB 查询变慢

即使外部化后，每个 tick 仍通过 `preload_externalized_markers()` **eager load** 所有 externalized 字段回内存。当字段大到数 MB 时：

- 每次 `claim_due_instances` / `update_instance` 都要在网络传输大 JSON
- 查询变慢 → 连接占用时间变长 → 连接池耗尽的风险
- 即使外部化到 `externalized_state` 表，每 tick 的批查询仍会传输所有大字段

### 现状

- 引擎已有 **Externalization 机制**（`x-storage/src/externalizing.rs`）：将超过 threshold 的 top-level 字段自动替换为 marker（~60 字节），原始值存入 `externalized_state` 表
- **但默认关闭**：`SchedulerConfig.externalize_output_threshold` 默认为 0
- 每个 tick 的 `preload_externalized_markers()` 使用 `batch_get_externalized_state` 一次批量回填所有 marker
- Block output 侧的 externalization 通过 `maybe_externalize()` 处理（`x-engine/src/handlers/step.rs:372-412`）
- `max_context_bytes: 0` 可关闭大小检查，但大字段仍会在每 tick 传输

### 解决方案选型

| 方案 | 说明 | 实现成本 | 对大变量的支持 | DB 压力 |
|---|---|---|---|---|
| **A: 启用外部化** | 设置 `externalize_output_threshold: 1024` + `max_context_bytes: 0`，让引擎自动外部化大字段 | ✅ 配置一行 | 🟡 可存，但每 tick 仍传输 | 🟡 中等 |
| **B: 存引用** | context 只存文件引用（bucket/key/size），需要时通过 handler 读取 | 🟡 写一个 handler | ✅ 完美隔离 | 🟢 低 |
| **C: 懒加载** | 增强 preload 机制：只有 handler 真正访问时才从 externalized_state 读取 | 🔴 较大改动 | ✅ 最优 | 🟢 低 |

#### 方案 A：启用外部化（零代码改动）

```rust
SchedulerConfig {
    externalize_output_threshold: 1024,  // 1KB 以上字段自动外部化
    max_context_bytes: 0,                // 0 = 关闭大小检查
    ..
}
```

**注意**：block output 的外部化已支持（`step.rs:maybe_externalize`），context.data 的外部化也完整。但即使外部化，每 tick 仍会 eager load 所有字段回内存。

#### 方案 B：存引用（推荐）

不在 context 中存大 JSON，而是存一个轻量引用：

```json
{
  "ocr_file": {
    "bucket": "my-bucket",
    "key": "ocr/inst_abc123.json",
    "size_bytes": 5242880
  }
}
```

需要时通过 handler 按需读取：

```json
{
  "handler": "http_request",
  "params": {
    "method": "GET",
    "url": "https://storage.example.com/{{context.data.ocr_file.key}}"
  }
}
```

或自定义 object storage handler 实现 S3/MinIO 读取。

#### 方案 C：Lazy Load（需要开发）

修改 `preload_externalized_markers`：
- 标记哪些字段是 externalized
- 在模板/表达式解析时，只有真正访问到该字段才去 `externalized_state` 表读取
- 可以配合 `FieldAccess` 机制（`ContextAccess.data.fields`）预声明需要读取的字段

**关联模块**：
```
x-engine/src/
├── preload.rs          # MOD — 从 eager load 改为 lazy load
├── template.rs         # MOD — 在路径解析时触发 lazy load
├── expression.rs       # MOD — 同上
└── handlers/
    └── param_resolve.rs # MOD — OutputsSnapshot 同理支持 lazy
```

### 推荐路线

1. **短期（本周）**：方案 A + B 结合
   - 启用 externalization（防止小字段意外膨胀）
   - 大 JSON 存文件引用，用 `http_request` handler 读取
   - 如需要 S3/MinIO，实现一个简单的 object storage handler

2. **中期（后续 sprint）**：方案 C
   - 实现懒加载机制，让 `preload` 真正按需加载
   - handler 不再需要手动管理引用

### 相关文件

```
x-types/src/
├── config.rs            # SchedulerConfig.externalize_output_threshold
└── context.rs           # DEFAULT_MAX_CONTEXT_BYTES / check_size()

x-storage/src/
├── externalizing.rs     # externalize_fields() / is_marker() / marker()
├── postgres/
│   └── instances.rs     # update_instance_context_externalized()
└── sqlite/
    └── instances.rs     # 同上

x-engine/src/
├── preload.rs           # preload_externalized_markers() — 批量回填
├── handlers/
│   └── step.rs          # maybe_externalize() — block output 外部化
└── gc.rs                # externalized_state 清理
```

---

## 优先级与时间线

| 任务 | 优先级 | 预估工作量 | 说明 |
|---|---|---|---|
| 分布式集成测试 | 🔴 高 | 2-3 天 | 验证核心功能的正确性 |
| 上下文跨节点可见性测试 | 🔴 高 | 1-2 天 | 与分布式测试合并 |
| **外部 Worker HTTP API** | 🔴 高 | 2-3 天 | 让外部 Worker 可通过 HTTP 拉取/完成任务 |
| **HandlerSchema 注册机制（x-ui P1）** | 🔴 高 | 2-3 天 | 在 HandlerRegistry 中增加 schema 字段，LLM 可感知可用节点 |
| **大变量懒加载（方案 C）** | 🟡 中 | 3-5 天 | context.data 按需读取，避免每 tick eager load |
| **x-ui 基础框架 + LLM 对话（P2）** | 🟡 中 | 1 周 | x-ui 项目初始化、LLM 对话生成序列 |
| API Call 节点增强 | 🟡 中 | 3-5 天 | 专用 Block 类型 + 响应处理 |
| **序列画布 + 手动编辑（x-ui P3）** | 🟡 中 | 2 周 | React Flow 集成、拖拽编辑 |
| **实时调试 SSE（x-ui P4）** | 🟢 低 | 1.5 周 | x-api 增加 SSE event 端点 + 前端调试浮层 |
| **OpenAPI 导入（x-ui P5）** | 🟢 低 | 1 周 | 解析 OpenAPI 规范自动注册 handler |
| **实时 Prompt 调整（x-ui P6）** | 🟢 低 | 2 周 | 运行时修改未执行节点 |
| Redis 缓存 | 🟢 低 | 5-7 天 | 方案 A（缓存层），性能优化 |
| Redis 独立后端 | 🟢 低 | 10-15 天 | 方案 B，仅轻量场景 |
| 表达式引擎增强 | 🟡 中 | 3-5 天 | JSON Path / 正则 / 日期函数 |
