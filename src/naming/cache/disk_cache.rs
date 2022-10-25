use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
};

use tracing::{error, warn};

use crate::naming::dto::ServiceInfo;

pub fn write(service_info: Arc<ServiceInfo>, dir: String) {
    let json_data = service_info.to_json_str();
    if json_data.is_none() {
        warn!("write cache failed. service can't be serialize to json string");
        return;
    }
    let json_data = json_data.unwrap();

    let is_ok = make_sure_cache_dir(&dir);
    if !is_ok {
        return;
    }
    let file_name = get_cache_file_name(&service_info);

    let path_buf: PathBuf = [&dir, &file_name].iter().collect();

    let ret = File::create(path_buf.as_path());
    if let Err(e) = ret {
        error!("create file failed. {:?}", e);
        return;
    }
    let mut file = ret.unwrap();
    let ret = file.write_all(json_data.as_bytes());
    if let Err(e) = ret {
        error!("write cache data to file failed. {:?}", e);
    }
}

fn get_cache_file_name(service_info: &Arc<ServiceInfo>) -> String {
    let group_name = service_info.get_grouped_service_name();
    let url_encode_group_name: String =
        url::form_urlencoded::byte_serialize(group_name.as_bytes()).collect();
    ServiceInfo::get_key(&url_encode_group_name, &service_info.clusters)
}

fn make_sure_cache_dir(path: &str) -> bool {
    let ret = Path::new(path).try_exists();
    if let Err(e) = ret {
        error!("judgment cache dir: {}, error: {}", path, e);
        let ret = fs::create_dir_all(path);
        if let Err(e) = ret {
            error!("create cache dir: {}, error: {}", path, e);
            return false;
        } else {
            return true;
        }
    }
    let ret = ret.unwrap();
    if ret {
        return true;
    }
    let ret = fs::create_dir_all(path);
    if let Err(e) = ret {
        error!("create cache dir: {}, error: {}", path, e);
        return false;
    }
    true
}
