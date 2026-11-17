# AGENTS.md

This file provides guidance to AI coding agents when working with code in this repository.

## Project Overview

This is `nacos-sdk-rust`, a Rust client library for Nacos (a dynamic service discovery, configuration and service management platform). It provides async APIs for configuration management and service naming/discovery using gRPC communication with Nacos servers.

Nacos Server >= 2.3.0 is required: connections perform ability negotiation at setup and older servers fail with `UnsupportedServerVersion` in a normal `build()` (`emergency_start` logs the error and keeps retrying). An ordinary `build()` makes up to 3 connection attempts, each spending server-check time, so a failed build is not instantaneous (`emergency_start` returns immediately and reconnects in the background). Requests issued before readiness wait for the connection within the same 10-second budget; in-flight requests fail immediately on disconnect, and requests already queued when the connection drops are rejected (no unbounded queue).

## Common Commands

### Build Commands
- `cargo build` - Build the library
- `cargo build --examples` - Build all example applications
- `cargo build --all` - Build workspace including nacos-macro proc-macro crate

### Feature-Specific Builds
- `cargo build --features "config,naming,auth-by-http"` - Default build
- `cargo build --features "default,auth-by-aliyun"` - With Aliyun RAM auth
- `cargo build --features "default,tls"` - Enable TLS support

### Testing Commands
- `cargo test` - Run unit tests
- `cargo test --all-targets` - Run tests for all targets (library, examples, tests)
- `cargo test -- --ignored` - Run ignored tests (includes proto build test)
- `cargo test <test_name> -- --exact` - Run a single test

### Linting and Formatting
- `cargo fmt --all` - Format all code
- `cargo fmt -- --check` - Check formatting without applying
- `cargo clippy --all` - Run clippy on workspace
- `cargo clippy --all-targets` - Run clippy on all-targets
- `cargo clippy -- -W warnings` - Treat warnings as errors (CI mode)

### Running Examples
- `cargo run --example simple_app` - Run simple example (requires local Nacos server)
- `cargo run --example lazy_app` - Run lazy initialization example
- `cargo run --example aliyun_ram_app --features "default,auth-by-aliyun"` - Run with Aliyun auth

### Proto Generation (Maintenance)
- The proto build test is marked `#[ignore]` because it regenerates protobuf files
- `cargo test build_proto -- --ignored` - Regenerate protobuf bindings when proto file changes
- Generated files are checked in and should be committed when changed

## Architecture Overview

### Module Structure
The crate uses feature-gated modules at the root level:
- `api/` - Public API exports with builder patterns for `ConfigService` and `NamingService`
- `common/` - Internal utilities: gRPC client, executor, caching, remote communication
- `config/` - Configuration service implementation (feature: `config`)
- `naming/` - Service discovery implementation (feature: `naming`)
- `nacos-macro/` - Proc-macro crate (workspace member) for request/response message attributes

### Communication Protocol
Nacos uses a custom RPC over gRPC. Key architectural notes:
- Protocol Buffers defined in `proto/nacos_grpc_service.proto`
- Requests implement `NamingRequest` or `ConfigRequest` traits (proc-macro generated)
- Response types are derived via `#[nacos_macro::response]` attribute
- All Request/Response bodies are JSON-serialized and wrapped in protobuf `Payload`
- The `Metadata.type` field carries the Java-style class name (e.g., `com.alibaba.nacos.api.naming.remote.request.ServiceQueryRequest`)

### gRPC Client Architecture
- `NacosGrpcClient` sends requests through a cloneable `ConnectionHandle`; a supervisor task owns the connection lifecycle
- `tonic` provides the transport; `PollingServerListService::select` picks the next server and `TonicBuilder` implements `Connector` directly
- The supervisor performs the Nacos >= 2.3 handshake (ServerCheck + ability negotiation + `SetupAckRequest`), then keeps the persistent bidirectional stream (`requestBiStream`) alive with health checks and jittered exponential backoff
- Server pushes are dispatched to `ServerRequestHandler` implementations (for example config change notifications); the stream pump intercepts `SetupAckRequest` locally and never replies to it
- Connection states are published through a `watch` channel; connected/disconnected listeners fire on `Ready` transitions

### Builder Pattern and Service Lifecycle
Both ConfigService and NamingService use builder initialization:
1. `ClientProps` holds connection settings, auth credentials, namespace
2. Builders like `ConfigServiceBuilder` optionally enable auth plugins
3. `build()` establishes gRPC connection and starts background workers
4. Services maintain long-lived connections; create once and hold for application lifetime

### Auth Plugins (api/plugin/auth/)
Auth plugins are optional feature-gated:
- `auth-by-http`: Username/password auth via HTTP login endpoint
- `auth-by-aliyun`: Alibaba Cloud RAM signature-based auth
Both implement `AuthPlugin` trait; auth context is injected into gRPC metadata

### Config Filter and Encryption
- `ConfigFilter` trait allows transforming config content (e.g., decryption)
- `EncryptionPlugin` provides pluggable encryption/decryption
- Filters are applied when configs are received (after cache storage), not before storing

