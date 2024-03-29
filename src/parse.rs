use std::fmt;

use serde::{
    de::{self, MapAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};

use self::fancy_string::FancyText;

#[derive(Serialize, Deserialize, Debug, Hash, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct JavaServerInfo {
    pub version: Option<ServerVersion>,
    pub players: Option<ServerPlayers>,
    #[serde(deserialize_with = "de_description")]
    pub description: ServerDescription,
    pub favicon: Option<String>,
    #[serde(rename = "modinfo")]
    pub mod_info: Option<ServerModInfo>,
}

fn de_description<'de, D>(deserializer: D) -> Result<ServerDescription, D::Error>
where
    D: Deserializer<'de>,
{
    struct DeDescription;

    impl<'de> Visitor<'de> for DeDescription {
        type Value = ServerDescription;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("string or map")
        }

        fn visit_str<E>(self, value: &str) -> Result<ServerDescription, E>
        where
            E: de::Error,
        {
            Ok(ServerDescription {
                text: value.to_owned(),
                extra: None,
            })
        }

        fn visit_map<M>(self, map: M) -> Result<ServerDescription, M::Error>
        where
            M: MapAccess<'de>,
        {
            Deserialize::deserialize(de::value::MapAccessDeserializer::new(map))
        }
    }

    deserializer.deserialize_any(DeDescription)
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
pub struct ServerDescription {
    pub text: String,
    pub extra: Option<FancyText>,
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

pub mod fancy_string {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, Hash, Clone, PartialEq, Eq, Default)]
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

    #[derive(Debug, Serialize, Deserialize, Hash, Clone, PartialEq, Eq)]
    #[serde(untagged)]
    pub enum FancyTextComponent {
        #[serde(deserialize_with = "de_plain_text")]
        Plain {
            text: String,
            #[serde(default)]
            color: Option<String>,
        },
        Nested {
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
            #[serde(default)]
            extra: FancyText,
        },
    }

    impl FancyTextComponent {
        pub fn to_markdown(&self) -> String {
            match self {
                FancyTextComponent::Plain { text, .. } => text.clone(),
                FancyTextComponent::Nested {
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

    fn de_plain_text<'de, D>(deserializer: D) -> Result<(String, Option<String>), D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct DePlainText;

        impl<'de> serde::de::Visitor<'de> for DePlainText {
            type Value = (String, Option<String>);

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("string or plain text object")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok((value.to_owned(), None))
            }

            fn visit_map<M>(self, map: M) -> Result<Self::Value, M::Error>
            where
                M: serde::de::MapAccess<'de>,
            {
                serde::Deserialize::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }

        deserializer.deserialize_any(DePlainText)
    }
}
