use rust_raknet::error::RaknetError;
use snafu::{Backtrace, ResultExt, Snafu};
use std::{
    net::{AddrParseError, SocketAddr},
    str::FromStr,
    time::Duration,
};

//type RaknetResult<T> = Result<T, RaknetError>;
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
    map: Option<String>,
    game_mode: Option<String>,
    nintendo_only: Option<String>,
    ipv4_port: Option<String>,
    ipv6_port: Option<String>,
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
                map: components.next(),
                game_mode: components.next(),
                nintendo_only: components.next(),
                ipv4_port: components.next(),
                ipv6_port: components.next(),
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
    #[snafu(display("Raknet error: {source:?}"))]
    Raknet {
        #[snafu(source(false))]
        source: RaknetError,
        backtrace: Backtrace,
    },
    #[snafu(display("Failed to parse server info: {source}"), context(false))]
    ServerInfoParse {
        source: ServerInfoParseError,
        backtrace: Backtrace,
    },
}

pub type BedrockPingResult<T> = Result<T, BedrockPingError>;

pub async fn ping(address: (String, u16)) -> BedrockPingResult<(BedrockServerInfo, Duration)> {
    let address = SocketAddr::new(
        address
            .0
            .parse()
            .context(AddressParseSnafu { address: address.0 })?,
        address.1,
    );
    let (latency_ms, motd) = rust_raknet::RaknetSocket::ping(&address)
        .await
        .map_err(|source| RaknetSnafu { source }.build())?;

    Ok((motd.parse()?, Duration::from_millis(latency_ms as u64)))
}
