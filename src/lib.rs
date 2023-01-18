use std::{fmt::Debug, time::Duration};
#[cfg(feature = "connect")]
use tokio::net::{lookup_host, TcpStream, ToSocketAddrs};
use tracing::{event, instrument, Level};

#[cfg(feature = "connect")]
pub mod mc_string;
#[cfg(feature = "connect")]
pub mod protocol;
#[cfg(feature = "connect")]
use crate::protocol::ProtocolError;
#[cfg(feature = "simple")]
pub use protocol::error::PingError;
#[cfg(feature = "connect")]
pub use protocol::SlpProtocol;

#[cfg(feature = "parse")]
pub mod parse;
#[cfg(feature = "parse")]
pub use parse::ServerPingInfo;

#[cfg(feature = "connect")]
#[instrument]
pub async fn connect<T>(addrs: T) -> Result<SlpProtocol, ProtocolError>
where
    T: ToSocketAddrs + std::fmt::Debug,
{
    use snafu::{Backtrace, GenerateImplicitData};

    let addrs_debug = format!("{:?}", addrs);
    // lookup_host can return multiple but we just need one so we discard the rest
    let socket_addrs = match lookup_host(addrs).await?.next() {
        Some(socket_addrs) => socket_addrs,
        None => {
            event!(Level::INFO, "DNS lookup failed for address");
            return Err(protocol::ProtocolError::DNSLookupFailed {
                address: addrs_debug,
                backtrace: Backtrace::generate(),
            });
        }
    };

    match TcpStream::connect(socket_addrs).await {
        Ok(stream) => {
            event!(Level::INFO, "Connected to SLP server");
            Ok(SlpProtocol::new(socket_addrs, stream))
        }
        Err(error) => {
            event!(Level::INFO, "Failed to connect to SLP server: {}", error);
            Err(error.into())
        }
    }
}

#[cfg(feature = "simple")]
pub async fn ping(
    addrs: impl ToSocketAddrs + Debug,
) -> Result<(ServerPingInfo, Duration), PingError> {
    let mut client = connect(addrs).await?;
    client.handshake().await?;
    let status = client.get_status().await?;
    let latency = client.get_latency().await?;
    client.disconnect().await?;
    Ok((status, latency))
}

#[cfg(feature = "simple")]
pub async fn ping_or_timeout(
    addrs: impl ToSocketAddrs + Debug,
    timeout: Duration,
) -> Result<(ServerPingInfo, Duration), PingError> {
    use snafu::{Backtrace, GenerateImplicitData};
    use tokio::{select, time};
    let sleep = time::sleep(timeout);
    tokio::pin!(sleep);

    select! {
        biased;
        info = ping(addrs) => info,
        _ = sleep => Err(PingError::Timeout { backtrace: Backtrace::generate() }),
    }
}

#[cfg(test)]
mod tests {
    use snafu::ErrorCompat;

    use super::*;

    const PING_TIMEOUT: Duration = Duration::from_secs(5);

    #[tokio::test]
    async fn hypixel() {
        let address = "mc.hypixel.net";
        let port = 25565;
        let ping = ping_or_timeout((address, port), PING_TIMEOUT).await;
        if let Err(err) = ping {
            panic!("Error: {err} ({err:?})\n{:?}", err.backtrace().unwrap());
        }
    }
}
