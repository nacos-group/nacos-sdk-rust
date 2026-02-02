// Licensed to the Apache Software Foundation (ASF) under one or more
// contributor license agreements.  See the NOTICE file distributed with
// this work for additional information regarding copyright ownership.
// The ASF licenses this file to You under the Apache License, Version 2.0
// (the "License"); you may not use this file except in compliance with
// the License.  You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//

// This test helps to keep files generated and used by tonic_build(proto) update to date.
// If the generated files has been changed, please commit they.
#[test]
#[ignore]
fn build_proto() {
    tonic_build::configure()
        .build_client(true)
        .build_server(false)
        .build_transport(true)
        .out_dir("src")
        .compile_protos(&["./proto/nacos_grpc_service.proto"], &["./proto"])
        .expect("Failed to compile proto files for nacos gRPC service");
}
