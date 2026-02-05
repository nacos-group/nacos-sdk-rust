use super::RequestResource;
use crate::api::plugin::{AuthContext, AuthPlugin, LoginIdentityContext};
use arc_swap::ArcSwap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) const ACCESS_KEY: &str = "accessKey";
pub(crate) const ACCESS_SECRET: &str = "secretKey";
pub(crate) const SIGN_REGION_ID: &str = "signature_region_id";
pub const SIGNATURE_VERSION: &str = "signatureVersion";
pub const GROUPED_NAME: &str = "groupedName";
pub const ACCESS_KEY_HEADER: &str = "Spas-AccessKey";
pub const V4: &str = "v4";
pub const SIGNATURE_V4_METHOD: &str = "HmacSHA256";
pub const SIGNATURE_V4_PRODUCE: &str = "mse-nacos";
pub const PREFIX: &str = "aliyun_v4";
pub const CONSTANT: &str = "aliyun_v4_request";
pub const V4_SIGN_DATE_FORMATTER: &str = "%Y%m%d";
pub const SIGNATURE_FILED: &str = "signature";
pub const DATA_FILED: &str = "data";
pub const AK_FILED: &str = "ak";
pub const TIMESTAMP_HEADER: &str = "Timestamp";
pub const SIGNATURE_HEADER: &str = "Spas-Signature";
pub const GROUP_KEY: &str = "group";
pub const TENANT_KEY: &str = "tenant";
pub const SHA_ENCRYPT: &str = "HmacSHA1";

fn get_current_timestamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System time should be after UNIX_EPOCH")
        .as_millis()
}

mod sign_utils {
    use base64::Engine;
    use ring::hmac::{self, Algorithm, HMAC_SHA1_FOR_LEGACY_USE_ONLY};

    pub fn sign_with_alg(data: &[u8], key: &[u8], alg: Algorithm) -> Vec<u8> {
        let key = hmac::Key::new(alg, key);
        let tag = hmac::sign(&key, data);
        tag.as_ref().to_vec()
    }

    pub fn sign_to_base64_with_alg(data: &[u8], key: &[u8], alg: Algorithm) -> String {
        base64::prelude::BASE64_STANDARD.encode(sign_with_alg(data, key, alg).as_slice())
    }

    pub fn sign_with_hmac_sha1(data: &str, key: &str) -> String {
        sign_to_base64_with_alg(
            data.as_bytes(),
            key.as_bytes(),
            HMAC_SHA1_FOR_LEGACY_USE_ONLY,
        )
    }
}

mod spas_adaptor {
    use crate::api::plugin::auth::auth_by_aliyun_ram::{get_current_timestamp, sign_utils};
    use crate::api::plugin::{SIGNATURE_HEADER, TIMESTAMP_HEADER};
    use std::collections::HashMap;

    pub fn get_sign_header_for_resource(
        resource: &str,
        secret_key: &str,
    ) -> HashMap<String, String> {
        let mut headers = HashMap::default();
        let timestamp = get_current_timestamp();
        headers.insert(TIMESTAMP_HEADER.to_string(), timestamp.to_string());
        if !secret_key.is_empty() {
            let sign = if resource.is_empty() {
                sign_utils::sign_with_hmac_sha1(&timestamp.to_string(), secret_key)
            } else {
                sign_utils::sign_with_hmac_sha1(
                    format!("{}+{}", resource, timestamp).as_str(),
                    secret_key,
                )
            };
            headers.insert(SIGNATURE_HEADER.to_string(), sign.to_string());
        }
        headers
    }
}

mod calculate_v4_signing_key_util {
    use crate::api::plugin::{PREFIX, V4_SIGN_DATE_FORMATTER};
    use chrono::Utc;
    use ring::hmac::{self, Algorithm};

    use super::{
        CONSTANT, SIGNATURE_V4_PRODUCE,
        sign_utils::{self, sign_to_base64_with_alg},
    };

