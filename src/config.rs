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
    sync::RwLock,
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

pub fn spawn_hot_reload_task(path: PathBuf, shared_config: SharedConfig) {
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
            println!("Config hot reloaded from {}", path_string);
        }
    });
}

async fn file_modified_time(path: &Path) -> Option<SystemTime> {
    tokio_fs::metadata(path).await.ok()?.modified().ok()
}
