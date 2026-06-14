# 大变量处理：自动对象存储 Offload + Lazy Load

## 动机

### 问题

工作流中有大 JSON 变量（如 PDF OCR 结果数 MB），需在多个 step 间传递。

原始 externalization 机制存在两个问题：

1. **256 KiB 硬上限**：`ExecutionContext` 序列化后不能超过 `DEFAULT_MAX_CONTEXT_BYTES`，否则返回 `ContextTooLarge` 错误
2. **每 tick eager load**：即使外部化到 `externalized_state` 表，每 tick 仍通过 `preload_externalized_markers()` 把所有 externalized 字段批量回填到内存。200 个实例中 5 个有大字段，全部加载，DB 连接池压力大

### 目标

- 大字段**自动**存入对象存储（S3 / MinIO / 本地文件系统），不经过关系型数据库
- context 中只保留轻量引用（~60 bytes）
- **访问时才加载**（lazy），不做每 tick 的预加载
- **内存 LRU 缓存**消除跨 tick 的重复读取

---

## 架构

```
写入路径（自动，基于阈值）:

  handler 设置 context.data.ocr = <5MB>
    │
    ▼
  检查 > threshold（context_offload_threshold）?
    │
    ├── 否 → 正常写入 task_instances.context（与原来一样）
    │
    └── 是 → offload_fields_async()
               ├── serde_json::to_vec(ocr)
               ├── storage.put_artifact_with_key("inst/ctx/ocr", json_bytes)
               │    └── object store (S3/MinIO/local FS)
               ├── context 中替换为 {"$ref": "inst/ctx/ocr"}
               ├── invalidate_cache_key("inst/ctx/ocr")
               └── update_instance_context() 写入 DB（仅 ~60 bytes）


读取路径（lazy + 缓存）:

  scheduler process_instance() 入口
    │
    ▼
  resolve_offload_markers_global()
    │
    ├── 遍历 context.data 顶层 key
    │    │
    │    ├── 值是 {"$ref": "key"}？
    │    │   ├── 否 → 普通字段，跳过
    │    │   └── 是 → 查 ContextCache（moka LRU）
    │    │       ├── hit → 直接从缓存返回
    │    │       └── miss → storage.get_artifact(key)
    │    │                   ├── 从对象存储读取
    │    │                   ├── 解析 JSON → 存入 ContextCache
    │    │                   └── 返回
    │    │
    │    └── 后续 tick 访问同字段 → cache hit，0 开销
    │
    └── 替换 $ref 标记为真实值，handler 无感知


缓存失效:

  offload_fields_async() 返回后
    │
    ├── 遍历刚写入的 $ref 标记
    ├── 逐个 invalidate_cache_key()
    └── 下一 tick 从 artifact store 读取新值，而非旧缓存
```

### marker 形状

```json
{"$ref": "inst_xxx/ctx/ocr_result"}
```

- 单 key JSON 对象，key 为 `$ref`，value 为 artifact key
- artifact key 格式：`{instance_uuid}/ctx/{field_name}`
- 与 legacy `_externalized` 标记（`{"_externalized": true, "_ref": "..."}`）**形状不同，不会误判**

---

## 实现细节

### 新增文件

| 文件 | 说明 |
|------|------|
| `x-engine/src/context_cache.rs` | moka LRU 缓存，默认 10K 条目，线程安全，按 artifact key 索引 |

### 修改文件

| 文件 | 改动 |
|------|------|
| `x-types/src/artifact.rs` | 新增 `REF_MARKER_KEY`, `is_offload_marker()`, `extract_offload_key()`, `offload_marker()`, `context_artifact_key()` |
| `x-types/src/config.rs` | `SchedulerConfig` 新增 `context_offload_threshold: u32`（默认 0 = 禁用） |
| `x-storage/src/lib.rs` | `ResourceStore` 新增 `put_artifact_with_key(&self, key, content_type, bytes)` trait 方法 |
| `x-storage/src/sqlite/mod.rs` | SQLite 实现 `put_artifact_with_key()` → `ObjectArtifactStore::put_with_key()` |
| `x-storage/src/postgres/mod.rs` | Postgres 实现 `put_artifact_with_key()` → `ObjectArtifactStore::put_with_key()` |
| `x-storage/src/artifacts.rs` | 新增 `put_with_key(&self, key, content_type, bytes)` — 按指定 key 写入 |
| `x-storage/src/externalizing.rs` | 新增 `offload_fields_async()` — 遍历 context.data，超过 threshold 的字段存入 artifact store，替换为 `$ref` 标记 |
| `x-engine/src/externalized.rs` | 新增 `resolve_offload_markers()` / `resolve_offload_markers_global()` / `invalidate_cache_key()` |
| `x-engine/src/scheduler.rs` | ① `TickContext` + `process_instance()` 参数穿透 `context_offload_threshold` ② `process_instance()` 入口调用 `resolve_offload_markers_global()` ③ step 完成后、`update_instance_context` 前调用 `offload_fields_async()` + `invalidate_cache_key()` ④ `fresh_context` 加载后防御性 resolve `$ref` 标记 |
| `x-engine/src/lib.rs` | 注册 `context_cache` 模块 |

### 关键函数签名

#### 写入

