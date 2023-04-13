use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use tokio::{sync::Mutex, time::sleep};
use tracing::{debug, error, info, instrument, warn, Instrument};

use crate::common::{
    executor,
    remote::{
        generate_request_id,
        grpc::{message::GrpcResponseMessage, NacosGrpcClient},
    },
};

use super::{
    dto::ServiceInfo,
    handler::ServiceInfoHolder,
    message::{request::ServiceQueryRequest, response::QueryServiceResponse},
};

pub(crate) struct ServiceInfoUpdater {
    service_info_holder: Arc<ServiceInfoHolder>,
    nacos_grpc_client: Arc<NacosGrpcClient>,
    namespace: String,
    task_map: Mutex<HashMap<String, ServiceInfoUpdateTask>>,
    client_id: String,
}

impl ServiceInfoUpdater {
    pub(crate) fn new(
        service_info_holder: Arc<ServiceInfoHolder>,
        nacos_grpc_client: Arc<NacosGrpcClient>,
        namespace: String,
        client_id: String,
    ) -> Self {
        Self {
            service_info_holder,
            nacos_grpc_client,
            namespace,
            task_map: Mutex::new(HashMap::default()),
            client_id,
        }
    }

    #[instrument(skip_all)]
    pub(crate) async fn schedule_update(
        &self,
        service_name: String,
        group_name: String,
        cluster: String,
    ) {
        let task_key = ServiceInfoUpdater::update_task_key(&service_name, &group_name, &cluster);
        let mut lock = self.task_map.lock().await;

        let is_exist = lock.contains_key(&task_key);
        if !is_exist {
            let update_task = ServiceInfoUpdateTask::new(
                service_name,
                self.namespace.clone(),
                group_name,
                cluster,
                self.service_info_holder.clone(),
                self.nacos_grpc_client.clone(),
            );
            update_task.start();

            let _ = lock.insert(task_key, update_task);
        }
    }

    #[instrument(fields(client_id = &self.client_id), skip_all)]
    pub(crate) async fn stop_update(
        &self,
        service_name: String,
        group_name: String,
        cluster: String,
    ) {
        let task_key = ServiceInfoUpdater::update_task_key(&service_name, &group_name, &cluster);
        let mut lock = self.task_map.lock().await;
        let ret = lock.remove(&task_key);
        if let Some(task) = ret {
            task.stop();
        }
    }

    fn update_task_key(service_name: &str, group_name: &str, cluster: &str) -> String {
        let group_service_name = ServiceInfo::get_grouped_service_name(service_name, group_name);
        ServiceInfo::get_key(&group_service_name, cluster)
    }
}

struct ServiceInfoUpdateTask {
    running: Arc<AtomicBool>,
    service_name: String,
    namespace: String,
    group_name: String,
    cluster: String,
    service_info_holder: Arc<ServiceInfoHolder>,
    nacos_grpc_client: Arc<NacosGrpcClient>,
}

impl ServiceInfoUpdateTask {
    const DEFAULT_DELAY: u64 = 1000;
    const DEFAULT_UPDATE_CACHE_TIME_MULTIPLE: u8 = 6;
    const MAX_FAILED: u8 = 6;

    fn new(
        service_name: String,
        namespace: String,
        group_name: String,
        cluster: String,
        service_info_holder: Arc<ServiceInfoHolder>,
        nacos_grpc_client: Arc<NacosGrpcClient>,
    ) -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            service_name,
            namespace,
            group_name,
            cluster,
            service_info_holder,
            nacos_grpc_client,
        }
    }

    fn start(&self) {
        let running = self.running.clone();
        if self.running.load(Ordering::Acquire) {
            return;
        }
        self.running.store(true, Ordering::Release);

        let cluster = self.cluster.clone();
        let group_name = self.group_name.clone();
        let namespace = self.namespace.clone();
        let service_name = self.service_name.clone();

        let service_info_holder = self.service_info_holder.clone();
        let grpc_client = self.nacos_grpc_client.clone();

        executor::spawn(async move {
            let mut delay_time = ServiceInfoUpdateTask::DEFAULT_DELAY;

            let mut last_refresh_time = u64::MAX;

            let mut failed_count = 0;

            let request = ServiceQueryRequest {
                cluster,
                group_name: Some(group_name),
                healthy_only: false,
                udp_port: 0,
                namespace: Some(namespace),
                service_name: Some(service_name),
                ..Default::default()
            };

            let log_tag = format!(
                "{}:{}:{}:{}",
                request.namespace.as_deref().unwrap_or_default(),
                request.group_name.as_deref().unwrap_or_default(),
                request.service_name.as_deref().unwrap_or_default(),
                request.cluster
            );

            info!("{log_tag}:ServiceInfoUpdateTask started ");

            while running.load(Ordering::Acquire) {
                let delay_time_millis = Duration::from_millis(
                    (delay_time << failed_count).min(ServiceInfoUpdateTask::DEFAULT_DELAY * 60),
                );
                sleep(delay_time_millis).await;

                if !running.load(Ordering::Acquire) {
                    warn!("{log_tag}:ServiceInfoUpdateTask has been already stopped!");
                    break;
                }

                info!("{log_tag}:ServiceInfoUpdateTask refreshing");

                let service_info = service_info_holder
                    .get_service_info(
                        request.group_name.as_deref().unwrap_or_default(),
                        request.service_name.as_deref().unwrap_or_default(),
                        &request.cluster,
                    )
                    .await;

                let mut need_query_service_info = true;

                if let Some(service_info) = service_info {
                    let is_outdate = last_refresh_time >= (service_info.last_ref_time as u64);
                    if is_outdate {
                        need_query_service_info = true;
                    } else {
                        need_query_service_info = false;
                        last_refresh_time = service_info.last_ref_time as u64;
                    }
                }

                if !need_query_service_info {
                    debug!("{log_tag}:ServiceInfoUpdateTask don't need to refresh service info");
                    continue;
                }

                let mut request = request.clone();
                request.request_id = Some(generate_request_id());

                let ret = grpc_client
                    .unary_call_async::<ServiceQueryRequest, QueryServiceResponse>(request)
                    .await;
                if let Err(e) = ret {
                    error!("{log_tag}:ServiceInfoUpdateTask occur an error: {e:?}");
                    if failed_count < ServiceInfoUpdateTask::MAX_FAILED {
                        failed_count += 1;
                    }
                    continue;
                }
                let response = ret.unwrap();
                debug!("{log_tag}:ServiceInfoUpdateTask query service info response: {response:?}");
                if !response.is_success() {
                    let result_code = response.result_code;
                    let error_code = response.error_code;
                    let ret_message = response.message.unwrap_or_default();
                    error!("{log_tag}:ServiceInfoUpdateTask query services failed: resultCode: {result_code}, errorCode:{error_code}, message:{ret_message}");
                    if failed_count < ServiceInfoUpdateTask::MAX_FAILED {
                        failed_count += 1;
                    }
                    continue;
                }
                let service_info = response.service_info;
                last_refresh_time = service_info.last_ref_time as u64;
                delay_time = (service_info.cache_millis
                    * (ServiceInfoUpdateTask::DEFAULT_UPDATE_CACHE_TIME_MULTIPLE as i64))
                    as u64;

                service_info_holder.process_service_info(service_info).await;

                failed_count = 0;
                info!("{log_tag}:ServiceInfoUpdateTask finish");
            }

            warn!("{log_tag}:ServiceInfoUpdateTask is stopped");
        }.in_current_span());
    }

    fn stop(&self) {
        self.running.store(false, Ordering::Release);
    }
}
