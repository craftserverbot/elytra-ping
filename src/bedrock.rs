use bytes::{Buf, BufMut};
use chrono::Utc;
use snafu::{Backtrace, OptionExt, ResultExt, Snafu};
use std::{
    io::{Cursor, Read},
    net::AddrParseError,
    str::FromStr,
    time::Duration,
    vec,
};
use tokio::net::{lookup_host, UdpSocket};
use tracing::{debug, trace};

#[derive(Debug, Hash, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct BedrockServerInfo {
    edition: String,
    name: String,
    protocol_version: String,
    mc_version: String,
    online_players: String,
    max_players: String,
    server_id: Option<String>,
    map_name: Option<String>,
    game_mode: Option<String>,
    numeric_game_mode: Option<String>,
    ipv4_port: Option<String>,
    ipv6_port: Option<String>,
    extra: Vec<String>,
}

#[derive(Debug, Snafu)]
#[snafu(display("Not enough motd components"))]
pub struct ServerInfoParseError;

impl FromStr for BedrockServerInfo {
    type Err = ServerInfoParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        fn parse_impl(s: &str) -> Option<BedrockServerInfo> {
            let mut components = s.split(';').map(|component| component.to_owned());
            Some(BedrockServerInfo {
                edition: components.next()?,
                name: components.next()?,
                protocol_version: components.next()?,
                mc_version: components.next()?,
                online_players: components.next()?,
                max_players: components.next()?,
                server_id: components.next(),
                map_name: components.next(),
                game_mode: components.next(),
                numeric_game_mode: components.next(),
                ipv4_port: components.next(),
                ipv6_port: components.next(),
                extra: components.collect(),
            })
        }

        parse_impl(s).ok_or(ServerInfoParseError)
    }
}

#[derive(Debug, Snafu)]
pub enum BedrockPingError {
    #[snafu(display("Failed to parse address {address:?}: {source}"))]
    AddressParse {
        source: AddrParseError,
        address: String,
        backtrace: Backtrace,
    },
    #[snafu(display("Server did not respond"))]
    NoResponse { backtrace: Backtrace },
    #[snafu(display("Failed to parse server info: {source}"), context(false))]
    ServerInfoParse {
        source: ServerInfoParseError,
        backtrace: Backtrace,
    },
    #[snafu(display("io error: {source}"), context(false))]
    Io {
        source: std::io::Error,
        backtrace: Backtrace,
    },
    #[snafu(display("dns lookup failed for address `{address}`"))]
    DNSLookupFailed {
        address: String,
        backtrace: Backtrace,
    },
    #[snafu(display("failed to open socket: {source}"))]
    ConnectFailed {
        source: std::io::Error,
        backtrace: Backtrace,
    },
}

pub type BedrockPingResult<T> = Result<T, BedrockPingError>;

/// Random number that must be in ping packets.
/// https://wiki.vg/Raknet_Protocol#Data_types
const MAGIC: u128 = 0x00ffff00fefefefefdfdfdfd12345678;

struct PingRequestFrame {
    time: i64,
    magic: u128,
    guid: i64,
}

impl PingRequestFrame {
    const PACKET_ID: u8 = 0x01;
    pub fn to_vec(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(1028);
        buf.put_u8(Self::PACKET_ID);
        buf.put_i64(self.time);
        buf.put_u128(self.magic);
        buf.put_i64(self.guid);
        buf
    }
}

struct PingResponseFrame {
    time: i64,
    /// "Server ID string" on wiki.vg
    motd: String,
}

impl PingResponseFrame {
    const SIZE: usize = 8 + 8 + 16 + 2;
    const PACKET_ID: u8 = 0x1c;
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::SIZE {
            return None;
        }
        let mut cursor = Cursor::new(bytes);

        let packet_id = cursor.get_u8();
        if packet_id != Self::PACKET_ID {
            return None;
        }

        let time = cursor.get_i64();
        let _guid = cursor.get_i64();
        let magic = cursor.get_u128();

        if magic != MAGIC {
            return None;
        }

        let motd_len = cursor.get_u16();
        let mut motd_bytes = vec![0u8; motd_len as usize];
        cursor.read_exact(&mut motd_bytes).ok()?;
        let motd = String::from_utf8(motd_bytes).ok()?;

        Some(PingResponseFrame { time, motd })
    }
}

/// Ping a bedrock server and return the info and latency. Timeout is `retry_timeout * retries`.
pub async fn ping(
    address: (String, u16),
    retry_timeout: Duration,
    retries: u64,
) -> BedrockPingResult<(BedrockServerInfo, Duration)> {
    let resolved = lookup_host(address.clone())
        .await?
        .next()
        .context(DNSLookupFailedSnafu { address: address.0 })?;
    trace!("host resolved to {resolved}");

    let socket = UdpSocket::bind("0.0.0.0:0")
        .await
        .context(ConnectFailedSnafu)?;
    socket.connect(resolved).await.context(ConnectFailedSnafu)?;
    trace!("opened udp socket");

    let mut response = None;
    for retry in 0..retries {
        debug!("pinging raknet server, attempt {}", retry + 1);
        tokio::select! {
            biased;
            _ = tokio::time::sleep(retry_timeout) => continue,
            res = attempt_ping(&socket) => response = res,
        }
        if response.is_some() {
            break;
        }
    }
    let (response, latency) = response.context(NoResponseSnafu)?;

    trace!("ping finished");

    Ok((response.motd.parse()?, latency))
}

/// See: https://wiki.vg/Raknet_Protocol#Unconnected_Ping
async fn attempt_ping(socket: &UdpSocket) -> Option<(PingResponseFrame, Duration)> {
    let outgoing_packet = PingRequestFrame {
        time: Utc::now().timestamp_millis(),
        magic: MAGIC,
        guid: rand::random(),
    };
    socket.send(&outgoing_packet.to_vec()).await.ok()?;
    let mut buffer = Vec::with_capacity(1024);
    socket.recv_buf(&mut buffer).await.ok()?;
    let incoming_packet = PingResponseFrame::from_bytes(&buffer)?;
    let latency_millis = Utc::now().timestamp_millis() - incoming_packet.time;
    let latency = Duration::from_millis(latency_millis as u64);

    Some((incoming_packet, latency))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cubecraft() {
        ping(
            ("play.cubecraft.net".to_owned(), 19132),
            Duration::from_secs(2),
            3,
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn the_hive() {
        ping(
            ("geo.hivebedrock.network".to_owned(), 19132),
            Duration::from_secs(2),
            3,
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    #[should_panic]
    async fn invalid_address() {
        ping(("example.com".to_owned(), 19132), Duration::from_secs(2), 3)
            .await
            .unwrap();
    }
}
