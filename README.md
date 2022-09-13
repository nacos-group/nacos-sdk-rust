# nacos-sdk-rust
Nacos client in Rust

## 开发说明
- Build with `cargo build`
> Note: The proto buf client generation is built into cargo build process so updating the proto files under proto/ is enough to update the proto buf client.

- 请 `cargo fmt` 格式化代码再提交

- Rust 入门，还有太多东西不规范，仍需斟酌各种实现逻辑
- 测试用例暂未能实现自动化，开发过程需本地启动 [nacos server](https://github.com/alibaba/nacos) `-Dnacos.standalone=true`

### 主要依赖包
在 nacos-sdk-rust 工程里，为主要功能的实现，将会引入以下依赖包。

- serde-rs/serde 一个超高性能的通用序列化/反序列化框架，可以跟多种协议的库联合使用，实现统一编解码格式；serde-rs/json 快到上天的 JSON 库，也是 Rust 事实上的标准 JSON
- tikv/grpc-rs  一个 Rust 版的 gRPC 客户端和服务器端
- tokio-rs/prost tokio 出品的 Protocol Buffers 工具，简单易用，文档详细
- tokio-rs/tokio 最火的异步网络库，除了复杂上手难度高一些外，没有其它大的问题。同时 tokio 团队提供了多个非常优秀的 Rust 库，整个生态欣欣向荣，用户认可度很高
- tokio-rs/tracing 强大的日志框架，同时还支持 OpenTelemetry 格式，无缝打通未来的监控

*Tip：Rust 入门推荐 [Rust语言圣经(Rust Course)](https://course.rs/about-book.html)*

### 简要描述 client & server 的交互

请关注 `proto/nacos_grpc_service.proto` 并知晓构建出客户端侧的 stub，实现同步调用 `service Request.request()`，流式交互 `service BiRequestStream.requestBiStream()`。

`tikv/grpc-rs` 创建与 Nacos-server 的 gRPC 双工长链接，`serde/json` 适配与 server 的交互序列化；

gRPC 交互的 Payload 和 Metadata 由 `Protocol Buffers` 序列化，具体的 Request/Response 实体 json 格式二进制数据维护于 Payload.body，类型名字符串维护于 Metadata.type (为 java 全类名)。

有了 gRPC 双工长链接，也有了数据序列化方式，那么就是对 Request/Response 的处理逻辑啦；
而 client 会接受 server 的主动调用，故可以实现一个通用的 RequestHandler 接受 server 的请求，根据 Request 类型分发到具体的处理实现并返回对应的 Response。

而 client 请求 server 的部分，则 do it ...

以上交互务必参考 java nacos-client 和 nacos-server 的实现。

### Config 配置交互
- [x] 构建客户端，api 待规范
- [x] 获取配置，api 待规范
- [ ] 监听配置，api 待规范，待实现完整逻辑
- [ ] 监听配置 List-Watch 机制，有一个 list 兜底逻辑
- [ ] 配置 Filter，提供配置解密默认实现；配置获取后，内存缓存，磁盘缓存均是原文，仅返回到用户时经过配置 Filter

### Naming 注册交互
- [ ] 构建客户端 api 与实现
- [ ] 注册服务 api 与实现
- [ ] 反注册服务 api 与实现
- [ ] 批量注册服务 api 与实现
- [ ] 获取服务 api 与实现
- [ ] 订阅服务 api 与实现
- [ ] 订阅服务 List-Watch 机制，有一个 list 兜底逻辑
- [ ] 服务防推空，默认开启，可选关闭。

### Common 通用能力
- [ ] 创建参数，自定义传参 + ENV 环境变量读取，后者优先级高；ENV 统一前缀，例如 `NACOS_CLIENT_CONFIG_*` `NACOS_CLIENT_NAMING_*`
- [x] 请求交互，Request/Response 通用 grpc 逻辑
- [ ] 通用客户端，囊括多线程操作，数据跨线程通知，受益于 Config/Naming
- [ ] 账密登陆，accessToken
- [x] 通用日志，`tracing::info!()`
- [ ] Monitor，`opentelemetry`
- [ ] 内存缓存
- [ ] 数据落盘
- [ ] 磁盘加载（用于服务端宕机弱依赖），优先级放低，已云原生容器化是否必要考虑？

# License
[Apache License Version 2.0](LICENSE)
