# AGENTS.md

This file provides guidance to AI coding agents when working with code in this repository.

## Project Overview

This is `nacos-sdk-rust`, a Rust client library for Nacos (a dynamic service discovery, configuration and service management platform). It provides async APIs for configuration management and service naming/discovery using gRPC communication with Nacos servers.

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
- `NacosGrpcClient` maintains persistent bidirectional stream (`requestBiStream`) with server
- Uses `tonic` for gRPC transport with Tower middleware
- Connection lifecycle managed through `NacosGrpcConnection`
- Server pushes messages to client via this stream; client sends requests on the same channel
- `RequestHandler` trait allows handling server-initiated requests (like config change notifications)

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

Tests require a running Nacos server. CI workflow runs Nacos in Docker:
```bash
# For local testing, start Nacos first
docker run --name nacos-quick -e MODE=standalone -p 8848:8848 -p 9848:9848 -d nacos/nacos-server:latest
```

## Key Implementation Files

- `src/common/remote/grpc/nacos_grpc_client.rs` - Core gRPC client logic
- `src/common/remote/grpc/message/` - Request/Response message definitions and handlers
- `src/config/worker.rs` - Config change detection and cache management
- `src/naming/observable/` - Service instance change observation
- `src/api/plugin/` - Plugin trait definitions

## Dependencies Worth Knowing

- `tonic` - gRPC framework
- `tokio` - Async runtime (requires `rt-multi-thread`, `time`, `net`, `fs` features)
- `prost` - Protobuf encoding
- `serde` + `serde_json` - JSON for request/response bodies
- `tracing` - Structured logging
- `dashmap` - Concurrent hash map for caches
