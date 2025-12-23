use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
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
    #[serde(default)]
    pub tls: Option<HostTls>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone, Validate)]
pub struct HostTls {
    pub cert_file: Option<String>,
    pub key_file: Option<String>,
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
        let mut last_state: HashMap<PathBuf, Option<SystemTime>> = HashMap::new();

        loop {
            sleep(Duration::from_secs(1)).await;

            let paths = {
                let config_guard = shared_config.read().await;
                config_guard.collect_tls_file_paths()
            };

            let mut should_notify = false;
            let mut next_state: HashMap<PathBuf, Option<SystemTime>> = HashMap::new();

            for path in &paths {
                let modified = file_modified_time(path).await;
                if initialized {
                    match last_state.get(path) {
                        Some(previous) if *previous == modified => {}
                        Some(_) => should_notify = true,
                        None => should_notify = true,
                    }
                }
                next_state.insert(path.clone(), modified);
            }

            if initialized {
                let removed_paths = last_state
                    .keys()
                    .filter(|p| !next_state.contains_key(*p))
                    .count();
                if removed_paths > 0 {
                    should_notify = true;
                }
            } else {
                initialized = true;
            }

            last_state = next_state;

            if should_notify
                && tls_reload_tx
                    .send(TlsReloadSignal::TlsArtifactChanged)
                    .is_err()
            {
                break;
            }
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

    pub fn host_tls_entries(&self) -> Vec<(String, PathBuf, PathBuf)> {
        self.hosts
            .iter()
            .filter_map(|(host_name, host)| {
                let tls = host.tls.as_ref()?;
                let cert = tls.cert_file.as_ref()?;
                let key = tls.key_file.as_ref()?;
                Some((
                    host_name.to_ascii_lowercase(),
                    PathBuf::from(cert),
                    PathBuf::from(key),
                ))
            })
            .collect()
    }

    pub fn collect_tls_file_paths(&self) -> Vec<PathBuf> {
        let mut unique: HashSet<PathBuf> = HashSet::new();

        unique.insert(self.resolved_ssl_cert_path());
        unique.insert(self.resolved_ssl_key_path());

        for (_, cert, key) in self.host_tls_entries() {
            unique.insert(cert);
            unique.insert(key);
        }

        unique.into_iter().collect()
    }
}
