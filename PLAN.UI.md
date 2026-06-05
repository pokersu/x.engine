# x-ui — LLM-native 工作流编辑器（设计蓝图）

> **x-ui** 是一个独立于 x-engine 的前端应用，目标是通过 LLM 自然语言对话来创建、调试和修改工作流序列。它不是简单的拖拽编辑器，而是一个 **LLM-native 的工作流 Studio**。

---

## 1. 架构概览

```
┌──────────────────────────────────────────────────────────────┐
│                      x-ui (独立进程)                           │
│                                                              │
│   Frontend (React + TypeScript + React Flow)                  │
│  ┌──────────────┐  ┌──────────────┐  ┌────────────────────┐ │
│  │  序列画布      │  │  实时监控      │  │  LLM 对话面板      │ │
│  │  (流程图)      │  │  (节点IO)     │  │  (聊天界面)        │ │
│  └──────────────┘  └──────────────┘  └────────────────────┘ │
│                                                              │
│   Backend (Rust axum)                                        │
│  ┌────────────┐ ┌──────────┐ ┌──────────┐ ┌──────────────┐ │
│  │ LLM Gateway│ │ SSE Hub  │ │Session   │ │ Schema       │ │
│  │(多后端路由) │ │ (实时流)  │ │ Manager  │ │ Registry     │ │
│  └────────────┘ └──────────┘ └──────────┘ └──────────────┘ │
└──────────────────────────┬───────────────────────────────────┘
                           │ HTTP + SSE
                           ▼
┌──────────────────────────────────────────────────────────────┐
│                    x-api (现有 REST API)                       │
│  /api/v1/sequences  /api/v1/instances  /api/v1/...           │
└──────────────────────────────────────────────────────────────┘
```

## 2. 技术选型（已确认）

| 层面 | 选择 | 理由 |
|---|---|---|
| **前端框架** | React + TypeScript | 类型安全、生态成熟、React Flow 集成 |
| **画布** | React Flow | 社区活跃、支持自定义节点、插件丰富 |
| **LLM 后端** | 多后端支持 | OpenAI / Anthropic / 本地 ollama，可插拔路由 |
| **Handler Schema** | 代码注册 + 配置文件声明 | 内置 handler 代码注册，自定义 API 用配置文件 |

## 3. 核心功能

### 3.1 编辑器——两种协作模式

```
┌──────────────────────────────────────────────────────────────┐
│  模式切换: [💬 LLM 对话] [✋ 手动编辑]                       │
│                                                              │
│  ┌──────────────────────────────────────────────────────────┐│
│  │  画布 (React Flow)                                       ││
│  │                                                          ││
│  │  [Start] ──▶ [fetch_user] ──▶ [check_role] ──▶ [Router] ││
│  │                                        │         │       ││
│  │                                        │    ┌────┴────┐  ││
│  │                                        │    │ success │  ││
│  │                                        │    │  notify  │  ││
│  │                                        │    └─────────┘  ││
│  │                                        │         │       ││
│  │                                        │    ┌────┴────┐  ││
│  │                                        │    │ fail    │  ││
│  │                                        │    │ retry   │  ││
│  │                                        │    └─────────┘  ││
│  └──────────────────────────────────────────────────────────┘│
│                                                              │
│  ┌──────────────────────────────────────────────────────────┐│
│  │  LLM 对话面板 (右侧)                                      ││
│  │                                                          ││
│  │  User: 帮我创建一个序列，先获取用户信息，然后检查角色，    ││
│  │        如果是 admin 发通知，否则返回错误                   ││
│  │                                                          ││
│  │  LLM: 好的，我创建了以下序列:                             ││
│  │    ① fetch_user  — 调用用户API                           ││
│  │    ② check_role  — 检查 role 字段                       ││
│  │    ③ Router      — 根据角色分支                          ││
│  │       ├ success  → notify                               ││
│  │       └ fail     → return_error                          ││
│  │                                                          ││
│  │  [✓ 应用] [✗ 丢弃] [✏️ 修改]                              ││
│  └──────────────────────────────────────────────────────────┘│
└──────────────────────────────────────────────────────────────┘
```

