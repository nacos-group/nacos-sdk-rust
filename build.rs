fn main() {
    tonic_build::configure()
        .build_client(true)
        .build_server(false)
        .build_transport(true)
        .compile(&["proto/nacos_grpc_service.proto"], &["proto"])
        .unwrap()
}
