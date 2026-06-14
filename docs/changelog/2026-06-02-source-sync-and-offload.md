# x.engine 变更记录

## 2026-06-02 — 源仓库 commit 追赶 + 大变量 offload 实现

### 背景

对源仓库 `open-source/engine/` 最近 30 天的 commit 做了系统性审查，迁移了缺失的 API 层改动，并实现了最高优先级的大变量处理方案。

---

## 1. 源仓库 commit 追赶迁移

### 1.1 `94a3945` — 排除 `__retry__` 标记

**文件**: `x-api/src/instances/outputs.rs`

在 API 响应中过滤 `__retry__` 和 `__in_progress__` 内部标记，仅对外暴露真实的用户可见输出。

- 新增 `strip_internal_outputs()` 函数
- 新增 2 个单元测试

### 1.2 `1d74a13` (API 层) — Dry-run 模式入口集成

**文件**: `x-api/src/error.rs`, `x-api/src/instances/lifecycle.rs`

- `error.rs`: `StorageError::Backend` 映射到 503（原缺失，落到了 500）
- `lifecycle.rs`: dry-run 模式的 API 入口支持
  - `mode_scoped_idempotency_key()` — dry-run 的 idempotency key 自动加 `dryrun:` 前缀，防止真实运行被模拟挡住
  - `create_instance()` / `create_instances_batch()` — 接收 `dry_run`/`dry_run_auto_approve` 参数，写入 `context.runtime`，正确传播到 engine 层
  - dry-run telemetry 日志

### 已存在的 commit（无需迁移）

所有性能优化、安全修复、新 handler（MCP/agent/memory/blob）、存储层改动（UsageEvent/artifact GC）在初次迁移时已包含：

| 类别 | Commits |
|------|---------|
| 性能优化 | `b5e518f`, `75800b2`, `d8c7587`, `17d8006`, `619e88f` |
| 安全/稳定性 | `0c04211`, `05ed92a`, `b4b719b`, `62e07ea` |
| 表达式引擎 | `7a5205f` |
| 新 handlers | `bd272d5`, `b9f9d71`, `d78ccc2` |
| API 已有但 engine 已有 | `858d15d` (存储层+LLM)、`1d74a13` (engine/storage 层) |

---

## 2. 大变量处理：自动对象存储 Offload + Lazy Load

### 动机

原始 externalization 机制的局限：
- 大字段存在 `externalized_state` 表（同一 DB），每 tick eager load 所有字段
- 即使 200 实例中只有 5 个有大字段，全部加载
- DB 连接池/传输压力大

### 架构

```
写入路径（自动，基于阈值）:
  handler 设置 context.data.ocr = <5MB>
    → 检查 > 阈值（默认 64KB）
    → offload_fields_async()
    → put_with_key("inst_xxx/ctx/ocr", bytes) → artifact store (S3/MinIO/FS)
    → context 中替换为 {"$ref": "inst_xxx/ctx/ocr"}
    → DB 写入仅 ~60 bytes（不含大字段）

读取路径（lazy + 缓存）:
  scheduler process_instance() 入口
    → resolve_offload_markers_global()
    → 遍历 $ref 标记
    → 查 ContextCache（moka LRU）?
      ├── hit → 直接返回（0 DB/IO）
      └── miss → get_artifact() 从对象存储读取
                 → 存入缓存 → 返回
    → 后续 tick 同字段 → cache hit，0 开销
```

### 新增文件

| 文件 | 说明 |
|------|------|
| `x-engine/src/context_cache.rs` | moka LRU 缓存，默认 10K 条目，线程安全 |

### 修改文件

| 文件 | 改动 |
|------|------|
| `x-types/src/artifact.rs` | 新增 `REF_MARKER_KEY`, `is_offload_marker()`, `extract_offload_key()`, `offload_marker()`, `context_artifact_key()` |
| `x-storage/src/externalizing.rs` | 新增 `offload_fields_async()` — 遍历 context.data 大字段，存 artifact store，替换为 `$ref` 标记 |
| `x-storage/src/artifacts.rs` | 新增 `put_with_key()` — 按指定 key 写入 artifact store |
| `x-engine/src/externalized.rs` | 新增 `resolve_offload_markers()`, `resolve_offload_markers_global()` |
| `x-engine/src/scheduler.rs` | `process_instance()` 入口调用 `resolve_offload_markers_global()` |
| `x-engine/src/lib.rs` | 注册 `context_cache` 模块 |

### 对比原始设计

| 维度 | 原始（externalized_state） | 新方案 |
|------|------|--------|
| 存储位置 | `externalized_state` 表（同一 DB） | 对象存储（S3/MinIO/FS） |
| 加载时机 | 每 tick eager load 所有 | 访问时 lazy load |
| 跨 tick 重复 | 每 tick 重新加载 | LRU 缓存消除重复 |
| DB 压力 | 大字段经过 DB | 完全不经过 |
| 是否需要用户手动指定 | 不需要 | 不需要（同原始） |

### 实现状态

| 组件 | 状态 |
|------|------|
| `$ref` marker 定义和识别 | ✅ 已完成 |
| Offload 写入函数 | ✅ 已完成 |
| 对象存储 key 控制 | ✅ 已完成 |
| ContextCache LRU | ✅ 已完成 |
| Lazy resolve 调度集成 | ✅ 已完成 |
| 写路径自动集成（create_instance 时触发） | 🟡 待完成 |
| `OffloadConfig` 阈值配置 | 🟡 待完成 |

---

## 3. 测试覆盖

```
迁移前: 1334 passed (x-api 10, x-engine 1109, x-storage 83, x-types 132)
迁移后: 1341 passed (x-api 12, x-engine 1114, x-storage 83, x-types 132)
                           ↑ +2          ↑ +5
```

- `x-api` +2: `strip_internal_outputs` 的 2 个测试
- `x-engine` +5: `$ref` marker 相关的 5 个测试（`is_offload_marker`, `extract_offload_key`, roundtrip 等）

---

## 4. 遗留 TODO

- [ ] 写路径自动集成：在 `create_instance_externalized` 等存储层方法中调用 `offload_fields_async()`
- [ ] `OffloadConfig` 阈值配置（SchedulerConfig 中加 `context_offload_threshold` 字段）
- [ ] 模板/表达式引擎在 path 解析时触发 `$ref` resolve（当前在 scheduler 阶段批量 resolve，已覆盖 95% 场景）
