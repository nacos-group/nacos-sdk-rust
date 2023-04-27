use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use tokio::{sync::Mutex, time::sleep};
use tracing::{debug, error, info, warn};

use crate::common::{
    cache::Cache,
    executor,
    remote::{
        generate_request_id,
        grpc::{message::GrpcResponseMessage, NacosGrpcClient},
    },
};

use super::{
    dto::ServiceInfo,
    message::{request::ServiceQueryRequest, response::QueryServiceResponse},
    observable::service_info_observable::ServiceInfoEmitter,
};

pub(crate) struct ServiceInfoUpdater {
    service_info_emitter: Arc<ServiceInfoEmitter>,
    cache: Arc<Cache<ServiceInfo>>,
    nacos_grpc_client: Arc<NacosGrpcClient>,
    task_map: Mutex<HashMap<String, ServiceInfoUpdateTask>>,
}

impl ServiceInfoUpdater {
    pub(crate) fn new(
        service_info_emitter: Arc<ServiceInfoEmitter>,
        cache: Arc<Cache<ServiceInfo>>,
        nacos_grpc_client: Arc<NacosGrpcClient>,
    ) -> Self {
        Self {
            service_info_emitter,
            cache,
            nacos_grpc_client,
            task_map: Mutex::new(HashMap::default()),
        }
    }

    pub(crate) async fn schedule_update(
        &self,
        namespace: String,
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
                namespace,
                group_name,
                cluster,
                self.cache.clone(),
                self.nacos_grpc_client.clone(),
                self.service_info_emitter.clone(),
            );
            update_task.start();

            let _ = lock.insert(task_key, update_task);
        }
    }

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
    cache: Arc<Cache<ServiceInfo>>,
    nacos_grpc_client: Arc<NacosGrpcClient>,
    service_info_emitter: Arc<ServiceInfoEmitter>,
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
        cache: Arc<Cache<ServiceInfo>>,
        nacos_grpc_client: Arc<NacosGrpcClient>,
        service_info_emitter: Arc<ServiceInfoEmitter>,
    ) -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            service_name,
            namespace,
            group_name,
            cluster,
            cache,
            nacos_grpc_client,
            service_info_emitter,
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

        let cache = self.cache.clone();
        let grpc_client = self.nacos_grpc_client.clone();
        let service_info_emitter = self.service_info_emitter.clone();

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

                let service_info = {
                    let group_name = request.group_name.as_deref().unwrap_or_default();
                    let service_name = request.service_name.as_deref().unwrap_or_default();
                    let cluster = &request.cluster;

                    let grouped_name =
                        ServiceInfo::get_grouped_service_name(service_name, group_name);
                    let key = ServiceInfo::get_key(&grouped_name, cluster);
                    let ret = cache.get(&key).map(|data| data.clone());
                    ret
                };

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
                    .send_request::<ServiceQueryRequest, QueryServiceResponse>(request)
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

                service_info_emitter.emit(service_info).await;

                failed_count = 0;
                info!("{log_tag}:ServiceInfoUpdateTask finish");
            }

            warn!("{log_tag}:ServiceInfoUpdateTask is stopped");
        });
    }

    fn stop(&self) {
        self.running.store(false, Ordering::Release);
    }
}