- **LLM 对话模式**：用户用自然语言描述需求 → LLM 生成 JSON diff → 确认后应用到画布
- **手动编辑模式**：拖拽节点、配置参数，与 LLM 模式共享同一个序列状态
- 两种模式**完全可切换**，任意一种的修改对另一种立即可见

### 3.2 实时调试器

调试时每个节点显示实时状态浮层：

| 节点状态 | 视觉 |
|---|---|
| 待执行 | 灰色边框 |
| 执行中 | 蓝色脉冲动画 |
| 成功 | 绿色 + 耗时 |
| 失败 | 红色 + 错误信息 |
| 跳过 | 灰色虚线 |

点击节点展开详情面板，显示：

```
┌─────────────────────────────────┐
│  fetch_user                     │
│  URL: GET /api/users/{{id}}    │
│  Status: 200 OK                 │
│  Duration: 340ms                │
│  Attempt: 1                     │
├─────────────────────────────────┤
│  Input Params:                  │
│  { "id": "{{data.user_id}}" }   │
│  → resolved: { "id": 42 }      │
├─────────────────────────────────┤
│  Output:                        │
│  { "id": 42, "name": "Alice" } │
├─────────────────────────────────┤
│  [✏️ 修改参数并重试] [⏭ 跳过]   │
└─────────────────────────────────┘
```

### 3.3 实时 Prompt 调整

调试过程中发现问题，直接在 LLM 面板描述修改意图：

```
User: fetch_user 返回字段太多了，我只要 id 和 name

LLM:  两个方案：
      方案 A: 在 fetch_user 后加 transform 节点过滤字段
      方案 B: 修改 URL 添加 ?fields=id,name

      你选哪个？
```

技术要点：
- LLM 收到当前序列定义 + 调试上下文（节点输入/输出）
- 返回 JSON patch（RFC 6902），只修改未执行节点
- 已执行节点不可修改（保证可追溯性）

## 4. Handler Schema 注册机制

这是让 LLM **知道有哪些可用节点**的核心设计。

### 4.1 代码注册（内置 handler）

在 `x-engine` 现有的 `HandlerRegistry` 中增加 schema 字段：

```rust
pub struct HandlerSchema {
    pub name: String,                    // handler 标识符
    pub description: String,             // 人类可读描述
    pub params: serde_json::Value,       // JSON Schema (参数规范)
    pub output: serde_json::Value,       // JSON Schema (输出规范)
    pub examples: Vec<Example>,          // few-shot 示例
    pub tags: Vec<String>,               // 分类标签 (如 "api", "ai", "data")
}
```

内置 handler 注册时附带 schema：

```rust
registry.register(
    "send_email",
    email_handler,
    HandlerSchema {
        description: "发送邮件通知",
        params: json!({
            "type": "object",
            "properties": {
                "to":        { "type": "string", "description": "收件人邮箱" },
                "subject":   { "type": "string", "description": "邮件标题" },
                "body":      { "type": "string", "description": "邮件内容 (支持 HTML)" },
            },
            "required": ["to", "subject"]
        }),
        output: json!({
            "type": "object",
            "properties": {
                "message_id": { "type": "string" },
                "status":     { "type": "string", "enum": ["sent", "queued"] }
            }
        }),
        examples: vec![Example {
            params: json!({
                "to": "user@example.com",
                "subject": "欢迎 {{context.data.user_name}}",
            }),
            output: json!({ "message_id": "msg_123", "status": "sent" }),
        }],
        tags: vec!["communication", "email"],
    },
);
```

### 4.2 配置文件声明（自定义 API）

在 `x-ui` 中定义配置目录，用户可添加 YAML 文件声明自定义 API：

