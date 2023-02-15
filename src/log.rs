use ansi_term::Colour::{Red, Green, Yellow, Blue, White};

pub fn log_proxy(domain: &str, protocol: &str, ip: &str, port: &str){
    println!("{} <--{}--> {}", Green.paint(domain), Yellow.paint(protocol), Green.paint(format!("{}:{}", ip, port)));
}