use serde::{Deserialize, Serialize};
use std::{fs, collections::HashMap,};

type Port = u16;

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone,)]
pub struct Config {
    pub port: Port,
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
        hosts: HashMap::new(),
    });
    result
}
