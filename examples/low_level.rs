use std::{
    net::{IpAddr, SocketAddr},
    str::FromStr,
    time::Duration,
};

use minecraft_slp::{connect, parse::ServerPingInfo, protocol::SlpError, Frame};
use tokio::net::lookup_host;

#[tokio::main]
async fn main() -> Result<(), SlpError> {
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

    println!("Connecting to {}", addr);
    let mut connection = connect(&addr).await?;
    println!("Connected.");
    connection
        .write_frame(connection.create_handshake_frame())
        .await?;
    println!("Sent handshake frame");

    tokio::time::sleep(Duration::from_millis(250)).await;

    connection
        .write_frame(minecraft_slp::protocol::Frame::StatusRequest)
        .await?;
    println!("Requested status");
    let frame = connection
        .read_frame()
        .await?
        .expect("Connection closed before response was received");

    if let Frame::StatusResponse { json } = frame {
        let info = ServerPingInfo::from_str(&json);
        println!("Server info: {:#?}", info);
    } else {
        println!("Error: received unexpected frame: {:?}", frame);
    }

    connection.disconnect().await?;
    println!("Disconnected");

    Ok(())
}
