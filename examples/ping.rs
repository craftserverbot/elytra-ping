use std::process::exit;

use elytra_ping::{ping, protocol::ProtocolError, PingError};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let args = std::env::args().collect::<Vec<_>>();
    let host = args
        .get(1)
        .map(String::to_string)
        .expect("address required");
    let port = args
        .get(2)
        .map(|port| port.parse().expect("invalid port"))
        .unwrap_or(25565);

    println!("Pinging {}:{}", host, port);

    let (info, latency) = match ping((host, port)).await {
        Ok(res) => res,
        Err(PingError::Protocol {
            source: ProtocolError::JsonParse { source, json, .. },
        }) => {
            eprintln!("Invalid JSON: {source}");
            eprintln!("{json}");
            exit(1);
        }
        Err(other) => Err(other)?,
    };

    println!("Server info: {:#?}", info);
    println!("Latency: {}ms", latency.as_millis());

    Ok(())
}
