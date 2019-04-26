use log;
use std::net::Ipv4Addr;
use std::env;
use std::fs::File;
use std::io::BufReader;

use serde::{Deserialize};

mod text;
mod dom;
mod transport;
mod text_layout;
mod event_loop;

#[derive(Deserialize)]
struct PortFileContent {
    #[serde(rename = "httpPort")]
    http_port: u16,
    #[serde(rename = "tcpPort")]
    tcp_port: u16
}

fn main() -> std::io::Result<()> {
    env_logger::init();
    let args: Vec<String> = env::args().collect();
    if args.len() >= 2 {
        let port_file = &args[1];
        let f = File::open(port_file).expect("No port file");
        let content: PortFileContent = serde_json::from_reader(BufReader::new(f)).unwrap();
        event_loop::run_event_loop((Ipv4Addr::new(127, 0, 0, 1), content.tcp_port));
    }
    return Ok(());
}