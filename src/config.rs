use serde::{Deserialize, Serialize};
use std::{fs, collections::HashMap,};

type Port = u16;

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone,)]
pub struct Config {
    pub port: Option<Port>,
    pub ssl: Option<bool>,
    pub ssl_port: Option<Port>,
    pub ssl_key_file: Option<String>,
    pub ssl_cert_file: Option<String>,
    pub hosts: HashMap<String, Host>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone,)]
pub struct Host {
    pub ip: String,
    pub port: Port,
    pub protocol: String,
}

pub fn read_yaml_file(yaml_path: &str) -> Config {
    let yaml_content =
        fs::read_to_string(yaml_path).ok().unwrap_or_default();
    let result: Config = serde_yaml::from_str(&yaml_content).ok().unwrap_or(Config {
        port: Some(80),
        ssl_port: Some(443),
        hosts: HashMap::new(),
        ssl: Some(false),
        ssl_key_file: Some(String::from("./ssl/private.pem")),
        ssl_cert_file: Some(String::from("./ssl/certificate.crt")),
    });
    result
}
