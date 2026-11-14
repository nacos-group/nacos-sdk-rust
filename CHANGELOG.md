# 变更日志 | Change log

### Unreleased

- 破坏性变更：要求 Nacos Server >= 2.3.0；连接建立后必须完成能力协商（`SetupAckRequest`），旧版本服务端在普通 `build()` 中返回 `UnsupportedServerVersion`（`emergency_start` 仅记录错误并持续重试）
- 破坏性变更：连接层重写为独立监督任务；`build()` 在 3 次连接尝试仍失败时返回错误（`emergency_start` 仍立即返回并后台重连）；普通模式在服务端不支持能力协商时立即失败
- 特性：连接握手支持能力表协商，记录服务端能力；新增指数退避 + 随机偏移（base 500ms、cap 30s、稳定 10s 后重置）
- 变更：集成测试默认使用 `nacos/nacos-server:v2.5.4`（podman/docker 自动探测）；rnacos 暂不支持（未实现能力协商）
- 变更：就绪前发起的请求会等待连接就绪，并与响应共享 10 秒预算；断连时在途请求立即失败，断开瞬间已排队的请求被拒绝（不再无限排队）（用户可见行为变化）
- 变更：普通 `build()` 每次失败尝试都要消耗服务端检查耗时（共 3 次尝试），因此连接失败不是瞬时返回
- 破坏性变更：API 入口的必填字符串参数统一改为 `impl Into<String>`（如 `data_id`、`group`、`content`、`cas_md5`、`beta_ips`、`service_name`）；`ConfigResponse` 与 `ServiceInstance` 的 getter 改为返回 `&str` / `Option<&str>`
- 破坏性变更：`get_all_instances` / `select_instances` 返回 `Arc<Vec<ServiceInstance>>`，`select_one_healthy_instance` 返回 `Arc<ServiceInstance>`，`NamingChangeEvent.instances` 改为 `Option<Arc<Vec<ServiceInstance>>>`
- 增强：上述入口可直接传入 `&str` / `&String`，无需手动 `to_string()`
- 破坏性变更：`AuthPlugin::get_login_identity` 改为返回 `Arc<LoginIdentityContext>`，HTTP token 认证直接共享已登录身份，不再克隆整份身份 map（header 条目仍会复制进每次请求；该 trait 文档已声明后续可能变更）
- 修复：`ConfigFilter` 改由 worker 在出口统一应用，不再随每条缓存数据保存；从磁盘恢复的配置缓存（`config_load_cache_at_start`）也会继续经过 filter（如解密）
- 修复：naming redo 调度不再在自动重试请求执行期间持有读锁，重试进行中注册/注销不再被阻塞
- 修复：配置响应缺少 content/content_type/md5 字段时，拉取路径返回错误、推送路径告警并跳过通知，不再 panic 导致通知任务退出
- 修复：服务端返回 300（配置不存在）时清空本地缓存中的旧内容与 md5，并以空 content、空 md5 通知此前订阅过该配置的 listener；避免 list-watch 每 60 秒重复拉取已删除配置、持续打印 `ConfigNotFound`，同时不再继续返回已删除配置的旧内容
- 性能：服务实例变更日志仅在日志启用时才序列化实例数据
- 性能：未配置 `ConfigFilter` 时 config 缓存命中不再复制配置内容，naming 缓存命中的实例列表改为共享，选择单个实例时只克隆最终选中的实例
- 性能：缓存序列化移出调用线程，并且只在写守卫被实际修改后才落盘；注册/注销 listener 不再重写磁盘缓存
- 性能：config/naming 用户监听回调改由阻塞线程池执行，避免同步回调阻塞 SDK 运行时
- 增强：gRPC 请求默认 10 秒响应头超时（覆盖连接建立与等待响应头，不限制双向流生命周期；暂不支持自定义）
- 破坏性变更：移除已废弃的 `ClientProps::max_retries` 与 `Error::MaxRetriesExceeded`

---

