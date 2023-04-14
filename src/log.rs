use ansi_term::Colour::{Red, Green, Yellow, Blue, White};

pub fn log_proxy(domain: &str, protocol: &str, ip: &str, port: &str){
    println!("{} <----> {}", Green.paint(domain), Green.paint(format!("{}://{}:{}", protocol, ip, port)));
}