    pub fn first_signing_key(secret: &str, date: &str, alg: Algorithm) -> Vec<u8> {
        sign_utils::sign_with_alg(
            date.as_bytes(),
            format!("{}{}", PREFIX, secret).as_bytes(),
            alg,
        )
    }

    pub fn region_signing_key(secret: &str, date: &str, region: &str, alg: Algorithm) -> Vec<u8> {
        let vec = first_signing_key(secret, date, alg);
        sign_utils::sign_with_alg(region.as_bytes(), vec.as_slice(), alg)
    }

    pub fn final_signing_key_string(
        secret: &str,
        date: &str,
        region: &str,
        product_code: &str,
        alg: Algorithm,
    ) -> String {
        let second_sign_key = region_signing_key(secret, date, region, alg);
        let third_signing_key =
            sign_utils::sign_with_alg(product_code.as_bytes(), &second_sign_key, alg);
        sign_to_base64_with_alg(CONSTANT.as_bytes(), &third_signing_key, alg)
    }

    pub fn final_signing_key_string_with_default_info(secret: &str, region: &str) -> String {
        let sign_date = Utc::now().format(V4_SIGN_DATE_FORMATTER).to_string();
        final_signing_key_string(
            secret,
            &sign_date,
            region,
            SIGNATURE_V4_PRODUCE,
            hmac::HMAC_SHA256,
        )
    }
}

pub(crate) struct AliyunRamAuthPlugin {
    access_key: ArcSwap<Option<String>>,
    secret_key: ArcSwap<Option<String>>,
    signature_region_id: ArcSwap<Option<String>>,
    initialized: ArcSwap<bool>,
}

impl AliyunRamAuthPlugin {
    pub(crate) fn default() -> Self {
        Self {
            access_key: ArcSwap::from_pointee(None),
            secret_key: ArcSwap::from_pointee(None),
            signature_region_id: ArcSwap::from_pointee(None),
            initialized: ArcSwap::from_pointee(false),
        }
    }

    fn get_login_identify_for_naming(&self, resource: RequestResource) -> LoginIdentityContext {
        Self::get_sign_data(&resource)
            .map(|sign_data| {
                let mut res = LoginIdentityContext::default();
                let mut signature_key = self
                    .secret_key
                    .load()
                    .as_ref()
                    .as_ref()
                    .expect("Secret key should be initialized before getting login identity")
                    .clone();
                if let Some(region_id) = self.signature_region_id.load().as_ref() {
                    signature_key =
                        calculate_v4_signing_key_util::final_signing_key_string_with_default_info(
                            self.secret_key.load().as_ref().as_ref().expect(
                                "Secret key should be initialized before calculating signature",
                            ),
                            region_id,
                        );
                    res = res.add_context(SIGNATURE_VERSION, V4);
                }
                let signature = sign_utils::sign_with_hmac_sha1(&sign_data, &signature_key);
                res = res.add_context(SIGNATURE_FILED, signature);
                res = res.add_context(DATA_FILED, sign_data);
                res =
                    res.add_context(
                        AK_FILED,
                        self.access_key.load().as_ref().as_ref().expect(
                            "Access key should be initialized before getting login identity",
                        ),
                    );
                res
            })
            .unwrap_or_default()
    }

    fn get_sign_data(resource: &RequestResource) -> Option<String> {
        Self::get_grouped_service_name(resource).map(|grouped_service_name| {
            let timestamp = get_current_timestamp();
            if grouped_service_name.is_empty() {
                format!("{}", timestamp)
            } else {
                format!("{}@@{}", timestamp, grouped_service_name)
            }
        })
    }

    fn get_grouped_service_name(resource: &RequestResource) -> Option<String> {
        resource.resource.as_ref().map(|service_name| {
            if service_name.contains("@@") || resource.group.is_none() {
                service_name.clone()
            } else {
                format!(
                    "{}@@{}",
                    resource
                        .group
                        .as_ref()
                        .expect("Group should be set when formatting grouped service name"),
                    service_name
                )
            }
        })
    }