- Breaking: Nacos Server >= 2.3.0 is required; connections must complete ability negotiation (`SetupAckRequest`), and older servers fail with `UnsupportedServerVersion` in a normal `build()` (`emergency_start` logs the error and keeps retrying)
- Breaking: the connection layer is now an independent supervisor task; `build()` fails after 3 connection attempts (unless `emergency_start`), or immediately in normal mode when the server reports no ability negotiation
- Feature: ability-table negotiation during setup with server abilities recorded; exponential backoff with jitter (base 500ms, cap 30s, reset after 10s stable)
- Change: integration tests default to `nacos/nacos-server:v2.5.4` (podman/docker auto-detected); rnacos is unsupported until it implements ability negotiation
- Change: requests issued before readiness wait for the connection within the same 10-second budget; in-flight requests fail immediately on disconnect, and requests already queued when the connection drops are rejected (no unbounded queue) (user-visible behavior change)
- Change: an ordinary `build()` spends server-check time on every failed attempt (3 attempts total), so a failed build is not instantaneous
- Change: required string parameters of API entries now accept `impl Into<String>` (e.g. `data_id`, `group`, `content`, `cas_md5`, `beta_ips`, `service_name`); `ConfigResponse` and `ServiceInstance` getters now return `&str` / `Option<&str>`
- Change: `get_all_instances` / `select_instances` return `Arc<Vec<ServiceInstance>>`, `select_one_healthy_instance` returns `Arc<ServiceInstance>`, and `NamingChangeEvent.instances` is now `Option<Arc<Vec<ServiceInstance>>>`
- Enhance: those entries accept `&str` / `&String` directly, no manual `to_string()`
- Change: `AuthPlugin::get_login_identity` now returns `Arc<LoginIdentityContext>`, so HTTP token auth shares the logged-in identity instead of cloning the whole identity map (entries are still copied into each request; the trait is documented as subject to change)
- Fix: `ConfigFilter` is now applied by the worker at the response exits instead of being stored with each cache entry; config cache entries restored from disk (`config_load_cache_at_start`) still run filters such as decryption
- Fix: the naming redo scheduler no longer holds a read lock while automatic retry requests run, so registrations and deregistrations are not blocked during a retry
- Fix: config responses missing content/content_type/md5 now make the fetch path return an error and the push path warn and skip notification, instead of panicking and killing the notify task
- Fix: a server-side 300 (config not found) now clears the stale cached content and md5 and notifies listeners that had previously observed the config, using an empty content and md5; the list-watch loop no longer re-fetches deleted configs every 60 seconds (no more repeated `ConfigNotFound` errors) and deleted configs are no longer served from the cache
- Performance: service-instance diff logs serialize instance data only when logging is enabled
- Performance: without a configured `ConfigFilter`, config cache hits no longer copy the config content; naming cache hits share the instance list, and single-instance selection clones only the chosen instance
- Performance: cache serialization runs off the caller thread and only writes after a write guard is actually mutated; listener add/remove no longer rewrites the disk cache
- Performance: config/naming user listener callbacks run on the blocking pool so synchronous callbacks cannot stall the SDK runtime
- Enhance: gRPC requests now have a default 10-second response-header timeout (covers connection establishment and response headers, not the bi-stream lifetime; not user-configurable yet)
- Change: remove the deprecated `ClientProps::max_retries` and `Error::MaxRetriesExceeded`

### 0.8.4

