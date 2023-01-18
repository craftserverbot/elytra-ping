use std::{
    str::FromStr,
    time::{Duration, Instant},
};

use elytra_ping::{
    connect,
    parse::ServerPingInfo,
    protocol::{Frame, ProtocolError, SlpProtocol},
};
use tokio::net::lookup_host;

async fn next_frame(connection: &mut SlpProtocol) -> Result<Frame, ProtocolError> {
    Ok(connection
        .read_frame()
        .await?
        .expect("Connection closed before response was received"))
}

#[tokio::main]
async fn main() -> Result<(), ProtocolError> {
    let args = std::env::args().collect::<Vec<_>>();
    let host = args
        .get(1)
        .map(String::to_string)
        .expect("address required");
    let port = args
        .get(2)
        .map(|port| port.parse().expect("invalid port"))
        .unwrap_or(25565);

    println!("Connecting to {}:{}", host, port);
    let mut connection = connect((host, port)).await?;
    println!("Connected.");
    connection
        .write_frame(connection.create_handshake_frame())
        .await?;
    println!("Sent handshake frame");

    tokio::time::sleep(Duration::from_millis(250)).await;

    connection
        .write_frame(elytra_ping::protocol::Frame::StatusRequest)
        .await?;
    println!("Requested status");
    let frame = next_frame(&mut connection).await?;

    if let Frame::StatusResponse { json } = frame {
        let info = ServerPingInfo::from_str(&json);
        println!("Server info: {:#?}", info);
    } else {
        println!("Error: received invalid response: {:?}", frame);
    }

    let ping_time = Instant::now();
    // the payload can be anything - it will be sent back by the server
    let ping_payload: i64 = 999;

    connection
        .write_frame(Frame::PingRequest {
            payload: ping_payload,
        })
        .await?;
    println!("Checking latency");
    let frame = next_frame(&mut connection).await?;

    if let Frame::PingResponse { payload } = frame {
        assert_eq!(
            payload, ping_payload,
            "server's ping response did not match our request"
        );
        println!("Latency: {}ms", ping_time.elapsed().as_millis());
    } else {
        println!("Error: received invalid response: {:?}", frame);
    }

    connection.disconnect().await?;
    println!("Disconnected");

    Ok(())
}
