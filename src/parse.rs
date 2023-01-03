use serde::Deserialize;

use self::fancy_string::FancyText;

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
    pub extra: Option<FancyText>,
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

pub mod fancy_string {
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    pub struct FancyText(pub Vec<FancyTextComponent>);

    impl FancyText {
        pub fn to_markdown(&self) -> String {
            let mut builder = String::with_capacity(10);
            for component in &self.0 {
                builder += &component.to_markdown();
            }
            builder
        }
    }

    #[derive(Debug, Deserialize)]
    #[serde(untagged)]
    pub enum FancyTextComponent {
        ColorText {
            color: String,
            text: String,
        },
        PlainText {
            text: String,
        },
        NestedText {
            #[serde(default)]
            bold: bool,
            #[serde(default)]
            italic: bool,
            #[serde(default)]
            underlined: bool,
            #[serde(default)]
            strikethrough: bool,
            #[serde(default)]
            obfuscated: bool,
            extra: FancyText,
        },
    }

    impl FancyTextComponent {
        pub fn to_markdown(&self) -> String {
            match self {
                FancyTextComponent::ColorText { color: _, text } => text.clone(),
                FancyTextComponent::PlainText { text } => text.clone(),
                FancyTextComponent::NestedText {
                    bold,
                    italic,
                    underlined,
                    strikethrough,
                    obfuscated: _,
                    extra,
                } => {
                    let mut text = extra.to_markdown();
                    if *bold {
                        text = format!("**{text}**");
                    }
                    if *italic {
                        text = format!("*{text}*");
                    }
                    if *underlined {
                        text = format!("__{text}__");
                    }
                    if *strikethrough {
                        text = format!("~~{text}~~");
                    }
                    text
                }
            }
        }
    }
}
