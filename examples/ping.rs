use minecraft_slp::ping;
use tokio::net::lookup_host;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    let host = args
        .get(1)
        .map(String::to_string)
        .expect("address required");
    let port = args
        .get(2)
        .map(|port| port.parse().expect("invalid port"))
        .unwrap_or(25565);

    let addr = lookup_host(format!("{}:{}", host, port))
        .await?
        .next()
        .expect("dns lookup failed");

    println!("Pinging {}", addr);

    let (info, latency) = ping(&addr).await?;
    println!("Server info: {:#?}", info);
    println!("Latency: {}ms", latency.as_millis());

    Ok(())
}
