# Elytra Ping

> Easily retrieve the status of running Minecraft servers

[![CI Status](https://github.com/doinkythederp/elytra-ping/actions/workflows/build.yml/badge.svg)](https://github.com/doinkythederp/elytra-ping/actions/workflows/build.yml)

This crate can interact with on servers running Minecraft 1.7 or later. If you have the server's address and port, Elytra Ping can retrieve metadata like the server's description, player count, vendor, and icon. The (lack of the) server's response can also be used to infer whether it is online and usable or not.

## Install

```sh
cargo add elytra-ping
```

## Usage

Use the `ping_or_timeout` function to retrieve a server's status and latency, aborting if it takes too long.

```rs
let (ping_info, latency) = elytra_ping::ping_or_timeout(
    ("mc.hypixel.net".to_string(), 25565),
    std::time::Duration::from_secs(1),
).await.unwrap();
println!("{ping_info:#?}, {latency:?}");
// JavaServerInfo {
//     players: 31757 of 200000,
//     ...
// }, 62.84175ms
```

### Bedrock Edition

```rs
let (ping_info, latency) = elytra_ping::bedrock::ping(
    ("play.cubecraft.net".to_string(), 19132),
    std::time::Duration::from_secs(1),
    3
).await.unwrap();
println!("{ping_info:#?}, {latency:?}");
// BedrockServerInfo {
//     online_players: 10077,
//     max_players: 55000,
//     game_mode: Some(
//         "Survival",
//     ),
//     ...
// }, 83ms
```

### Advanced API

Elytra Ping can be customized for advanced usage by using the `SlpProtocol` API, which provides an interface for sending and receiving packets.

```rs
let addrs = ("mc.hypixel.net".to_string(), 25565);
let mut client: elytra_ping::SlpProtocol = elytra_ping::connect(addrs).await?;

// Set up our connection to receive a status packet
client.handshake().await?;
client.write_frame(elytra_ping::protocol::Frame::StatusRequest).await?;

// Read the status packet from the server
let frame: elytra_ping::protocol::Frame = client
    .read_frame(None)
    .await?
    .expect("connection closed by server");

let status: String = match frame {
    elytra_ping::protocol::Frame::StatusResponse { json } => json,
    _ => panic!("expected status packet"),
};

println!("Status: {}", status);

client.disconnect().await?;
```