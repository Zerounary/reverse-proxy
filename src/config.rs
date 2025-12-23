use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};
use tokio::{
    fs as tokio_fs,
    sync::{watch, RwLock},
    time::{sleep, Duration},
};
use validator::{Validate, ValidationError};

type Port = u16;

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone, Validate)]
pub struct Config {
    pub port: Option<Port>,
    pub ssl: Option<bool>,
    pub ssl_port: Option<Port>,
    pub ssl_key_file: Option<String>,
    pub ssl_cert_file: Option<String>,
    pub hosts: HashMap<String, Host>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone, Validate)]
pub struct Host {
    pub ip: String,
    pub port: Port,
    #[validate(custom(function = "protocol_check"))]
    pub protocol: String,
}

pub type SharedConfig = Arc<RwLock<Config>>;

pub fn read_yaml_file(yaml_path: &str) -> Config {
    let yaml_content = fs::read_to_string(yaml_path).ok().unwrap_or_default();
    let result: Config = serde_yaml::from_str(&yaml_content).ok().unwrap_or(Config {
        port: Some(80),
        ssl_port: Some(443),
        hosts: HashMap::new(),
        ssl: Some(false),
        ssl_key_file: Some(String::from("./ssl/private.pem")),
        ssl_cert_file: Some(String::from("./ssl/certificate.crt")),
    });
    match result.validate() {
        Ok(_) => {
            for host in result.hosts.values() {
                match host.validate() {
                    Err(e) => panic!("{}", e),
                    _ => (),
                }
            }
            return result;
        }
        Err(e) => panic!("{}", e),
    }
}

pub fn protocol_check(value: &str) -> Result<(), ValidationError> {
    if vec!["http", "https", "ws", "wss"].contains(&value) {
        Ok(())
    } else {
        Err(ValidationError::new(
            "protocol only support 'http', 'https', 'ws', 'wss'",
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TlsReloadSignal {
    ConfigChanged,
    TlsArtifactChanged,
}

pub fn spawn_hot_reload_task(
    path: PathBuf,
    shared_config: SharedConfig,
    tls_reload_tx: watch::Sender<TlsReloadSignal>,
) {
    tokio::spawn(async move {
        let mut last_modified = file_modified_time(&path).await;

        loop {
            sleep(Duration::from_secs(1)).await;

            let Some(current_modified) = file_modified_time(&path).await else {
                continue;
            };

            let should_reload = match last_modified {
                Some(previous) => current_modified != previous,
                None => true,
            };

            if !should_reload {
                continue;
            }

            let path_string = path.to_string_lossy().to_string();
            let updated_config = read_yaml_file(&path_string);
            {
                let mut config_guard = shared_config.write().await;
                *config_guard = updated_config;
            }
            last_modified = Some(current_modified);
            let _ = tls_reload_tx.send(TlsReloadSignal::ConfigChanged);
            println!("Config hot reloaded from {}", path_string);
        }
    });
}

pub fn spawn_tls_watch_task(
    shared_config: SharedConfig,
    tls_reload_tx: watch::Sender<TlsReloadSignal>,
) {
    tokio::spawn(async move {
        let mut initialized = false;
        let mut last_cert_path: Option<PathBuf> = None;
        let mut last_cert_modified: Option<SystemTime> = None;
        let mut last_key_path: Option<PathBuf> = None;
        let mut last_key_modified: Option<SystemTime> = None;

        loop {
            sleep(Duration::from_secs(1)).await;

            let (cert_path, key_path) = {
                let config_guard = shared_config.read().await;
                (
                    config_guard.resolved_ssl_cert_path(),
                    config_guard.resolved_ssl_key_path(),
                )
            };

            let cert_modified = file_modified_time(&cert_path).await;
            let key_modified = file_modified_time(&key_path).await;

            let mut should_notify = false;
            if initialized {
                if last_cert_path.as_ref() != Some(&cert_path)
                    || last_cert_modified != cert_modified
                {
                    should_notify = true;
                }

                if last_key_path.as_ref() != Some(&key_path) || last_key_modified != key_modified {
                    should_notify = true;
                }
            } else {
                initialized = true;
            }

            if should_notify
                && tls_reload_tx
                    .send(TlsReloadSignal::TlsArtifactChanged)
                    .is_err()
            {
                break;
            } else if should_notify {
                // already notified
            }

            last_cert_path = Some(cert_path);
            last_cert_modified = cert_modified;
            last_key_path = Some(key_path);
            last_key_modified = key_modified;
        }
    });
}

async fn file_modified_time(path: &Path) -> Option<SystemTime> {
    tokio_fs::metadata(path).await.ok()?.modified().ok()
}

impl Config {
    pub fn resolved_http_port(&self) -> Port {
        self.port.unwrap_or(80)
    }

    pub fn resolved_ssl_port(&self) -> Port {
        self.ssl_port.unwrap_or(443)
    }

    pub fn resolved_ssl_cert_path(&self) -> PathBuf {
        PathBuf::from(
            self.ssl_cert_file
                .clone()
                .unwrap_or_else(|| "./ssl/certificate.crt".to_string()),
        )
    }

    pub fn resolved_ssl_key_path(&self) -> PathBuf {
        PathBuf::from(
            self.ssl_key_file
                .clone()
                .unwrap_or_else(|| "./ssl/private.pem".to_string()),
        )
    }

    pub fn ssl_enabled(&self) -> bool {
        self.ssl.unwrap_or(false)
    }
}
