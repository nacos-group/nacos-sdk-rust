pub(crate) mod ability;
pub(crate) mod endpoint_redo;
pub(crate) mod message;
pub(crate) mod model;
pub(crate) mod updater;

use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

use crate::ai::ability::{ensure_mcp_draft_release, ensure_mcp_registry};
use crate::ai::endpoint_redo::{EndpointOps, EndpointRedo};
use crate::ai::message::request::*;
use crate::ai::message::response::*;
use crate::ai::message::{INVALID_PARAM_CODE, NOT_FOUND_CODE, response_error};
use crate::ai::model::mcp::{
    McpEndpointSpec, McpResourceSpecification, McpServerBasicInfo, McpServerDetailInfo,
    McpToolSpecification,
};
use crate::ai::model::prompt::Prompt;
use crate::ai::updater::{McpServerListener, McpServerQuery, McpServerUpdater, query_result};
use crate::api::error::{Error, Result};
use crate::api::plugin::AuthPlugin;
use crate::api::props::ClientProps;
use crate::common::error::handle_response;
use crate::common::remote::generate_request_id;
use crate::common::remote::grpc::connection::abilities::{MCP, MCP_DRAFT_RELEASE};
use crate::common::remote::grpc::message::{GrpcRequestMessage, GrpcResponseMessage};
use crate::common::remote::grpc::{NacosGrpcClient, NacosGrpcClientBuilder};
use crate::common::remote::server_list::create_server_list_provider;

const MODULE_NAME: &str = "ai";

pub(crate) struct NacosAiService {
    nacos_grpc_client: Arc<NacosGrpcClient>,
    namespace: String,
    endpoint_redo: Arc<EndpointRedo>,
    mcp_updater: Arc<McpServerUpdater>,
    query: Arc<GrpcMcpQuery>,
}

impl std::fmt::Debug for NacosAiService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NacosAiService")
            .field("namespace", &self.namespace)
            .finish()
    }
}

impl NacosAiService {
    pub(crate) async fn new(
        client_props: ClientProps,
        auth_plugin: Arc<dyn AuthPlugin>,
        update_interval: Duration,
    ) -> Result<Self> {
        let namespace = client_props.get_namespace_default_if_empty();
        let server_list_provider = create_server_list_provider(&client_props).await?;
        let pending_redo: Arc<std::sync::OnceLock<std::sync::Weak<EndpointRedo>>> =
            Arc::new(std::sync::OnceLock::new());
        let listener_redo = pending_redo.clone();
        let nacos_grpc_client = NacosGrpcClientBuilder::new(server_list_provider)
            .port(client_props.get_remote_grpc_port())
            .namespace(namespace.clone())
            .client_version(client_props.get_client_version())
            .add_labels(client_props.get_labels())
            .add_default_labels(crate::api::constants::common_remote::LABEL_MODULE_AI)
            .app_name(client_props.get_app_name())
            .auth_context(client_props.get_auth_context())
            .auth_plugin(auth_plugin)
            .connected_listener(move |connection_id| {
                if let Some(redo) = listener_redo.get().and_then(std::sync::Weak::upgrade) {
                    crate::common::executor::spawn(async move {
                        redo.on_connected(connection_id).await;
                        redo.flush().await;
                    });
                }
            })
            .build(crate::common::util::generate_client_id(
                MODULE_NAME,
                &client_props.get_address_identifier(),
                &namespace,
                &AI_SEQ,
            ))
            .await?;

        let nacos_grpc_client = Arc::new(nacos_grpc_client);
        let endpoint_redo = EndpointRedo::new(
            namespace.clone(),
            Arc::new(GrpcEndpointOps {
                client: nacos_grpc_client.clone(),
            }),
            Duration::from_secs(3),
        );
        let _ = pending_redo.set(Arc::downgrade(&endpoint_redo));
        if let Some(connection_id) = nacos_grpc_client.connection_id() {
            endpoint_redo.on_connected(connection_id).await;
        }

        let query = Arc::new(GrpcMcpQuery {
            client: nacos_grpc_client.clone(),
            namespace: namespace.clone(),
        });
        let mcp_updater = McpServerUpdater::new(namespace.clone(), query.clone(), update_interval);

        Ok(Self {
            nacos_grpc_client,
            namespace,
            endpoint_redo,
            mcp_updater,
            query,
        })
    }