    fn get_login_identify_for_config(&self, resource: RequestResource) -> LoginIdentityContext {
        let mut res = LoginIdentityContext::default();
        let mut signature_key = self
            .secret_key
            .load()
            .as_ref()
            .as_ref()
            .expect("Secret key should be initialized before getting login identity")
            .clone();
        if let Some(region_id) = self.signature_region_id.load().as_ref().as_ref() {
            signature_key =
                calculate_v4_signing_key_util::final_signing_key_string_with_default_info(
                    self.secret_key
                        .load()
                        .as_ref()
                        .as_ref()
                        .expect("Secret key should be initialized before calculating v4 signature"),
                    region_id,
                );
            res = res.add_context(SIGNATURE_VERSION, V4);
        }
        res = res.add_context(
            ACCESS_KEY_HEADER,
            self.access_key
                .load()
                .as_ref()
                .as_ref()
                .expect("Access key should be initialized before getting login identity"),
        );
        res = res.add_contexts(spas_adaptor::get_sign_header_for_resource(
            Self::get_resource_for_config(&resource).as_str(),
            signature_key.as_str(),
        ));
        res
    }

    fn get_resource_for_config(resource: &RequestResource) -> String {
        let namespace = resource
            .namespace
            .as_ref()
            .filter(|value| !value.is_empty());
        let group = resource.group.as_ref().filter(|value| !value.is_empty());
        if namespace.is_some() && group.is_some() {
            format!(
                "{}+{}",
                namespace.expect("Namespace should be set when formatting resource for config"),
                group.expect("Group should be set when formatting resource for config")
            )
        } else if let Some(g) = group {
            g.to_string()
        } else if let Some(n) = namespace {
            n.to_string()
        } else {
            String::from("")
        }
    }
}

#[async_trait::async_trait]
impl AuthPlugin for AliyunRamAuthPlugin {
    /// no need to login
    async fn login(&self, _: Vec<String>, auth_context: AuthContext) {
        if *self.initialized.load().as_ref() {
            return;
        }

        let ak = auth_context
            .params
            .get(ACCESS_KEY)
            .expect("Init aliyun ram auth plugin error, missing accessKey")
            .to_owned();
        let sk = auth_context
            .params
            .get(ACCESS_SECRET)
            .expect("Init aliyun ram auth plugin error, missing secretKey")
            .to_owned();
        tracing::info!("Initialize using ak: {}", ak);
        self.access_key.store(Arc::new(Some(ak)));
        self.secret_key.store(Arc::new(Some(sk)));
        if let Some(signature_region_id) = auth_context.params.get(SIGN_REGION_ID) {
            self.signature_region_id
                .store(Arc::new(Some(signature_region_id.to_string())));
            tracing::info!(
                "Initializing using signature region: {}",
                signature_region_id
            );
        }
        self.initialized.store(Arc::new(true));
    }

