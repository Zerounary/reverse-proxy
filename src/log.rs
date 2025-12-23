use ansi_term::Colour::Green;

pub fn log_proxy(domain: &str, protocol: &str, ip: &str, port: &str) {
    println!(
        "{} <----> {}",
        Green.paint(domain),
        Green.paint(format!("{}://{}:{}", protocol, ip, port))
    );
}
