#[cfg(feature = "connect")]
use tokio::net::{lookup_host, TcpStream, ToSocketAddrs};
use tracing::{event, instrument, Level};

#[cfg(feature = "connect")]
pub mod mc_string;
#[cfg(feature = "connect")]
pub mod protocol;

#[cfg(feature = "parse")]
pub mod parse;

#[cfg(feature = "connect")]
use crate::protocol::{ProtocolError, SlpProtocol};
#[cfg(feature = "connect")]
pub use protocol::Frame;

#[cfg(feature = "connect")]
#[instrument]
pub async fn connect<T>(addrs: T) -> Result<SlpProtocol, ProtocolError>
where
    T: ToSocketAddrs + std::fmt::Debug,
{
    let addrs_debug = format!("{:?}", addrs);
    // lookup_host can return multiple but we just need one so we discard the rest
    let socket_addrs = match lookup_host(addrs).await?.next() {
        Some(socket_addrs) => socket_addrs,
        None => {
            event!(Level::INFO, "DNS lookup failed for address");
            return Err(protocol::ProtocolError::DNSLookupFailed(addrs_debug));
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

/*
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
*/
