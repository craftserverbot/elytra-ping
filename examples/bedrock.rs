use std::time::Duration;

use elytra_ping::bedrock::ping;

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
        .unwrap_or(19132);

    println!("Pinging {}:{}", host, port);

    let (info, latency) = ping((host, port), Duration::from_secs(2), 3).await?;
    println!("Server info: {:#?}", info);
    println!("Latency: {}ms", latency.as_millis());

    Ok(())
}