```yaml
# x-ui/apis/user-service.yaml
name: "用户服务 API"
base_url: "https://api.example.com/v2"
auth:
  type: bearer
  token_from: "{{context.config.api_key}}"

endpoints:
  - name: get_user
    description: "根据 ID 查询用户信息"
    method: GET
    path: "/users/{{user_id}}"
    params:
      user_id:
        type: string
        description: "用户 UUID"
        required: true
    response_schema:
      type: object
      properties:
        id: { type: string }
        name: { type: string }
        email: { type: string }
        role: { type: string, enum: ["admin", "user", "guest"] }
    examples:
      - params: { user_id: "u_123" }
        response: { id: "u_123", name: "Alice", role: "admin" }

  - name: list_users
    description: "分页查询用户列表"
    method: GET
    path: "/users"
    params:
      page:    { type: integer, default: 1 }
      per_page: { type: integer, default: 20 }
    response_schema:
      type: object
      properties:
        items: { type: array, items: { type: object } }
        total: { type: integer }
```

x-ui 启动时扫描配置目录，自动生成 `HandlerSchema`，注入 LLM 上下文。

### 4.3 OpenAPI 规范导入

用户可在 UI 上直接粘贴 OpenAPI 规范的 URL 或 JSON：

```
用户上传 OpenAPI 规范 (Swagger):
  GET /api/users      → 自动注册为 api.user_service.list_users
  GET /api/users/{id} → 自动注册为 api.user_service.get_user
  POST /api/users     → 自动注册为 api.user_service.create_user
```

导入流程：
1. 解析 OpenAPI 规范
2. 每个 endpoint 生成一个 HandlerSchema
3. 存为配置文件（`x-ui/apis/imported-{name}.yaml`）
4. LLM 下次对话即可感知

## 5. LLM 集成设计

### 5.1 每次调用的 Prompt 结构

```
[System] — 一次性，随 x-ui 启动加载
────────────────────────────────────
你是一个工作流编排助手。以下是 x.engine 的工作流定义格式：
{format_spec}

可用的 Block 类型: step, parallel, race, loop, for_each, router, try_catch, ...

以下是所有可用的 Handler:
{all_handler_schemas_json}

[User] — 每次对话动态构建
────────────────────────────────────
当前序列定义:
{current_sequence_json}

最近一次调试上下文:
{debug_context_json}

用户需求: {user_prompt}

请返回修改后的完整序列 JSON。
```

### 5.2 Handler Schema 注入策略

当 handler 数量增多（几十上百个），不能全部塞进 context 窗口。需要分级注入：

| 级别 | 内容 | 注入时机 |
|---|---|---|
| **L1 核心** | 20+ 内置 handler + 基础 block 类型 | 每次请求 |
| **L2 常用** | 用户高频使用的自定义 API (top-30) | 每次请求 |
| **L3 全部** | 所有已注册 handler 的摘要列表 | 每次请求底部（仅名称+描述） |
| **L4 按需** | 用户明确提到的 handler 完整 schema | 检测到关键词时注入 |

```json
// L3 摘要格式
{
  "available_handlers": [
    { "name": "send_email", "description": "发送邮件通知", "tags": ["communication"] },
    { "name": "http_request", "description": "发起 HTTP 请求", "tags": ["core"] },
    { "name": "user_service.get_user", "description": "查询用户信息", "tags": ["api"] },
    ...
  ]
}
```

### 5.3 LLM 返回格式

```json
{
  "explanation": "我做了以下修改...",
  "changes": [
    { "op": "replace", "path": "/blocks/2/params/url", "value": "..." },
    { "op": "add", "path": "/blocks/-", "value": { "type": "step", ... } }
  ],
  "full_sequence": { ... }
}
```

## 6. 模块结构

