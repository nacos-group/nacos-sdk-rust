use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use tokio::{sync::Mutex, time::sleep};
use tracing::{Instrument, debug, error, info, instrument, warn};

use crate::common::{
    cache::Cache,
    executor,
    remote::{
        generate_request_id,
        grpc::{NacosGrpcClient, message::GrpcResponseMessage},
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

    #[instrument(fields(group_name = group_name), skip_all)]
    pub(crate) async fn schedule_update(
        &self,
        namespace: String,
        service_name: String,
        group_name: String,
    ) {
        let task_key = ServiceInfoUpdater::update_task_key(&service_name, &group_name);
        let mut lock = self.task_map.lock().await;

        let is_exist = lock.contains_key(&task_key);
        if !is_exist {
            let update_task = ServiceInfoUpdateTask::new(
                service_name,
                namespace,
                group_name,
                self.cache.clone(),
                self.nacos_grpc_client.clone(),
                self.service_info_emitter.clone(),
            );
            update_task.start();

            let _ = lock.insert(task_key, update_task);
        }
    }

    #[instrument(fields(group_name = group_name), skip_all)]
    pub(crate) async fn stop_update(&self, service_name: String, group_name: String) {
        let task_key = ServiceInfoUpdater::update_task_key(&service_name, &group_name);
        let mut lock = self.task_map.lock().await;
        let ret = lock.remove(&task_key);
        if let Some(task) = ret {
            task.stop();
        }
    }

    /// The polling task is identified by service and group only.
    ///
    /// The Java client runs a single update task per service and group
    /// (`ServiceInfoUpdateService.scheduleUpdateIfAbsent`) and queries the
    /// server without a cluster filter, so the cache always holds the full
    /// instance set.
    fn update_task_key(service_name: &str, group_name: &str) -> String {
        ServiceInfo::get_key_without_clusters(service_name, group_name)
    }
}
struct ServiceInfoUpdateTask {
    running: Arc<AtomicBool>,
    service_name: String,
    namespace: String,
    group_name: String,
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
        cache: Arc<Cache<ServiceInfo>>,
        nacos_grpc_client: Arc<NacosGrpcClient>,
        service_info_emitter: Arc<ServiceInfoEmitter>,
    ) -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            service_name,
            namespace,
            group_name,
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

            // Query without a cluster filter so the polled result covers every
            // instance, the same way the Java client refreshes the cache.
            let request = ServiceQueryRequest {
                cluster: String::new(),
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

            let cache_key = {
                let group_name = request.group_name.as_deref().unwrap_or_default();
                let service_name = request.service_name.as_deref().unwrap_or_default();
                // The cache is keyed by service and group only, never by
                // cluster: the Java client stores the full instance set and
                // narrows it down in memory. Using a cluster-qualified key
                // here would never match the entry the observer wrote, so
                // `last_ref_time` would always be missing and every poll
                // would hit the server.
                ServiceInfo::get_key_without_clusters(service_name, group_name)
            };

            info!("{log_tag}:ServiceInfoUpdateTask started");

            while running.load(Ordering::Acquire) {
                let delay_time_millis = Duration::from_millis(
                    (delay_time << failed_count).min(ServiceInfoUpdateTask::DEFAULT_DELAY * 60),
                );
                debug!("{log_tag}:ServiceInfoUpdateTask delay sleep {delay_time_millis:?}");
                sleep(delay_time_millis).await;

                if !running.load(Ordering::Acquire) {
                    warn!("{log_tag}:ServiceInfoUpdateTask has been already stopped!");
                    break;
                }

                debug!("{log_tag}:ServiceInfoUpdateTask refreshing");

                let cached_last_ref_time = cache.get(&cache_key).map(|data| data.last_ref_time);

                let mut need_query_service_info = true;

                if let Some(cached_last_ref_time) = cached_last_ref_time {
                    let is_outdate = last_refresh_time >= (cached_last_ref_time as u64);
                    if is_outdate {
                        need_query_service_info = true;
                    } else {
                        need_query_service_info = false;
                        last_refresh_time = cached_last_ref_time as u64;
                    }
                }

                if !need_query_service_info {
                    warn!("{log_tag}:ServiceInfoUpdateTask don't need to refresh service info");
                    continue;
                }

                let request = ServiceQueryRequest {
                    request_id: Some(generate_request_id()),
                    ..request.clone()
                };

                let ret = grpc_client
                    .send_request::<ServiceQueryRequest, QueryServiceResponse>(request)
                    .in_current_span()
                    .await;
                let Ok(response) = ret else {
                    error!("{log_tag}:ServiceInfoUpdateTask occur an error: {ret:?}");
                    if failed_count < ServiceInfoUpdateTask::MAX_FAILED {
                        failed_count += 1;
                    }
                    continue;
                };
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

                service_info_emitter.emit(service_info).in_current_span().await;

                failed_count = 0;
                debug!("{log_tag}:ServiceInfoUpdateTask finish");
            }

            warn!("{log_tag}:ServiceInfoUpdateTask is stopped");
        }.in_current_span());
    }

    fn stop(&self) {
        self.running.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_task_key_excludes_clusters() {
        // The polling task and the cache agree on one key shape: service and
        // group only. A cluster-qualified key would never match the entry the
        // observer wrote, so `last_ref_time` would always be missing and every
        // poll would hit the server.
        assert_eq!(
            ServiceInfoUpdater::update_task_key("svc", "DEFAULT"),
            "DEFAULT@@svc"
        );
        assert_eq!(
            ServiceInfoUpdater::update_task_key("svc", "DEFAULT"),
            ServiceInfo::get_key_without_clusters("svc", "DEFAULT")
        );
    }
}
