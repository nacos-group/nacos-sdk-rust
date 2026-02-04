use std::{borrow::Cow, collections::HashMap, io::BufReader, path::PathBuf};

use async_trait::async_trait;
use serde::de;
use tokio::{
    fs::{OpenOptions, remove_file, rename},
    io::AsyncWriteExt,
    time::{Duration, sleep},
};
use tracing::{debug, error, info, instrument, warn};

use super::Store;

pub(crate) struct DiskStore {
    disk_path: PathBuf,
}

impl DiskStore {
    pub(crate) fn new(disk_path: PathBuf) -> Self {
        info!(path = %disk_path.display(), "Creating DiskStore");
        Self { disk_path }
    }

    async fn try_save(
        &self,
        tmp_path: &PathBuf,
        write_path: &PathBuf,
        value: &[u8],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        debug!(tmp_path = %tmp_path.display(), "Opening temp file for write");

        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(tmp_path)
            .await?;

        let mut file = file;

        // Write data to temp file
        file.write_all(value).await?;

        // Ensure data is flushed to disk before renaming
        file.sync_all().await?;

        // Drop the file handle
        drop(file);

        // Atomic rename from temp file to target file
        rename(tmp_path, write_path).await?;

        Ok(())
    }

    #[allow(dead_code)]
    async fn try_remove(
        &self,
        path: &PathBuf,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if !path.exists() {
            return Ok(());
        }
        remove_file(path).await?;
        Ok(())
    }
}

#[async_trait]
impl<V> Store<V> for DiskStore
where
    V: de::DeserializeOwned + Send,
{
    fn name(&self) -> Cow<'_, str> {
        Cow::from("disk-store")
    }

    fn load(&self) -> HashMap<String, V>
    where
        V: de::DeserializeOwned,
    {
        let mut default_map = HashMap::default();

        let disk_path_display = self.disk_path.display();
        info!(path = %disk_path_display, "Loading cache from disk");

        if !self.disk_path.exists() {
            info!(path = %disk_path_display, "Cache directory does not exist, will create");
            if let Err(e) = std::fs::create_dir_all(&self.disk_path) {
                warn!(path = %disk_path_display, error = %e, "Failed to create cache directory");
                return default_map;
            }
            return default_map;
        }

        if !self.disk_path.is_dir() {
            error!(path = %disk_path_display, "Cache path is not a directory");
            return default_map;
        }

        let dir_iter = match std::fs::read_dir(&self.disk_path) {
            Ok(iter) => iter,
            Err(e) => {
                error!(path = %disk_path_display, error = %e, "Failed to read cache directory");
                return default_map;
            }
        };

        let mut loaded_count = 0u64;
        let mut failed_count = 0u64;

        for entry in dir_iter {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    debug!(error = %e, "Skipping invalid directory entry");
                    continue;
                }
            };

            let path = entry.path();
            if path.is_dir() {
                continue;
            }

            let filename = match path.file_name() {
                Some(f) => f.to_string_lossy().to_string(),
                None => continue,
            };

            // Skip temporary files
            if filename.ends_with(".tmp") {
                debug!(file = %filename, "Skipping temporary file");
                continue;
            }

            let file = match std::fs::File::open(&path) {
                Ok(f) => f,
                Err(e) => {
                    warn!(file = %filename, error = %e, "Failed to open cache file");
                    failed_count += 1;
                    continue;
                }
            };

            let reader = std::io::BufReader::new(file);

            match serde_json::from_reader::<BufReader<std::fs::File>, V>(reader) {
                Ok(value) => {
                    default_map.insert(filename.clone(), value);
                    loaded_count += 1;
                    debug!(file = %filename, "Loaded cache entry");
                }
                Err(e) => {
                    warn!(file = %filename, error = %e, "Failed to deserialize cache file");
                    failed_count += 1;
                }
            }
        }

        info!(
            path = %disk_path_display,
            loaded = loaded_count,
            failed = failed_count,
            "Cache loading completed"
        );

        default_map
    }

    #[instrument(fields(key = key), skip_all)]
    async fn save(&self, key: &str, value: Vec<u8>) {
        const MAX_RETRIES: u32 = 3;
        const RETRY_DELAY_MS: u64 = 100;

        let mut write_path = PathBuf::from(&self.disk_path);
        write_path.push(key);

        let write_path_display = write_path.display();
        info!(path = %write_path_display, "Saving cache entry to disk");

        // Write to a temporary file first, then rename for atomicity
        let tmp_path = write_path.with_extension("tmp");

        for attempt in 1..=MAX_RETRIES {
            match self.try_save(&tmp_path, &write_path, &value).await {
                Ok(()) => {
                    info!(path = %write_path_display, "Cache entry saved successfully");
                    return;
                }
                Err(e) => {
                    if attempt == MAX_RETRIES {
                        error!(
                            path = %write_path_display,
                            error = %e,
                            "Failed to save cache entry after all retries"
                        );
                        return;
                    }
                    warn!(
                        path = %write_path_display,
                        attempt = attempt,
                        max_retries = MAX_RETRIES,
                        error = %e,
                        "Save attempt failed, retrying..."
                    );
                    sleep(Duration::from_millis(RETRY_DELAY_MS * attempt as u64)).await;
                }
            }
        }
    }

    #[instrument(fields(key = key), skip_all)]
    async fn remove(&self, key: &str) {
        const MAX_RETRIES: u32 = 3;
        const RETRY_DELAY_MS: u64 = 100;

        let mut delete_path = PathBuf::from(&self.disk_path);
        delete_path.push(key);

        let delete_path_display = delete_path.display();
        info!(key = key, path = %delete_path_display, "Removing cache entry from disk");

        for attempt in 1..=MAX_RETRIES {
            match self.try_remove(&delete_path).await {
                Ok(()) => {
                    info!(key = key, path = %delete_path_display, "Cache entry removed successfully");
                    return;
                }
                Err(e) => {
                    if attempt == MAX_RETRIES {
                        error!(
                            key = key,
                            path = %delete_path_display,
                            error = %e,
                            "Failed to remove cache entry after all retries"
                        );
                        return;
                    }
                    warn!(
                        key = key,
                        attempt = attempt,
                        max_retries = MAX_RETRIES,
                        error = %e,
                        "Remove attempt failed, retrying..."
                    );
                    sleep(Duration::from_millis(RETRY_DELAY_MS * attempt as u64)).await;
                }
            }
        }
    }
}
