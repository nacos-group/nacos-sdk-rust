use std::{borrow::Cow, collections::HashMap, io::BufReader, path::PathBuf};

use async_trait::async_trait;
use serde::de;
use tokio::{
    fs::{OpenOptions, remove_file},
    io::AsyncWriteExt,
};
use tracing::{debug, info, instrument, warn};

use super::Store;

pub(crate) struct DiskStore {
    disk_path: PathBuf,
}

impl DiskStore {
    pub(crate) fn new(disk_path: PathBuf) -> Self {
        Self { disk_path }
    }
}

#[async_trait]
impl<V> Store<V> for DiskStore
where
    V: de::DeserializeOwned,
{
    fn name(&self) -> Cow<'_, str> {
        Cow::from("disk-store")
    }

    fn load(&mut self) -> HashMap<String, V> {
        let mut default_map = HashMap::default();

        let disk_path_display = self.disk_path.display();
        info!("DiskStore load the disk_path={}", disk_path_display);

        if !self.disk_path.exists() {
            warn!("disk path is not exists, trying create it.");
            let ret = std::fs::create_dir_all(&self.disk_path);
            if let Err(e) = ret {
                warn!("create directory {} failed {}.", disk_path_display, e);
                return default_map;
            }
        }

        if !self.disk_path.is_dir() {
            warn!("disk path is not a directory. {}", disk_path_display);
            return default_map;
        }

        let dir_iter = std::fs::read_dir(&self.disk_path);
        if let Err(e) = dir_iter {
            warn!(
                "read directory {} failed {}, trying create a empty directory",
                disk_path_display, e
            );
            return default_map;
        }

        let dir_iter =
            dir_iter.expect("Directory iterator should exist after checking it's not error");

        for entry in dir_iter {
            if entry.is_err() {
                // skip
                debug!("entry error");
                continue;
            }

            let entry = entry.expect("Directory entry should exist after checking it's not error");
            let path = entry.path();
            if path.is_dir() {
                // directory skip
                continue;
            }
            let file = std::fs::File::open(&path);

            if let Err(e) = file {
                warn!("cannot open file {}, {}", path.display(), e);
                continue;
            }
            let file = file.expect("File should exist after checking it's not error");
            let reader = std::io::BufReader::new(file);

            let ret = serde_json::from_reader::<BufReader<std::fs::File>, V>(reader);
            if let Err(e) = ret {
                warn!("cannot deserialize {}, {}.", path.display(), e);
                continue;
            }

            let value = ret.expect("Deserialized value should exist after checking it's not error");
            let key = path.file_name();
            if key.is_none() {
                // skip
                continue;
            }

            let key = key.expect("File name should exist after checking it's not none");
            let key: String = key.to_string_lossy().into();

            default_map.insert(key, value);
        }

        default_map
    }

    #[instrument(fields(key = key), skip_all)]
    async fn save(&mut self, key: &str, value: Vec<u8>) {
        let mut write_path = PathBuf::from(&self.disk_path);
        write_path.push(key);

        let write_path_display = write_path.display();
        debug!("save {}", write_path_display);

        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(write_path.as_path())
            .await;

        if let Err(e) = file {
            debug!("open file {} failed {}.", write_path_display, e);
            return;
        }

        let mut file = file.expect("File should exist after checking it's not error");
        let ret = file.write(&value).await;

        if let Err(e) = ret {
            let str = String::from_utf8(value);
            if let Ok(item) = str {
                warn!(
                    "the data {} cannot write to file {}, {}.",
                    item, write_path_display, e
                );
            } else {
                warn!(
                    "write to file {} failed {} and the data cannot convert to string.",
                    write_path_display, e
                );
            }
            return;
        }
    }

    #[instrument(fields(key = key), skip_all)]
    async fn remove(&mut self, key: &str) {
        let mut delete_path = PathBuf::from(&self.disk_path);
        delete_path.push(key);

        let delete_path_display = delete_path.display();
        debug!("remove {}", delete_path_display);

        if !delete_path.exists() {
            return;
        }

        let ret = remove_file(&delete_path).await;

        if let Err(e) = ret {
            warn!("delete file {} failed {}.", delete_path_display, e);
        }
    }
}