- 特性：支持关闭磁盘缓存而仅使用内存缓存；新增 `CacheKind` 枚举，通过设置 `ClientProps::cache_kind(CacheKind::None)` 或环境变量 `NACOS_CLIENT_CACHE_KIND=none` (#313)

---

- Feature: Support disabling disk store and only using memory cache. add `CacheKind` enum, allow disabling the disk store via `ClientProps::cache_kind(CacheKind::None)` or env `NACOS_CLIENT_CACHE_KIND=none` for memory-only cache (#313)

### 0.8.3

- 修复：修复配置变更通知流程中跨 await 持有 DashMap guard 的问题 (#310)

---

- Fix: avoid holding DashMap guard across await in notify flow (#310)

### 0.8.2

- 修复：健康检查请求添加超时，避免半死连接上挂起 (#306)
- 修复：修正 http2 keepalive 超时设置并默认开启 http2 keepalive (#307)

---

- Fix: add timeout to health check request, avoid hang on half-dead connection (#306)
- Fix: correct http2 keepalive timeout setter and enable http2 keepalive by default (#307)

### 0.8.1

- 特性：缓存目录支持环境变量配置和自定义设置
- 增强：依赖特性最小化、服务器列表日志和 clippy 修复

---

- Feature: cache directory supports environment variable configuration and custom settings
- Chore: minimize dependency features, server list logging & clippy fixes

### 0.8.0

- 破坏性变更：自定义 `AuthPlugin` 需要适配 `login` 方法签名（参数改为 `Arc` 包装）；`remote_grpc_port` 参数类型 `u32` → `u16`
- 特性：新增 `endpoint` 配置，支持从远程服务端点动态获取服务器列表，`endpoint` 优先级高于 `server_addr`
- 增强：CI 添加 sccache 编译缓存、`thiserror` 升级至 2.0、`url` crate 统一处理 endpoint URL

---

- Change: custom `AuthPlugin` needs to adapt `login` method signature (params wrapped in `Arc`); `remote_grpc_port` parameter type `u32` → `u16`
- Feature: add `endpoint` for dynamic server list resolution, `endpoint` takes priority over `server_addr`
- Chore: add sccache compilation caching for CI, upgrade `thiserror` to 2.0, unify `url` crate for endpoint URL

### 0.7.0

- 特性：提供 gRPC TLS 支持，通过 `features = ["tls"]` 开启，支持自定义 CA 证书
- 特性：完善应急启动模式 `load_cache_at_start(true)`，Nacos 服务端不可用时应用仍可启动，优先读取磁盘缓存
- 增强：初始化重试优化（快失败 + 运行时无限重试）、依赖升级（tonic 0.14 / tower 0.5）、新增 zino 框架示例
- CI：新增集成测试框架（rnacos, 39 用例）、双通道 CI（rnacos 快速验证 + docker 完整验证）、覆盖率统计升级
- 修复：对齐 Java 客户端监听器注册键修复 #278 通知延迟、消除全部 clippy 警告、文档修复

---

- Feature: add gRPC TLS support via `features = ["tls"]`, with custom CA certificate support
- Feature: emergency start mode by `load_cache_at_start(true)`, app can start even when Nacos server unavailable, reads from disk cache
- Enhance: init retry optimization (fast-fail + infinite runtime retries), dependency upgrades (tonic 0.14 / tower 0.5), zino framework examples
- CI: add integration test framework (rnacos, 39 tests), dual-channel CI (rnacos fast + docker full validation), coverage statistics upgrade
- Fix: align listener registry key with Java client to fix #278 notification delay, resolve all clippy warnings, documentation fixes


### 0.6.0

- 变更：配置与注册的客户端构建异步初始化，不在内部阻塞，毋必 build 后 await
- 重构：与 AI-Coding 重构很多品味不佳的代码，非常感谢 Claude + Kimi-K2.5
- 特性：提供新特性 features of `tracing-log`，仅当您不自定义  tracing-subscriber 时才需要此项
- 特性：完整实现数据落盘与加载（用于服务端宕机弱依赖），请设置 `load_cache_at_start(true)`

---

- Change: make naming/config service initialization fully async, must build and await
- Refactor: written a lot of tasteful code with AI Coding. Thanks very much to Claude + Kimi-K2.5
- Feature: support features of `tracing-log`, only need this if you don't customize `tracing-subscriber`
- Feature: implement naming/config data persistence and loading, set by `load_cache_at_start(true)`


### 0.5.3

- 增强: tokio 只引入需要的 `features = ["rt-multi-thread", "time", "net", "fs"]`

---

- Chore: do not require unused features of tokio #266


### 0.5.2

- 修复: 兼容 Nacos-Server 3.x 的 ServiceInfo 属性 `allIps`

---

- Fix: Compatible with Nacos Server 3.x ServiceInfo's field `allIps`


### 0.5.1

- 修复: 使用安全的 UTF-8 字符边界处理修复字符串截断

---

- Fix: Fix panic on string truncation by using safe UTF-8 character boundary handling by @wslongchen


### 0.5.0

- 增强: rust-edition=2024, MSRV=1.80
- 增强: ConfigService / NamingService 改为 struct 而无需 Box<dyn> 包裹

---

- Enhance: rust-edition=2024, MSRV=1.80
- Enhance: ConfigService / NamingService changed to struct without Box<dyn> wrapping


### 0.4.3

- 功能: 提供 Aliyun ram AuthPlugin (#245)，通过 `features = ["auth-by-aliyun"]` 开启
- 文档: Add aliyun ram examples and improve doc by @luoxiner in #248
- 增强: use LazyLock replace lazy_static by @CherishCai in #250

---

- Feature: Support Aliyun ram AuthPlugin (#245), enabled with `features = ["auth-by-aliyun"]`
- Doc: Add aliyun ram examples and improve doc by @luoxiner in #248
- Chore: use LazyLock replace lazy_static by @CherishCai in #250


### 0.4.2 Yank

- 功能: 提供 Aliyun ram AuthPlugin，通过 `features = ["auth-by-aliyun"]` 开启

---

- Feature: Support Aliyun ram AuthPlugin (#245), enabled with `features = ["auth-by-aliyun"]`

### 0.4.1

- 优化: 在 `auth-plugin-http` 使用 `arc-swap` 替换 unsafe 代码
- 增强: 可以设置参数 `max_retries` 使内部连接 nacos-server 仅重试一定次数（默认无限重连），否则失败抛出 Error
- 增强: 针对 Config/Naming 的 sdk 调用检查参数
- 升级: 升级 `tonic` and `prost` 版本
- 样例: 增加一个使用 LazyLock 的用例 lazy_app

---

- Opt: `auth-plugin-http` unsafe code replace with `arc-swap` by @thynson in #234
- Enhance: Prop `max_retries` for InnerConnection with nacos-server by @451846939 in #242
- Enhance: Check params for Config/Naming by @CherishCai in #240
- Bump: Upgrade `tonic` and `prost` version by @thynson in #233
- Chore: Add an example lazy_app by @CherishCai in #239

### 0.4.0

- 破坏性变更: 使 NamingService 和 ConfigService impl Send + Sync
- 破坏性变更: 默认 async，去掉 sync api，需要的话建议 `futures::executor::block_on(future_fn)`

---

- Change: make NamingService and ConfigService Send + Sync
- Change: all async API; If you need sync, maybe `futures::executor::block_on(future_fn)`

### 0.3.6

- 文档: 补充说明 `NamingService` 和 `ConfigService` 需要全局的生命周期
- 优化: 调整 `connection health check` 日志级别为 `warn` 

---

- Doc: supplement that `NamingService` and `ConfigService` need a global lifecycle
- Enhance: adjust the log level of `connection health check` to `warn`

### 0.3.5

- 修复: 磁盘加载持久化数据不触发 `Listener` 回调
- 功能: 新增 `naming_load_cache_at_start` 属性，用于控制是否在启动时加载缓存, 默认 `false`

---

- Fix: load service info from disk not trigger `Listener` callback
- Feature: add `naming_load_cache_at_start` property to control whether to load the cache at startup, default `false`

### 0.3.4

- 增强: 当设置 ephemeral=false 时，注册持久化实例

---

- Enhance: register persistent-instance when instance's ephemeral=false

### 0.3.3

- 增强: Nacos client 公共线程池线程数量默认为1并升级一些依赖版本

---

- Enhance: upgrade some dependencies and nacos common thread pool default thread number is 1

### 0.3.2

- 增强：支持环境变量设置部分参数，默认为环境变量优先
- 增强：提供防推空参数设置，默认 true
- 增强：支持 server_addr 不设置端口，默认 8848
- 测试：Integration Test with nacos-server

---

- Enhance: Read props from environment variables, please see `nacos_sdk::api::constants::ENV_NACOS_CLIENT_*`
- Enhance: The `naming_push_empty_protection` could be set by ClientProps
- Enhance: Support `server-addr` without port, default 8848
- Test：Integration Test with nacos-server

### 0.3.1

- Fix：异步登陆未完成，进行其它调用出现未登陆异常 `user not found`

---

- Fix: Asynchronous login not completed, there is an exception to `user not found` in when making other calls.

### 0.3.0

- 重构：gRPC 连接层使用 tonic 替代 tikv/grpc-rs ，让编译构建更舒适
- 破坏性变更：api 插件 auth/config-filter/config-encryption 都改成 async 函数

---

- Refactor: tonic instead of tikv/grpc-rs
- Change: Break Change api of auth plugin, support async fn
- Change: Break Change api of config-filter plugin, support async fn
- Change: Break Change api of config-encryption plugin, support async fn

### 0.2.6

- 修复 `ServiceInfoUpdateTask` 丢失 auth header

---

- fix lose auth headers in ServiceInfoUpdateTask

### 0.2.5

- 优化重连机制

---

- Enhance: optimize reconnect logic

### 0.2.4

- 清理无用代码
- login url 携带账号密码
- 统一使用变量名占位方式打印日志
- 支持 https login 认证
- 支持自定义 grpc 端口
- 实现 List-Watch 机制 naming 模块
- 设置默认 grpc 请求超时时间
- 修复服务端多次推送服务变更信息

---

- Chore: login with url encode username password.
- Chore: clean code with clippy 
- Chore: log macro args into string 
- Feature: add https scheme in feathre for auth and custom grpc port support 
- Feature: implement List-Watch for naming module 
- Enhance: set default timeout 
- Fix: service info push many times from server 

### 0.2.3

- 提供 async api，可以通过 `features = ["async"]` 来启用
- 优化内部逻辑，减少核心线程数目、去除 tls/openssl 依赖
- 变更 naming api `register_instance/select_instances` 用以替代 `register_service/select_instance`
- 修复 naming 服务变更的日志打印

---

- Api: provides the async API, which can be enabled via `features = ['async"]`
- Chore: optimize internal logic, reduce the number of core threads, remove tls/openssl dependencies
- Change: naming api `register_instance/select_instances` instead of `register_service/select_instance`
- Fix: naming changed service log

### 0.2.2

- 修复 cluster_name 无效

---

- fix cluster_name invalid when the service register

### 0.2.1
- 支持设置多服务端地址，形如：`address:port[,address:port],...]`

---

- Support multi server-addr, following format: `address:port[,address:port],...]`

### 0.2.0

- Config/Naming 功能均可用
- 登陆鉴权 username/password
- 配置解密插件
- 底层 grpc 链接健康检测，自动重连

---

- The module of Config and naming are available
- Support Auth Plugin and with props username/password
- Config decryption Plugin
- Core grpc health detection, automatic reconnection

### 0.1.1

- Config 模块基本可用
- 欢迎更多贡献、修复和标准化 api

---

- The module of Config basically available
- Welcome more contributions, fixes and standardized APIs