```
x-ui/
├── Cargo.toml
├── src/
│   ├── lib.rs                        ← AppState + Router
│   ├── llm/
│   │   ├── mod.rs                    ← LLM Gateway (多后端路由)
│   │   ├── context_builder.rs        ← 构建 LLM 上下文
│   │   ├── schema_registry.rs        ← HandlerSchema 注册 + 序列化
│   │   └── prompt_templates.rs       ← 系统提示词模板
│   ├── sessions/
│   │   ├── mod.rs                    ← 对话 session CRUD
│   │   └── store.rs                  ← Session 存储 (内存/Redis)
│   ├── debug/
│   │   ├── mod.rs                    ← 调试会话管理
│   │   ├── sse_hub.rs                ← SSE 事件分发中心
│   │   └── instance_proxy.rs         ← 通过 x-api 创建/控制实例
│   ├── apis/
│   │   ├── mod.rs                    ← API 端点管理
│   │   ├── openapi_importer.rs       ← OpenAPI 规范解析导入
│   │   └── registry.rs               ← 从配置目录加载自定义 API
│   ├── routes/
│   │   ├── mod.rs                    ← 路由汇总
│   │   ├── llm.rs                    ← POST /api/ui/llm/chat (SSE 流式)
│   │   ├── sessions.rs               ← CRUD 对话 session
│   │   ├── debug.rs                  ← 调试会话控制
│   │   ├── sequences.rs              ← 序列预览/对比/diff
│   │   └── apis.rs                   ← 自定义 API 端点管理
│   └── frontend/                     ← React + TypeScript (打包后静态文件)
│       ├── package.json
│       ├── src/
│       │   ├── App.tsx
│       │   ├── components/
│       │   │   ├── Canvas.tsx        ← React Flow 画布
│       │   │   ├── LLMPanel.tsx      ← LLM 对话面板
│       │   │   ├── DebugOverlay.tsx  ← 调试浮层
│       │   │   ├── NodeDetail.tsx    ← 节点详情面板
│       │   │   └── ...
│       │   ├── hooks/
│       │   │   ├── useSSE.ts         ← SSE 连接管理
│       │   │   └── useSequence.ts    ← 序列状态管理
│       │   └── types/
│       │       └── schema.ts         ← TypeScript 类型定义
│       └── ...
├── apis/                             ← 自定义 API 配置目录
│   └── README.md
└── config/
    └── llm.toml                      ← LLM 后端配置
```

## 7. 依赖关系

```
x-ui 与 workspace 的关系：
  - 依赖: x-types (序列/实例类型的序列化), x-storage (可选, 直接读 DB)
  - 通过 HTTP 调用: x-api (所有数据操作走 REST API)
  - 不依赖: x-engine (不直接链接引擎代码)
```

```
Cargo.toml 依赖:
  x-types / x-storage (类型)
  reqwest (调用 x-api)
  tokio + axum (后端框架)
  serde / serde_json / serde_yaml
  async-openapi (OpenAPI 解析)
```

## 8. 实施路线

| 阶段 | 内容 | 前置条件 | 工作量 |
|---|---|---|---|
| **P1** | HandlerSchema 注册机制 + x-ui 基础框架 | 无 | 1 周 |
| **P2** | LLM 对话生成序列（JSON 显示 + 应用） | P1 | 1 周 |
| **P3** | 序列画布渲染 + 手动编辑 | P2 | 2 周 |
| **P4** | 实时调试 SSE 流 | 需要 x-api 增加 SSE event 端点 | 1.5 周 |
| **P5** | OpenAPI 导入 + 自定义 API 注册 | P1 | 1 周 |
| **P6** | 实时 Prompt 调整 | P4 | 2 周 |

**总计工作量**: ~8-10 周（单人）

## 9. 与 TODO.md 的关系

参考 `TODO.md` 中的外部 Worker HTTP API（P0）和架构文档（已完成），建议 x-ui 的启动时机：

1. **先完成**: 外部 Worker HTTP API（让引擎能力完整）
2. **开始 x-ui**: P1 和 P2 可并行启动（HandlerSchema 可以在 x-engine 中先行实现）
3. **画布依赖**: P3 需要 x-api 的序列 CRUD 稳定（当前已完备）
4. **调试依赖**: P4 需要 x-api 新增 SSE event stream 端点（扩展现有 `streaming.rs`）

---

> 文档版本: v1.0 · 最后更新: 2025-07-16
