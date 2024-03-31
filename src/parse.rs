use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Hash, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct JavaServerInfo {
    pub version: Option<ServerVersion>,
    pub players: Option<ServerPlayers>,
    pub description: TextComponent,
    pub favicon: Option<String>,
    #[serde(rename = "modinfo")]
    pub mod_info: Option<ServerModInfo>,
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

#[derive(Serialize, Deserialize, Debug, Hash, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ServerPlayersSample {
    pub name: Option<String>,
    pub id: Option<String>,
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