### Configuration Sources (Priority Order)
1. Environment variables (highest priority)
2. ClientProps explicitly passed
3. Default values
Environment variables use prefix `NACOS_CLIENT_` for common props, `NACOS_CLIENT_CONFIG_*` for config, `NACOS_CLIENT_NAMING_*` for naming.

### Clippy Configuration
`.clippy.toml` enforces specific constraints:
- `type-complexity-threshold = 500`
- Disallows `std::env::set_var`, `std::env::remove_var` (thread safety issues)
- Disallows `std::thread::sleep` (use `tokio::time::sleep` instead)
- Disallows `std::time::Instant` (use `tokio::time::Instant` for consistency)

## Testing Notes

## Testing

### Integration Tests

Tests start `nacos/nacos-server:v2.5.4` automatically by default (podman first, then docker).
Nacos server >= 2.3.0 is required.

```bash
# Default: auto-start a container
cargo test --test it_config --test it_naming --test it_auth --features "config,naming,auth-by-http" -- --include-ignored

# Force a runtime
NACOS_CONTAINER_RUNTIME=podman cargo test --test it_config --features "config" -- --include-ignored

# External Nacos on 127.0.0.1:8848 (used by CI)
NACOS_SERVER=external cargo test --test it_config --test it_naming --test it_auth --features "config,naming,auth-by-http" -- --include-ignored

# Nacos 3.3 with auth disabled (console readiness is mapped to http_port + 2000)
NACOS_SERVER_PROFILE=3.3 \
NACOS_DOCKER_IMAGE=docker.m.daocloud.io/nacos/nacos-server:v3.3.0 \
cargo test --test it_config --test it_naming --features "config,naming" -- --include-ignored
```

`NACOS_SERVER_PROFILE` accepts `2.5.4` (default) or `3.3`; `NACOS_DOCKER_IMAGE` overrides the
image independently (use the daocloud mirror when Docker Hub is unreachable). The 3.3 profile
starts with `NACOS_AUTH_ENABLE=false`, but the image still requires `NACOS_AUTH_TOKEN` and
`NACOS_AUTH_IDENTITY_KEY/VALUE`. Auth-on bootstrap is not implemented yet, so keep `it_auth`
on 2.5.4. 3.3 reports readiness on its console (8080); the fixture maps `http_port + 2000` and
also requires `/nacos/v1/ns/operator/metrics` on 8848 before starting tests.

Known container-mode failures on both 2.5.4 and 3.3 (external mode skips both, and CI uses
external): `test_publish_config_beta` publishes to beta IP `127.0.0.1`, which does not match
the container source IP, and `test_register_instance_with_cluster` uses `CUSTOM_CLUSTER`,
whose underscore the server rejects. CI runs the container matrix for both versions in
external mode; `it_auth` is gated to 2.5.4 until auth-on bootstrap exists.

rnacos is not supported yet: it does not implement ability negotiation (`SetupAckRequest`).

AI 集成测试（`tests/it_ai.rs`）仅在 Nacos 3.x 上运行：本地用 `NACOS_SERVER_PROFILE=3.3`（默认 2.5.4 会打印跳过原因）；CI 在 3.3 matrix 行执行。MCP 需要服务端 >= 3.0.3，Prompt 需要 >= 3.2.0（实测基线 3.3.0）；`auth-by-aliyun` 暂不支持 AI。

MCP 订阅按 `name::version`（空版本为 `latest`）每键轮询，默认 10s；`NOT_FOUND` 保留旧缓存并告警；最后一个 listener 退订后任务停止。

AI integration tests (`tests/it_ai.rs`) run only on Nacos 3.x: use `NACOS_SERVER_PROFILE=3.3` locally (the 2.5.4 default prints a skip reason), and CI runs them in the 3.3 matrix row. MCP requires server >= 3.0.3 and Prompt requires >= 3.2.0 (tested baseline 3.3.0); `auth-by-aliyun` does not support AI yet.

MCP subscriptions poll per `name::version` (empty → `latest`), 10s by default; NOT_FOUND keeps the cached value and warns; the task stops after the last unsubscribe.

## Key Implementation Files

- `src/common/remote/grpc/nacos_grpc_client.rs` - Core gRPC client logic
- `src/common/remote/grpc/connection/` - Connection supervisor, handshake, stream pump, health checks, backoff
- `src/common/remote/grpc/message/` - Request/Response message definitions and handlers
- `src/config/worker.rs` - Config change detection and cache management
- `src/naming/observable/` - Service instance change observation
- `src/api/plugin/` - Plugin trait definitions
- `src/ai/` - AI service: MCP registry, endpoint redo, and prompt reads (feature `ai`)

## Dependencies Worth Knowing

- `tonic` - gRPC framework
- `tokio` - Async runtime (requires `rt-multi-thread`, `time`, `net`, `fs`, `macros` features)
- `prost` - Protobuf encoding
- `serde` + `serde_json` - JSON for request/response bodies
- `tracing` - Structured logging
- `dashmap` - Concurrent hash map for caches
