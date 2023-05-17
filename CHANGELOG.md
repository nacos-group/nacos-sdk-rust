# 变更日志 | Change log

### 0.3.0
- Refactor: tonic instead of tikv/grpc-rs
- Change: Break Change api of auth plugin, support async fn
- Change: Break Change api of config-filter plugin, support async fn
- Change: Break Change api of config-encryption plugin, support async fn
- TODO

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

