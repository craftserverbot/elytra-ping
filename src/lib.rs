use std::time::Duration;
#[cfg(feature = "connect")]
use tokio::net::{lookup_host, TcpStream};
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
pub async fn connect(mut addrs: (String, u16)) -> Result<SlpProtocol, ProtocolError> {
    use snafu::{Backtrace, GenerateImplicitData};
    use tracing::debug;
    use trust_dns_resolver::TokioAsyncResolver;

    let resolver = TokioAsyncResolver::tokio_from_system_conf()?;
    if let Ok(records) = resolver
        .srv_lookup(format!("_minecraft._tcp.{}", addrs.0))
        .await
    {
        if let Some(record) = records.iter().next() {
            let record = record.target().to_utf8();
            debug!("Found SRV record: {} -> {}", addrs.0, record);
            addrs.0 = record;
        }
    }

    // lookup_host can return multiple but we just need one so we discard the rest
    let socket_addrs = match lookup_host(addrs.clone()).await?.next() {
        Some(socket_addrs) => socket_addrs,
        None => {
            event!(Level::INFO, "DNS lookup failed for address");
            return Err(protocol::ProtocolError::DNSLookupFailed {
                address: format!("{:?}", addrs),
                backtrace: Backtrace::generate(),
            });
        }
    };

    match TcpStream::connect(socket_addrs).await {
        Ok(stream) => {
            event!(Level::INFO, "Connected to SLP server");
            Ok(SlpProtocol::new(addrs.0, addrs.1, stream))
        }
        Err(error) => {
            event!(Level::INFO, "Failed to connect to SLP server: {}", error);
            Err(error.into())
        }
    }
}

#[cfg(feature = "simple")]
pub async fn ping(addrs: (String, u16)) -> Result<(ServerPingInfo, Duration), PingError> {
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