    /// Get the [`LoginIdentityContext`].
    fn get_login_identity(&self, resource: RequestResource) -> LoginIdentityContext {
        tracing::trace!("signature to resource {:?}", resource);
        match resource.request_type.as_str() {
            "Naming" => self.get_login_identify_for_naming(resource),
            "Config" => self.get_login_identify_for_config(resource),
            _ => LoginIdentityContext::default(),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::api::config::{ConfigChangeListener, ConfigResponse, ConfigService};
    use crate::api::naming::{
        NamingChangeEvent, NamingEventListener, NamingService, ServiceInstance,
    };
    use crate::api::plugin::auth::auth_by_aliyun_ram::{
        calculate_v4_signing_key_util, sign_utils, spas_adaptor,
    };
    use crate::api::plugin::{
        ACCESS_KEY, ACCESS_KEY_HEADER, ACCESS_SECRET, AK_FILED, AliyunRamAuthPlugin, AuthContext,
        AuthPlugin, DATA_FILED, RequestResource, SIGN_REGION_ID, SIGNATURE_FILED, SIGNATURE_HEADER,
        SIGNATURE_VERSION, TIMESTAMP_HEADER,
    };
    use crate::api::props::ClientProps;
    use crate::config::NacosConfigService;
    use crate::naming::NacosNamingService;
    use arc_swap::ArcSwap;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::sleep;

    #[test]
    fn test_sign_with_hmac_sha1() {
        let sign_data = sign_utils::sign_with_hmac_sha1("test", "test");
        // DJRRXBXlCVuKh6ULoN87847QX+Y= is signed data generated by
        // com.alibaba.nacos.client.auth.ram.utils.SignUtil#sign
        // method in nacos java sdk
        assert_eq!(sign_data, "DJRRXBXlCVuKh6ULoN87847QX+Y=");
    }

    #[test]
    fn test_get_sign_header_for_resource() {
        let mut sign_headers = spas_adaptor::get_sign_header_for_resource("test", "");
        assert_eq!(sign_headers.len(), 1);
        assert!(sign_headers.contains_key(TIMESTAMP_HEADER));

        sign_headers = spas_adaptor::get_sign_header_for_resource("", "test");
        assert_eq!(sign_headers.len(), 2);
        let timestamp = sign_headers
            .get(TIMESTAMP_HEADER)
            .expect("Timestamp header should exist in sign_headers");
        let sign_data = sign_headers
            .get(SIGNATURE_HEADER)
            .expect("Signature header should exist in sign_headers");
        let expected_sign_data = sign_utils::sign_with_hmac_sha1(timestamp, "test");
        assert_eq!(&expected_sign_data, sign_data);

        sign_headers = spas_adaptor::get_sign_header_for_resource("test", "test");
        assert_eq!(sign_headers.len(), 2);
        let timestamp = sign_headers
            .get(TIMESTAMP_HEADER)
            .expect("Timestamp header should exist in sign_headers");
        let sign_data = sign_headers
            .get(SIGNATURE_HEADER)
            .expect("Signature header should exist in sign_headers");
        let expected_sign_data =
            sign_utils::sign_with_hmac_sha1(format!("{}+{}", "test", timestamp).as_str(), "test");
        assert_eq!(&expected_sign_data, sign_data);
    }

    #[tokio::test]
    async fn test_aliyun_ram_plugin_login() {
        let aliyun_ram_auth_plugin = AliyunRamAuthPlugin::default();
        let mut context = AuthContext::default();
        context = context.add_param(ACCESS_KEY, "test");
        context = context.add_param(ACCESS_SECRET, "test");
        context = context.add_param(SIGN_REGION_ID, "cn-hangzhou");
        aliyun_ram_auth_plugin.login(Vec::new(), context).await;
        assert!(*aliyun_ram_auth_plugin.initialized.load().as_ref());
        assert_eq!(
            "test",
            aliyun_ram_auth_plugin
                .access_key
                .load()
                .as_ref()
                .as_ref()
                .expect("System time should be after UNIX_EPOCH")
        );
        assert_eq!(
            "test",
            aliyun_ram_auth_plugin
                .secret_key
                .load()
                .as_ref()
                .as_ref()
                .expect("System time should be after UNIX_EPOCH")
        );
        assert_eq!(
            "cn-hangzhou",
            aliyun_ram_auth_plugin
                .signature_region_id
                .load()
                .as_ref()
                .as_ref()
                .expect("System time should be after UNIX_EPOCH")
        );
    }

    #[test]
    fn test_get_resource_for_config() {
        let mut resource = RequestResource::default();
        resource.group = Some("test-group".to_owned());
        let resource = AliyunRamAuthPlugin::get_resource_for_config(&resource);
        assert_eq!("test-group", resource);

        let mut resource = RequestResource::default();
        resource.namespace = Some("test-ns".to_owned());
        let resource = AliyunRamAuthPlugin::get_resource_for_config(&resource);
        assert_eq!("test-ns", resource);

        let mut resource = RequestResource::default();
        resource.namespace = Some("test-ns".to_owned());
        resource.group = Some("test-group".to_owned());
        let resource = AliyunRamAuthPlugin::get_resource_for_config(&resource);
        assert_eq!("test-ns+test-group", resource);

        let resource = RequestResource::default();
        let resource = AliyunRamAuthPlugin::get_resource_for_config(&resource);
        assert_eq!("", resource)
    }

    #[test]
    fn test_get_grouped_service() {
        let mut resource = RequestResource::default();
        resource.resource = Some("test@@test".to_owned());
        let grouped_service_name = AliyunRamAuthPlugin::get_grouped_service_name(&resource);
        assert_eq!(
            "test@@test",
            grouped_service_name.expect("Grouped service name should exist")
        );

        let mut resource = RequestResource::default();
        resource.resource = Some("test".to_owned());
        let grouped_service_name = AliyunRamAuthPlugin::get_grouped_service_name(&resource);
        assert_eq!(
            "test",
            grouped_service_name.expect("Grouped service name should exist")
        );

        let mut resource = RequestResource::default();
        resource.resource = Some("test".to_owned());
        resource.group = Some("test".to_owned());
        let grouped_service_name = AliyunRamAuthPlugin::get_grouped_service_name(&resource);
        assert_eq!(
            "test@@test",
            grouped_service_name.expect("Grouped service name should exist")
        );
    }

    #[test]
    fn test_get_sign_data() {
        let mut request = RequestResource::default();
        request.resource = Some("test".to_owned());
        let sign_data = AliyunRamAuthPlugin::get_sign_data(&request)
            .expect("Sign data should exist for request");
        assert!(sign_data.contains("@@test"))
    }

    #[tokio::test]
    async fn test_get_login_identity_for_naming() {
        let aliyun_ram_auth_plugin = AliyunRamAuthPlugin::default();
        let mut context = AuthContext::default();
        context = context.add_param(ACCESS_KEY, "test-ak");
        context = context.add_param(ACCESS_SECRET, "test-sk");
        context = context.add_param(SIGN_REGION_ID, "cn-hangzhou");
        aliyun_ram_auth_plugin.login(Vec::new(), context).await;

        let mut resource = RequestResource::default();
        resource.request_type = "Naming".to_owned();
        resource.namespace = Some("".to_owned());
        resource.group = Some("test-group".to_owned());
        resource.resource = Some("test-resource".to_owned());
        let identity_context = aliyun_ram_auth_plugin.get_login_identify_for_naming(resource);
        assert_eq!(4, identity_context.contexts.len());
        assert_eq!(
            "v4",
            identity_context
                .contexts
                .get(SIGNATURE_VERSION)
                .expect("Signature version should exist in identity contexts")
        );
        assert_eq!(
            "test-ak",
            identity_context
                .contexts
                .get(AK_FILED)
                .expect("AK field should exist in identity contexts")
        );
        let data = identity_context
            .contexts
            .get(DATA_FILED)
            .expect("Data field should exist in identity contexts");
        let sign_data = identity_context
            .contexts
            .get(SIGNATURE_FILED)
            .expect("System time should be after UNIX_EPOCH")
            .clone();

        let signature_key =
            calculate_v4_signing_key_util::final_signing_key_string_with_default_info(
                "test-sk",
                "cn-hangzhou",
            );
        assert_eq!(
            sign_utils::sign_with_hmac_sha1(&data, &signature_key),
            sign_data
        );
    }

    #[tokio::test]
    async fn test_get_login_identity_for_config() {
        let aliyun_ram_auth_plugin = AliyunRamAuthPlugin::default();
        let mut context = AuthContext::default();
        context = context.add_param(ACCESS_KEY, "test-ak");
        context = context.add_param(ACCESS_SECRET, "test-sk");
        context = context.add_param(SIGN_REGION_ID, "cn-hangzhou");
        aliyun_ram_auth_plugin.login(Vec::new(), context).await;

        let mut resource = RequestResource::default();
        resource.request_type = "Config".to_owned();
        resource.namespace = Some("".to_owned());
        resource.group = Some("test-group".to_owned());
        resource.resource = Some("test-resource".to_owned());
        let identity_context = aliyun_ram_auth_plugin.get_login_identify_for_config(resource);
        assert_eq!(4, identity_context.contexts.len());
        assert_eq!(
            "v4",
            identity_context
                .contexts
                .get(SIGNATURE_VERSION)
                .expect("Signature version should exist in identity contexts")
        );
        assert_eq!(
            "test-ak",
            identity_context
                .contexts
                .get(ACCESS_KEY_HEADER)
                .expect("Access key header should exist in identity contexts")
        );

        let timestamp = identity_context
            .contexts
            .get(TIMESTAMP_HEADER)
            .expect("Timestamp header should exist in identity contexts");
        let sign_data = identity_context
            .contexts
            .get(SIGNATURE_HEADER)
            .expect("Signature header should exist in identity contexts");

        let signature_key =
            calculate_v4_signing_key_util::final_signing_key_string_with_default_info(
                "test-sk",
                "cn-hangzhou",
            );
        assert_eq!(
            &sign_utils::sign_with_hmac_sha1(
                format!("{}+{}", "test-group", timestamp).as_str(),
                &signature_key
            ),
            sign_data
        );
    }

    struct TestNamingEventListener {
        instance_now: ArcSwap<Vec<ServiceInstance>>,
    }

    impl NamingEventListener for TestNamingEventListener {
        fn event(&self, event: Arc<NamingChangeEvent>) {
            self.instance_now.store(Arc::new(
                event
                    .instances
                    .as_ref()
                    .expect("Event instances should exist")
                    .clone(),
            ));
        }
    }

    impl TestNamingEventListener {
        fn default() -> Self {
            Self {
                instance_now: ArcSwap::new(Arc::new(Vec::new())),
            }
        }
    }

    fn init_ram_client_prop_from_env() -> ClientProps {
        ClientProps::new()
            .namespace(std::env::var("NAMESPACE").unwrap_or("".to_string()))
            .server_addr(
                std::env::var("SERVER_ADDR")
                    .expect("SERVER_ADDR environment variable should be set"),
            )
    }

    fn make_service_instance(ip: &str, port: i32) -> ServiceInstance {
        let mut instance = ServiceInstance::default();
        instance.ip = ip.to_string();
        instance.port = port;
        instance
    }

    #[ignore]
    #[tokio::test]
    async fn test_naming_ram() {
        let props = init_ram_client_prop_from_env();
        let naming_client =
            NacosNamingService::new(props, Arc::new(AliyunRamAuthPlugin::default()))
                .await
                .expect("Failed to create NacosNamingService with RAM auth plugin");
        naming_client
            .register_instance(
                "test".to_owned(),
                Some("test".to_owned()),
                make_service_instance("1.1.1.1", 8080),
            )
            .await
            .expect("System time should be after UNIX_EPOCH");
        let instances = naming_client
            .get_all_instances(
                "test".to_string(),
                Some("test".to_string()),
                Vec::new(),
                false,
            )
            .await
            .expect("System time should be after UNIX_EPOCH");
        assert_eq!(1, instances.len());
        assert_eq!("1.1.1.1", instances[0].ip);
        assert_eq!(8080, instances[0].port);

        naming_client
            .deregister_instance(
                "test".to_owned(),
                Some("test".to_owned()),
                make_service_instance("1.1.1.1", 8080),
            )
            .await
            .expect("System time should be after UNIX_EPOCH");
        sleep(Duration::from_secs(5)).await;
        let instance = naming_client
            .get_all_instances(
                "test".to_string(),
                Some("test".to_string()),
                Vec::new(),
                false,
            )
            .await
            .expect("System time should be after UNIX_EPOCH");
        assert_eq!(0, instance.len());

        let listener = Arc::new(TestNamingEventListener::default());
        naming_client
            .subscribe("test".to_string(), None, Vec::new(), listener.clone())
            .await
            .expect("System time should be after UNIX_EPOCH");

        naming_client
            .register_instance(
                "test".to_owned(),
                None,
                make_service_instance("2.2.2.2", 80),
            )
            .await
            .expect("System time should be after UNIX_EPOCH");
        sleep(Duration::from_secs(5)).await;
        assert_eq!(1, listener.instance_now.load().len());

        naming_client
            .unsubscribe("test".to_string(), None, Vec::new(), listener.clone())
            .await
            .expect("System time should be after UNIX_EPOCH");
        naming_client
            .batch_register_instance(
                "test".to_owned(),
                Some("test".to_owned()),
                vec![
                    make_service_instance("2.2.2.2", 8080),
                    make_service_instance("3.3.3.3", 8080),
                ],
            )
            .await
            .expect("System time should be after UNIX_EPOCH");

        sleep(Duration::from_secs(5)).await;
        let instance = naming_client
            .get_all_instances(
                "test".to_string(),
                Some("test".to_string()),
                Vec::new(),
                false,
            )
            .await
            .expect("System time should be after UNIX_EPOCH");
        assert_eq!(2, instance.len());
    }

    struct TestConfigListener {
        current_content: ArcSwap<String>,
    }

    impl ConfigChangeListener for TestConfigListener {
        fn notify(&self, config_resp: ConfigResponse) {
            self.current_content
                .store(Arc::new(config_resp.content().to_string()))
        }
    }

    impl Default for TestConfigListener {
        fn default() -> Self {
            Self {
                current_content: Default::default(),
            }
        }
    }

    #[ignore]
    #[tokio::test]
    async fn test_config_ram() {
        let props = init_ram_client_prop_from_env();
        let config_client =
            NacosConfigService::new(props, Arc::new(AliyunRamAuthPlugin::default()), Vec::new())
                .await
                .expect("System time should be after UNIX_EPOCH");
        config_client
            .publish_config(
                "test".to_string(),
                "test".to_string(),
                "test=test".to_string(),
                Some("properties".to_string()),
            )
            .await
            .expect("System time should be after UNIX_EPOCH");
        sleep(Duration::from_secs(5)).await;

        let response = config_client
            .get_config("test".to_string(), "test".to_string())
            .await
            .expect("System time should be after UNIX_EPOCH");
        assert_eq!("test=test", response.content());
        assert_eq!("properties", response.content_type());
        assert_eq!("test", response.group());
        assert_eq!("test", response.data_id());

        config_client
            .remove_config("test".to_string(), "test".to_string())
            .await
            .expect("System time should be after UNIX_EPOCH");
        sleep(Duration::from_secs(5)).await;
        let result = config_client
            .get_config("test".to_string(), "test".to_string())
            .await;
        assert!(result.is_err());
        assert_eq!(
            "config not found: error_code=300,message=config data not exist",
            result.err().expect("Error result should exist").to_string()
        );

        let listener = Arc::new(TestConfigListener::default());
        config_client
            .add_listener("test-1".to_string(), "test".to_string(), listener.clone())
            .await
            .expect("System time should be after UNIX_EPOCH");
        config_client
            .publish_config(
                "test-1".to_string(),
                "test".to_string(),
                "test=test".to_string(),
                Some("properties".to_string()),
            )
            .await
            .expect("System time should be after UNIX_EPOCH");
        sleep(Duration::from_secs(5)).await;
        let result = config_client
            .get_config("test-1".to_string(), "test".to_string())
            .await;
        assert_eq!(
            result.expect("Config result should exist").content(),
            "test=test"
        );

        config_client
            .remove_listener("test-1".to_string(), "test".to_string(), listener.clone())
            .await
            .expect("System time should be after UNIX_EPOCH");
    }
}
