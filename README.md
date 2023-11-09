# Elytra Ping

> Easily ping and get the status of Minecraft: Java Edition servers

[![CI Status](https://github.com/doinkythederp/elytra-ping/actions/workflows/build.yml/badge.svg)](https://github.com/doinkythederp/elytra-ping/actions/workflows/build.yml)

## Installation

```sh
cargo add elytra-ping
```

## Overview

This Rust library lets you fetch information from online Minecraft: Java Edition servers the [same way](https://wiki.vg/Server_List_Ping) that the official game client does.

Elytra Ping will give you metadata like the server's description, player count, brand, and icon. The (lack of the) server's response can also be used to infer whether it is online or not.

## Usage

The simplest way to start getting data is to use the `ping` function, which takes a hostname and port and produces the server's info and latency:

```rs
async fn ping(addrs: (String, u16)) -> Result<(ServerPingInfo, Duration), PingError>
```

It works on most servers running Minecraft 1.7 or later:

```rs
let addrs = ("mc.hypixel.net", 25565);
let ping_info = ping(addrs).await?;
// -> ServerPingInfo { players: 34428 / 100000, ... }
```

### Time limits

You can limit the time spent pinging by using the `ping_or_timeout` function, which takes a `Duration` as a second argument:

```rs
let timeout_after = Duration::from_ms(1);
let addrs = ("mc.hypixel.net", 25565);

let ping_info = ping_or_timeout(addrs, timeout_after).await;
// -> Err(PingError::Timeout)
```

### Lower-level API

Elytra Ping can be customized further by using the `connect` function, which simply establishes a connection to the server and provides you with an interface for sending and receiving packets.

For instance, to get the server info as a JSON string rather than a struct:

```rs
let addrs = ("mc.hypixel.net", 25565);
let client: SlpProtocol = connect(addrs).await?;

// Set up our connection to recieve a status packet
client.handshake().await?;
client.write_frame(Frame::StatusRequest).await?;

// Read the status packet from the server
let frame: Frame = client
    .read_frame(None)
    .await?
    .expect("connection closed by server");

let status: String = match frame {
    Frame::StatusResponse { json } => json,
    _ => panic!("expected status packet"),
};

println!("Status: {}", status);

// It's important to run the `disconnect`
// method when you're done to clean up the connection.
connection.disconnect().await?;
```

The low-level API can also parse packets meant to be sent to the server. Because the handshake packet ID and the status request packet ID are the same, it's necessary to keep some kind of state to decide which makes sense in the particular situation.

```rs
let mut server = SlpProtocol::new(server_hostname, server_port, tcp_stream);
let mut server_state = ServerState::Handshake;

loop {
    let frame: Frame = server
        .read_frame(server_state)
        .await?
        .expect("client lost connection"); server_state = ServerState::Status;

    match frame {
        Frame::Handshake { status, protocol, ..} => {
            // todo: ensure suitable values for status and protocol
            server_state = ServerState::Status;
        }

        Frame::StatusRequest => {
            server.write_frame(StatusResponse { json: "{...}" }).await?;
        }
    }
}
```
