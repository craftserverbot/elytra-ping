//! Through the `elytra_ping` crate, programs can retrieve the status and information about Minecraft Java Edition or Bedrock Edition servers.
//!
//! This crate can interact with servers running Minecraft Java 1.7+ or Bedrock. If you have the server's address and port, Elytra Ping can retrieve metadata like the server's description, player count, vendor, and icon. The (lack of the) server's response can also be used to infer whether it is online and usable or not.
//!
//! ## Usage
//!
//! Use the [`ping_or_timeout`] function to retrieve a Java Edition server's status and latency, aborting if it takes too long.
//!
//! ```
//! # use std::time::Duration;
//! # #[tokio::main]
//! # async fn main() {
//! let (ping_info, latency) = elytra_ping::ping_or_timeout(
//!     ("mc.hypixel.net".to_string(), 25565),
//!     Duration::from_secs(1),
//! ).await.unwrap();
//! println!("{ping_info:#?}, {latency:?}");
//! # }
//! ```
//!
//! Use the [`bedrock::ping`] function to retrieve a Bedrock Edition server's status and latency, specifying the number of retries
//! if the operation fails initially and the amount of time to spend before timing out on a single retry.
//!
//! ```
//! # use std::time::Duration;
//! # #[tokio::main]
//! # async fn main() {
//! let retry_timeout = Duration::from_secs(2);
//! let retries = 3;
//! let (ping_info, latency) = elytra_ping::bedrock::ping(
//!     ("play.cubecraft.net".to_string(), 19132),
//!     retry_timeout,
//!     retries,
//! ).await.unwrap();
//! println!("{ping_info:#?}, {latency:?}");
//! // BedrockServerInfo {
//! //     online_players: 10077,
//! //     max_players: 55000,
//! //     game_mode: Some(
//! //         "Survival",
//! //     ),
//! //     ...
//! // }, 83ms
//! # }
//! ```
//!
//! ### Advanced API
//!
//! Elytra Ping can be customized for advanced usage through the `SlpProtocol` API,
//! which provides an interface for sending and receiving packets to and from Java Edition servers.
//!
//! ```
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let addrs = ("mc.hypixel.net".to_string(), 25565);
//! let mut client: elytra_ping::SlpProtocol = elytra_ping::connect(addrs).await?;
//!
//! // Set up our connection to receive a status packet
//! client.handshake().await?;
//! client.write_frame(elytra_ping::protocol::Frame::StatusRequest).await?;
//!
//! // Read the status packet from the server
//! let frame: elytra_ping::protocol::Frame = client
//!     .read_frame(None)
//!     .await?
//!     .expect("connection closed by server");
//!
//! let status: String = match frame {
//!     elytra_ping::protocol::Frame::StatusResponse { json } => json,
//!     _ => panic!("expected status packet"),
//! };
//!
//! println!("Status: {}", status);
//!
//! client.disconnect().await?;
//! # Ok(())
//! # }
//! ```
use snafu::{Backtrace, Snafu};
use std::time::Duration;

#[cfg(feature = "java_connect")]
pub mod mc_string;
#[cfg(feature = "java_connect")]
pub mod protocol;
#[cfg(feature = "java_connect")]
pub use crate::protocol::connect;
#[cfg(feature = "java_connect")]
pub use protocol::SlpProtocol;

#[cfg(feature = "java_parse")]
pub mod parse;
#[cfg(feature = "java_parse")]
pub use parse::JavaServerInfo;

#[cfg(feature = "bedrock")]
pub mod bedrock;

#[cfg(feature = "simple")]
#[derive(Snafu, Debug)]
pub enum PingError {
    /// Connection failed.
    #[snafu(display("Connection failed: {source}"), context(false))]
    Protocol {
        #[snafu(backtrace)]
        source: crate::protocol::ProtocolError,
    },
    /// The connection did not finish in time.
    Timeout { backtrace: Backtrace },
}

#[cfg(feature = "simple")]
pub async fn ping(addrs: (String, u16)) -> Result<(JavaServerInfo, Duration), PingError> {
    let mut client = connect(addrs).await?;
    client.handshake().await?;
    let status = client.get_status().await?;
    let latency = client.get_latency().await?;
    client.disconnect().await?;
    Ok((status, latency))
}

#[cfg(feature = "simple")]
pub async fn ping_or_timeout(
    addrs: (String, u16),
    timeout: Duration,
) -> Result<(JavaServerInfo, Duration), PingError> {
    use tokio::{select, time};
    let sleep = time::sleep(timeout);
    tokio::pin!(sleep);

    select! {
        biased;
        info = ping(addrs) => info,
        _ = sleep => TimeoutSnafu.fail(),
    }
}

#[cfg(test)]
mod tests {
    use snafu::ErrorCompat;

    use super::*;

    #[ctor::ctor]
    fn init_logger() {
        use tracing_subscriber::EnvFilter;

        tracing_subscriber::fmt()
            .pretty()
            .with_env_filter(EnvFilter::from_default_env())
            .init();
    }

    const PING_TIMEOUT: Duration = Duration::from_secs(5);

    #[tokio::test]
    async fn hypixel() {
        let address = "mc.hypixel.net".to_owned();
        let port = 25565;
        let ping = ping_or_timeout((address, port), PING_TIMEOUT).await;
        match ping {
            Err(err) => panic!("Error: {err} ({err:?})\n{:?}", err.backtrace().unwrap()),
            Ok(ping) => println!("{:#?} in {:?}", ping.0, ping.1),
        }
    }

    #[tokio::test]
    async fn hypixel_bare() {
        let address = "hypixel.net".to_owned();
        let port = 25565;
        let ping = ping_or_timeout((address, port), PING_TIMEOUT).await;
        match ping {
            Err(err) => panic!("Error: {err} ({err:?})\n{:?}", err.backtrace().unwrap()),
            Ok(ping) => println!("{:#?} in {:?}", ping.0, ping.1),
        }
    }

    #[tokio::test]
    #[ignore = "mineplex is shut down"]
    async fn mineplex() {
        let address = "us.mineplex.com".to_owned();
        let port = 25565;
        let ping = ping_or_timeout((address, port), PING_TIMEOUT).await;
        match ping {
            Err(err) => panic!("Error: {err} ({err:?})\n{:?}", err.backtrace().unwrap()),
            Ok(ping) => println!("{:#?} in {:?}", ping.0, ping.1),
        }
    }

    #[tokio::test]
    #[ignore = "mineplex is shut down"]
    async fn mineplex_bare() {
        let address = "mineplex.com".to_owned();
        let port = 25565;
        let ping = ping_or_timeout((address, port), PING_TIMEOUT).await;
        match ping {
            Err(err) => panic!("Error: {err} ({err:?})\n{:?}", err.backtrace().unwrap()),
            Ok(ping) => println!("{:#?} in {:?}", ping.0, ping.1),
        }
    }
}