```rust
// x-storage/src/externalizing.rs
pub async fn offload_fields_async<F, Fut>(
    data: &mut Value,                    // context.data
    instance_id: &uuid::Uuid,            // 实例 UUID
    threshold_bytes: u32,                // 阈值，0 表示不 offload
    put_artifact: F,                     // 闭包：接收 (key, bytes) -> ArtifactRef
) -> Result<u32, StorageError>           // 返回 offloaded 字段数
where
    F: FnMut(String, Vec<u8>) -> Fut,
    Fut: Future<Output = Result<ArtifactRef, StorageError>>;
```

#### 读取

```rust
// x-engine/src/externalized.rs
pub async fn resolve_offload_markers(
    storage: &dyn StorageBackend,
    cache: &ContextCache,
    context: &mut ExecutionContext,
);

pub async fn resolve_offload_markers_global(
    storage: &dyn StorageBackend,
    context: &mut ExecutionContext,
);

pub fn invalidate_cache_key(key: &str);
```

---

## 与原始设计的对比

| 维度 | 原始（externalized_state 表） | 新方案（对象存储 offload） |
|------|------|--------|
| **大字段存储位置** | `externalized_state` 表（同一 PostgreSQL/SQLite） | **对象存储**（S3 / MinIO / 本地 FS） |
| **加载时机** | 每 tick **eager** load 所有 instance 的所有 externalized 字段 | 访问时 **lazy** load |
| **跨 tick 重复** | 每 tick 重新从 DB 加载 | **LRU 缓存**消除重复 |
| **DB 压力** | 🟡 大字段序列化后仍经过 DB 连接 / 网络 / 序列化 | 🟢 **完全不经过** |
| **marker 形状** | `{"_externalized": true, "_ref": "inst:ctx:data:field"}` | `{"$ref": "inst/ctx/field"}` |
| **是否需要手动指定** | ❌ 不需要（自动基于阈值） | ❌ **不需要**（同原始） |
| **handler 感知** | ✅ 无感知（preload 透明回填） | ✅ **无感知**（scheduler 入口透明解析） |
| **首次读取延迟** | 🟢 低（同 DB） | 🟡 ~50-100ms（S3 延迟），缓存后消除 |
| **两种机制共存** | 🟢 — | ✅ `externalize_fields()` 跳过 `$ref`，`offload_fields_async()` 跳过 `_externalized`，不冲突 |

---

## 配置方式

```toml
[engine]
# 64KB 以上字段自动 offload 到 artifact store
context_offload_threshold = 65536

# 也可以同时保留传统的 DB externalization（两个阈值独立工作）
externalize_output_threshold = 4096
```

```rust
// 默认值为 0 = offload 禁用。设为 64KB 以上即启用。
SchedulerConfig {
    context_offload_threshold: 65536,
    ..
}
```

---

## 设计评价

### 🟢 做得好的

| 维度 | 说明 |
|------|------|
| **写入函数解耦** | `offload_fields_async()` 接受 `FnMut` 闭包而非直接引用 `StorageBackend`，`x-storage` 层无需感知 engine trait |
| **两种机制不冲突** | `externalize_fields()` 跳过 `$ref`，`offload_fields_async()` 跳过 `_externalized`，共存安全 |
| **缓存失效** | offload 写入后立即 `invalidate_cache_key()`，下一 tick 不会读到脏数据 |
| **防御性 resolve** | `fresh_context` 从 DB 重新加载后、offload 前调用 `resolve_offload_markers_global()`，确保大小检查看到的是真实值而非 `$ref` 标记 |
| **配置优先级** | 默认 0 = 禁用，存量用户零影响。需要时按需开启 |

### 🟡 已知限制

| 限制 | 说明 | 影响 |
|------|------|------|
| **tree 路径未集成** | `process_instance_tree()`（复合 block）中不会触发 offload，大字段正常走 `update_instance_context()` 存 DB | 低 — 功能正确，只是享受不到 offload 好处 |
| **逐个 fetch** | 一个实例如果有 N 个 `$ref` 字段，会产生 N 次 `get_artifact` 调用 | 低 — 大多数场景 1-2 个字段 |
| **Cache 无 TTL** | `ContextCache` 只有 LRU 容量限制，无时间驱逐 | 极低 — cache 是读加速，不是正确性边界 |

### 🔴 改进项（已修复）

| 问题 | 发现时间 | 修复 |
|------|---------|------|
| `fresh_context` 可能含有上一 tick 残留的 `$ref` 标记，输入 `offload_fields_async()` 时因标记形状被跳过，导致该字段无法重新 offload | 代码审查 | 在 `fresh_context` 加载后加 `resolve_offload_markers_global()`，确保看到真实值 |

### 🔴 待改进项

| 项目 | 难度 | 说明 |
|------|------|------|
| **集成测试** | 🟡 中 | 缺少 offload → artifact store → next tick resolve 全链路测试 |
| **Tree 路径集成** | 🟢 低 | 在 `process_instance_tree()` 的 `evaluate()` 返回后、`update_instance_context` 前加 `offload_fields_async()` |
| **API 层 offload** | 🟢 低 | `POST /api/v1/instances` 创建时如果传入大 field 也可 offload（初始 context 通常很小，优先级低） |

---

## 验证

- `cargo build --workspace` — ✅ **0 errors**
- `cargo test --workspace --lib` — ✅ **1341 passed**（x-api 12, x-engine 1114, x-storage 83, x-types 132）
