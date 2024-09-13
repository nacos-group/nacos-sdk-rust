# 变更日志 | Change log

### 0.4.2

- 功能: 提供 Aliyun ram AuthPlugin，通过 `features = ["auth-plugin-http"]` 开启

---

- Feature: Support Aliyun ram AuthPlugin (#245), enabled with `features = ["auth-plugin-http"]`

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

