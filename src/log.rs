use ansi_term::Colour::{Red, Green, Yellow, Blue, White};

pub fn log_proxy(domain: &str, ip: &str, port: &str){
    println!("{} <--http--> {}", Green.paint(domain), Green.paint(format!("{}:{}", ip, port)));
}