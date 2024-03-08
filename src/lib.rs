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
    #[snafu(display("connection failed: {source}"), context(false))]
    Protocol {
        #[snafu(backtrace)]
        source: crate::protocol::ProtocolError,
    },
    #[snafu(display("connection did not respond in time"))]
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