    async fn request_to_server<R, P>(&self, request: R) -> Result<P>
    where
        R: GrpcRequestMessage + 'static,
        P: GrpcResponseMessage + 'static,
    {
        self.nacos_grpc_client.send_request::<R, P>(request).await
    }

    pub(crate) async fn get_mcp_server(
        &self,
        mcp_name: String,
        version: Option<String>,
    ) -> Result<McpServerDetailInfo> {
        ensure_mcp_registry(self.nacos_grpc_client.server_ability(MCP).await?)?;
        let response = self.query.send_query(&mcp_name, version.as_deref()).await?;
        handle_response(&response, "mcp server query")?;
        response
            .mcp_server_detail_info
            .ok_or_else(|| Error::ErrResult("mcp server query returned no detail".to_string()))
    }

    pub(crate) async fn subscribe_mcp_server(
        &self,
        mcp_name: String,
        version: Option<String>,
        listener: Arc<dyn McpServerListener>,
    ) -> Result<Option<McpServerDetailInfo>> {
        ensure_mcp_registry(self.nacos_grpc_client.server_ability(MCP).await?)?;
        let value = self
            .mcp_updater
            .subscribe(&mcp_name, version.as_deref(), listener)
            .await?;
        Ok(value.map(Arc::unwrap_or_clone))
    }

    pub(crate) async fn unsubscribe_mcp_server(
        &self,
        mcp_name: String,
        version: Option<String>,
        listener: &Arc<dyn McpServerListener>,
    ) -> Result<()> {
        self.mcp_updater
            .unsubscribe(&mcp_name, version.as_deref(), listener)
            .await
    }

    pub(crate) async fn release_mcp_server(
        &self,
        basic: McpServerBasicInfo,
        mcp_name: String,
        tools: Option<McpToolSpecification>,
        resources: Option<McpResourceSpecification>,
        endpoint: Option<McpEndpointSpec>,
        create_draft: bool,
    ) -> Result<String> {
        ensure_mcp_registry(self.nacos_grpc_client.server_ability(MCP).await?)?;
        if create_draft {
            ensure_mcp_draft_release(
                self.nacos_grpc_client
                    .server_ability(MCP_DRAFT_RELEASE)
                    .await?,
            )?;
        }
        let request = ReleaseMcpServerRequest {
            namespace_id: Some(self.namespace.clone()),
            mcp_name: Some(mcp_name),
            server_specification: Some(basic),
            tool_specification: tools,
            resource_specification: resources,
            endpoint_specification: endpoint,
            create_draft,
            request_id: Some(generate_request_id()),
            ..Default::default()
        };
        let response: ReleaseMcpServerResponse = self.request_to_server(request).await?;
        handle_response(&response, "mcp server release")?;
        response
            .mcp_id
            .ok_or_else(|| Error::ErrResult("mcp release returned no mcp id".to_string()))
    }

    pub(crate) async fn get_prompt(
        &self,
        prompt_key: String,
        version: Option<String>,
        label: Option<String>,
    ) -> Result<Prompt> {
        let request = QueryPromptRequest {
            namespace_id: Some(self.namespace.clone()),
            prompt_key: Some(prompt_key),
            version,
            label,
            request_id: Some(generate_request_id()),
            ..Default::default()
        };
        let response: QueryPromptResponse = self.request_to_server(request).await?;
        handle_response(&response, "prompt query")?;
        response
            .prompt_info
            .ok_or_else(|| Error::ErrResult("prompt query returned no prompt".to_string()))
    }

