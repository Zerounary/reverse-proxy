use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs};
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
