# nacos-sdk-rust
Nacos client in Rust

### Extra
> [nacos-sdk-rust-binding-node](https://github.com/opc-source/nacos-sdk-rust-binding-node.git) : nacos-sdk-rust binding for NodeJs with napi.
> 
> Tip: nacos-sdk-nodejs 仓库暂未提供 2.x gRPC 交互模式，为了能升级它，故而通过 node addon 方式调用 nacos-sdk-rust

> [nacos-sdk-rust-binding-py](https://github.com/opc-source/nacos-sdk-rust-binding-py.git) : nacos-sdk-rust binding for Python with PyO3.
>
> Tip: nacos-sdk-python 仓库暂未提供 2.x gRPC 交互模式，为了能升级它，故而通过 ffi 方式调用 nacos-sdk-rust


# Proposal 

https://github.com/alibaba/nacos/issues/8443#issuecomment-1248227587

## Quickstart

### Add Dependency
Add the dependency in `Cargo.toml`:

```toml
[dependencies]
# If you need sync API, maybe `futures::executor::block_on(future_fn)`
nacos-sdk = { version = "0.5", features = ["default"] }
```

### Usage of Config
```rust
    // 请注意！一般情况下，应用下仅需一个 Config 客户端，而且需要长期持有直至应用停止。
    // 因为它内部会初始化与服务端的长链接，后续的数据交互及变更订阅，都是实时地通过长链接告知客户端的。
    let config_service = ConfigServiceBuilder::new(
        ClientProps::new()
            .server_addr("127.0.0.1:8848")
            // Attention! "public" is "", it is recommended to customize the namespace with clear meaning.
            .namespace("")
            .app_name("simple_app"),
            .auth_username("username")
            .auth_password("password")
    )
    .enable_auth_plugin_http()
    .build()
    .await?;

    // example get a config
    let config_resp = config_service.get_config("todo-data-id".to_string(), "todo-group".to_string()).await;
    match config_resp {
        Ok(config_resp) => tracing::info!("get the config {}", config_resp),
        Err(err) => tracing::error!("get the config {:?}", err),
    }

    struct ExampleConfigChangeListener;

    impl ConfigChangeListener for ExampleConfigChangeListener {
        fn notify(&self, config_resp: ConfigResponse) {
            tracing::info!("listen the config={:?}", config_resp);
        }
    }
    
    // example add a listener
    let _listen = config_service.add_listener(
        "todo-data-id".to_string(),
        "todo-group".to_string(),
        Arc::new(ExampleConfigChangeListener {}),
    ).await;
    match _listen {
        Ok(_) => tracing::info!("listening the config success"),
        Err(err) => tracing::error!("listen config error {:?}", err),
    }
```

### Usage of Naming
```rust
    // 请注意！一般情况下，应用下仅需一个 Naming 客户端，而且需要长期持有直至应用停止。
    // 因为它内部会初始化与服务端的长链接，后续的数据交互及变更订阅，都是实时地通过长链接告知客户端的。
    let naming_service = NamingServiceBuilder::new(
        ClientProps::new()
            .server_addr("127.0.0.1:8848")
            // Attention! "public" is "", it is recommended to customize the namespace with clear meaning.
            .namespace("")
            .app_name("simple_app"),
            .auth_username("username")
            .auth_password("password")
    )
    .enable_auth_plugin_http()
    .build()
    .await?;

    pub struct ExampleInstanceChangeListener;

    impl NamingEventListener for ExampleInstanceChangeListener {
        fn event(&self, event: std::sync::Arc<NamingChangeEvent>) {
            tracing::info!("subscriber notify event={:?}", event);
        }
    }

    // example naming subscriber
    let subscriber = Arc::new(ExampleInstanceChangeListener);
    let _subscribe_ret = naming_service.subscribe(
        "test-service".to_string(),
        Some(constants::DEFAULT_GROUP.to_string()),
        Vec::default(),
        subscriber,
    ).await;

    // example naming register instances
    let service_instance1 = ServiceInstance {
        ip: "127.0.0.1".to_string(),
        port: 9090,
        ..Default::default()
    };
    let _register_instance_ret = naming_service.batch_register_instance(
        "test-service".to_string(),
        Some(constants::DEFAULT_GROUP.to_string()),
        vec![service_instance1],
    ).await;
```

### Props
Props count be set by `ClientProps`, or Environment variables (Higher priority).
See them in `nacos_sdk::api::props::ClientProps` or `nacos_sdk::api::constants::ENV_NACOS_CLIENT_*`.
e.g.
- env `NACOS_CLIENT_COMMON_THREAD_CORES` to set nacos-client-thread-pool num, default 1
- env `NACOS_CLIENT_NAMING_PUSH_EMPTY_PROTECTION` for naming empty data notify protection, default true
- env `NACOS_CLIENT_USERNAME` to set http auth username
- env `NACOS_CLIENT_PASSWORD` to set http auth password
- env `NACOS_CLIENT_ACCESS_KEY` to set Aliyun ram access-key
- env `NACOS_CLIENT_SECRET_KEY` to set Aliyun ram access-secret

### AuthPlugin Features
- > Set access-key, access-secret via Environment variables are recommended.
- auth-by-http
  - support http auth via username and password
  - how to use
    - enable auth-by-http(default is enabled)
    ```toml
    [dependencies]
    nacos-sdk = { version = "0.4", features = ["default"] }
    ```
    - Set username and password via environment variables
    ```shell
    export NACOS_CLIENT_USERNAME=you_username
    export NACOS_CLIENT_PASSWORD=you_password
    ```
    - Enable auth-by-http in your code
    ```rust
    ConfigServiceBuilder::new(
     ClientProps::new()
         .server_addr("localhost:8848"))
     .enable_auth_plugin_http()
  
    NamingServiceBuilder::new(
     ClientProps::new()
         .server_addr("localhost:8848"))
     .enable_auth_plugin_http()
     .build()
    ```
- auth-by-aliyun
  - support aliyun ram auth via access-key and access-secret
  - how to use 
    - enable auth-by-aliyun feature in toml
    ```toml
    [dependencies]
    nacos-sdk = { version = "0.4", features = ["default", "auth-by-aliyun"] }
    ```
    - Set accessKey and secretKey via environment variables
    ```shell
    export NACOS_CLIENT_ACCESS_KEY=you_access_key
    export NACOS_CLIENT_SECRET_KEY=you_secret_key
    ```
    - Enable aliyun ram auth plugin in code by enable_auth_plugin_aliyun
    ```rust
    ConfigServiceBuilder::new(
     ClientProps::new()
         .server_addr("localhost:8848"))
     .enable_auth_plugin_aliyun()
  
    NamingServiceBuilder::new(
     ClientProps::new()
         .server_addr("localhost:8848"))
     .enable_auth_plugin_aliyun()
     .build()
    ```

## 开发说明
- Build with `cargo build`
- Test with `cargo test`

- 请 `cargo clippy --all` 根据提示优化代码
> Run `cargo clippy --all` - this will catch common mistakes and improve your Rust code.

- 请 `cargo fmt --all` 格式化代码再提交
> Run `cargo fmt --all` - this will find and fix code formatting issues.

- 测试用例暂未能实现自动化，开发过程需本地启动 [nacos server](https://github.com/alibaba/nacos) `-Dnacos.standalone=true`

### 主要依赖包
在 nacos-sdk-rust 工程里，为主要功能的实现，将会引入以下依赖包。

- serde-rs/serde 一个超高性能的通用序列化/反序列化框架，可以跟多种协议的库联合使用，实现统一编解码格式
- serde-rs/json 快到上天的 JSON 库，也是 Rust 事实上的标准 JSON
- hyperium/tonic 一个 Rust 版的 gRPC 客户端和服务器端
- tokio-rs/prost tokio 出品的 Protocol Buffers 工具，简单易用，文档详细
- tokio-rs/tokio 最火的异步网络库，除了复杂上手难度高一些外，没有其它大的问题。同时 tokio 团队提供了多个非常优秀的 Rust 库，整个生态欣欣向荣，用户认可度很高
- tokio-rs/tracing 强大的日志框架，同时还支持 OpenTelemetry 格式，无缝打通未来的监控

*Tip：Rust 入门推荐 [Rust语言圣经(Rust Course)](https://course.rs/about-book.html)*

### 简要描述 client & server 的交互

请关注 `proto/nacos_grpc_service.proto` 并知晓构建出客户端侧的 stub，实现同步调用 `service Request.request()`，流式交互 `service BiRequestStream.requestBiStream()`。

`hyperium/tonic` 创建与 Nacos-server 的 gRPC 双工长链接，`serde/json` 适配与 server 的交互序列化；

gRPC 交互的 Payload 和 Metadata 由 `Protocol Buffers` 序列化，具体的 Request/Response 实体 json 格式二进制数据维护于 Payload.body，类型名字符串维护于 Metadata.type 。

有了 gRPC 双工长链接，也有了数据序列化方式，那么就是对 Request/Response 的处理逻辑啦；
而 client 会接受 server 的主动调用，故可以实现一个通用的 RequestHandler 接受 server 的请求，根据 Request 类型分发到具体的处理实现并返回对应的 Response。

而 client 请求 server 的部分，则 do it ...

以上交互务必参考 java nacos-client 和 nacos-server 的实现。

#### Config 配置管理模块
- [x] 客户端创建 api
- [x] 发布配置 api 与实现
- [x] 删除配置 api 与实现
- [x] 获取配置 api 与实现
- [x] 监听配置 api 与实现，List-Watch 机制，具备 list 兜底逻辑
- [x] 配置 Filter，提供配置解密默认实现；配置获取后，内存缓存，磁盘缓存均是原文，仅返回到用户时经过配置 Filter

#### Naming 服务注册模块
- [x] 客户端创建 api
- [x] 注册服务 api 与实现
- [x] 反注册服务 api 与实现
- [x] 批量注册服务 api 与实现
- [x] 获取服务 api 与实现
- [x] 订阅服务 api 与实现，List-Watch 机制，具备 list 兜底逻辑
- [x] 服务防推空，默认开启，可选关闭。

#### Common 通用能力
- [x] 创建参数，自定义传参 + ENV 环境变量读取，后者优先级高；ENV 统一前缀，例如 `NACOS_CLIENT_CONFIG_*` 于配置管理， `NACOS_CLIENT_NAMING_*` 于服务注册
- [x] 通用客户端请求交互，Request/Response 通用 gRPC 逻辑，提供给 Config/Naming
- [x] Auth 鉴权；账密登陆 username/password，阿里云RAM鉴权 accessKey/secretKey
- [x] 通用日志，`tracing::info!()`
- [ ] Monitor，`opentelemetry`
- [x] 数据落盘与加载（用于服务端宕机弱依赖）；请设置 `load_cache_at_start(true)`，形如 `register_instance` 在服务端宕机时还会阻塞请严肃测试及正确使用

# License
[Apache License Version 2.0](LICENSE)