    pub(crate) async fn register_mcp_server_endpoint(
        &self,
        mcp_name: String,
        address: String,
        port: u16,
        version: Option<String>,
    ) -> Result<()> {
        ensure_mcp_registry(self.nacos_grpc_client.server_ability(MCP).await?)?;
        self.endpoint_redo
            .register(mcp_name, address, i32::from(port), version)
            .await
    }

    pub(crate) async fn deregister_mcp_server_endpoint(
        &self,
        mcp_name: String,
        address: String,
        port: u16,
    ) -> Result<()> {
        ensure_mcp_registry(self.nacos_grpc_client.server_ability(MCP).await?)?;
        self.endpoint_redo
            .deregister(mcp_name, address, i32::from(port))
            .await
    }
}

fn endpoint_request(
    namespace: &str,
    mcp_name: &str,
    address: &str,
    port: i32,
    version: Option<&str>,
    endpoint_type: &str,
) -> McpServerEndpointRequest {
    McpServerEndpointRequest {
        namespace_id: Some(namespace.to_string()),
        mcp_name: Some(mcp_name.to_string()),
        address: Some(address.to_string()),
        port,
        version: version.map(str::to_string),
        r#type: Some(endpoint_type.to_string()),
        request_id: Some(generate_request_id()),
        ..Default::default()
    }
}

fn query_mcp_request(
    namespace: &str,
    mcp_name: &str,
    version: Option<&str>,
) -> QueryMcpServerRequest {
    QueryMcpServerRequest {
        namespace_id: Some(namespace.to_string()),
        mcp_name: Some(mcp_name.to_string()),
        version: version.map(str::to_string),
        request_id: Some(generate_request_id()),
        ..Default::default()
    }
}

pub(crate) struct GrpcMcpQuery {
    client: Arc<NacosGrpcClient>,
    namespace: String,
}

impl GrpcMcpQuery {
    async fn send_query(
        &self,
        name: &str,
        version: Option<&str>,
    ) -> Result<QueryMcpServerResponse> {
        let request = query_mcp_request(&self.namespace, name, version);
        self.client.send_request(request).await
    }
}

#[async_trait::async_trait]
impl McpServerQuery for GrpcMcpQuery {
    async fn query(&self, name: &str, version: Option<&str>) -> Result<McpServerDetailInfo> {
        query_result(self.send_query(name, version).await?)
    }
}

pub(crate) struct GrpcEndpointOps {
    client: Arc<NacosGrpcClient>,
}

#[async_trait::async_trait]
impl EndpointOps for GrpcEndpointOps {
    async fn register(
        &self,
        namespace: &str,
        mcp_name: &str,
        address: &str,
        port: i32,
        version: Option<&str>,
    ) -> Result<()> {
        let request = endpoint_request(
            namespace,
            mcp_name,
            address,
            port,
            version,
            McpServerEndpointRequest::REGISTER_ENDPOINT,
        );
        let response: McpServerEndpointResponse = self.client.send_request(request).await?;
        handle_response(&response, "mcp endpoint register")
    }

    async fn deregister(
        &self,
        namespace: &str,
        mcp_name: &str,
        address: &str,
        port: i32,
    ) -> Result<()> {
        let request = endpoint_request(
            namespace,
            mcp_name,
            address,
            port,
            None,
            McpServerEndpointRequest::DEREGISTER_ENDPOINT,
        );
        let response: McpServerEndpointResponse = self.client.send_request(request).await?;
        if !response.is_success()
            && matches!(response.error_code(), NOT_FOUND_CODE | INVALID_PARAM_CODE)
        {
            return Err(response_error(&response));
        }
        handle_response(&response, "mcp endpoint deregister")
    }
}

static AI_SEQ: AtomicU64 = AtomicU64::new(1);
