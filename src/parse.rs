use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Hash, Clone, PartialEq, Eq)]
#[non_exhaustive]
#[serde(rename_all = "camelCase")]
pub struct JavaServerInfo {
    pub version: Option<ServerVersion>,
    pub players: Option<ServerPlayers>,
    pub description: TextComponent,
    pub favicon: Option<String>,
    #[serde(rename = "modinfo")]
    pub mod_info: Option<ServerModInfo>,
    /// Servers with the No Chat Reports mod installed will set this field to `true` to indicate
    /// to players that all chat messages sent on this server are not reportable to Mojang.
    pub prevents_chat_reports: Option<bool>,
    /// If the server supports Chat Preview (added in 1.19 and removed in 1.19.3), this field is set to `true`.
    pub previews_chat: Option<bool>,
    /// Servers will set this field to `true` if they block chat messages that cannot be reported to Mojang.
    pub enforces_secure_chat: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Hash, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ServerVersion {
    pub name: String,
    pub protocol: u32,
}

#[derive(Serialize, Deserialize, Debug, Hash, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ServerPlayers {
    pub max: u32,
    pub online: u32,
    pub sample: Option<Vec<ServerPlayersSample>>,
}

/// Contains basic information about one of the players in a server.
#[derive(Serialize, Deserialize, Debug, Hash, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ServerPlayersSample {
    /// The player's username
    pub name: Option<String>,
    /// The player's UUID
    pub id: Option<String>,
}

impl ServerPlayersSample {
    /// Returns whether the server has chosen to hide this player's identity and is reporting placeholder information. This is generally caused by a player having the [Allow Server Listings](https://wiki.vg/Protocol#Client_Information_.28configuration.29) option set to `false`.
    pub fn is_anonymous(&self) -> bool {
        self.id
            .as_deref()
            .map_or(true, |id| id == "00000000-0000-0000-0000-000000000000")
    }
}

#[derive(Serialize, Deserialize, Debug, Hash, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ServerModInfo {
    #[serde(rename = "type")]
    pub loader_type: String,
    #[serde(rename = "modList")]
    pub mod_list: Vec<ServerMod>,
}

#[derive(Serialize, Deserialize, Debug, Hash, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ServerMod {
    #[serde(rename = "modid")]
    pub mod_id: String,
    pub version: String,
}

impl std::str::FromStr for JavaServerInfo {
    type Err = serde_json::Error;
    fn from_str(json: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(json)
    }
}

#[derive(Serialize, Deserialize, Debug, Hash, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum TextComponent {
    Plain(String),
    Fancy(FancyText),
    Extra(Vec<TextComponent>),
}

#[derive(Debug, Serialize, Deserialize, Hash, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct FancyText {
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub bold: Option<bool>,
    #[serde(default)]
    pub italic: Option<bool>,
    #[serde(default)]
    pub underlined: Option<bool>,
    #[serde(default)]
    pub strikethrough: Option<bool>,
    #[serde(default)]
    pub obfuscated: Option<bool>,
    #[serde(default)]
    pub extra: Option<Vec<TextComponent>>,
}

impl From<TextComponent> for FancyText {
    fn from(value: TextComponent) -> Self {
        match value {
            TextComponent::Plain(text) => FancyText {
                text: Some(text),
                ..Default::default()
            },
            TextComponent::Fancy(fancy) => fancy,
            TextComponent::Extra(components) => {
                let mut components = components.into_iter();
                let mut first = components.next().map(FancyText::from).unwrap_or_default();
                first.extra.get_or_insert_with(Vec::new).extend(components);
                first
            }
        }
    }
}
