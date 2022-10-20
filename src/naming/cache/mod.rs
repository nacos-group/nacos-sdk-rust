use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use super::dto::ServiceInfo;

pub(crate) struct ServiceInfoHolder {
    service_info_map: Arc<Mutex<HashMap<String, Arc<ServiceInfo>>>>,

    push_empty_protection: bool,

    cache_dir: String,

    notifier_event_scope: String,
}
