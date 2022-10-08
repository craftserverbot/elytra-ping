use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct ServerPingInfo {
    pub version: Option<ServerVersion>,
    pub players: Option<ServerPlayers>,
    pub description: Option<ServerDescription>,
    pub favicon: Option<String>,
    #[serde(rename = "modinfo")]
    pub mod_info: Option<ServerModInfo>,
}

#[derive(Deserialize, Debug)]
pub struct ServerVersion {
    pub name: String,
    pub protocol: u32,
}

#[derive(Deserialize, Debug)]
pub struct ServerPlayers {
    pub max: u32,
    pub online: u32,
    pub sample: Option<Vec<ServerPlayersSample>>,
}

#[derive(Deserialize, Debug)]
pub struct ServerPlayersSample {
    pub name: Option<String>,
    pub id: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct ServerDescription {
    pub text: String,
}

#[derive(Deserialize, Debug)]
pub struct ServerModInfo {
    #[serde(rename = "type")]
    pub loader_type: String,
    // pub mod_list: Vec<ServerModInfoMod>,
}

impl std::str::FromStr for ServerPingInfo {
    type Err = serde_json::Error;
    fn from_str(json: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(json)
    }
}
