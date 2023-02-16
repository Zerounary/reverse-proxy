use serde::{Deserialize, Serialize};
use std::{fs, collections::HashMap,};

type Port = u16;

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone,)]
pub struct Config {
    pub port: Port,
    pub ssl: bool,
    pub ssl_port: Port,
    pub ssl_key_file: String,
    pub ssl_cert_file: String,
    pub hosts: HashMap<String, Host>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone,)]
pub struct Host {
    pub ip: String,
    pub protocol: String,
    pub port: Port,
}

pub fn read_yaml_file(yaml_path: &str) -> Config {
    let yaml_content =
        fs::read_to_string(yaml_path).ok().unwrap_or_default();
    let result: Config = serde_yaml::from_str(&yaml_content).ok().unwrap_or(Config {
        port: 80,
        ssl_port: 443,
        hosts: HashMap::new(),
        ssl: false,
        ssl_key_file: String::default(),
        ssl_cert_file: String::default(),
    });
    result
}
