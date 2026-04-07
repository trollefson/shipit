use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[derive(Default)]
pub struct Settings {
    pub shipit: ShipitSettings,
    pub platform: PlatformSettings,
}


#[derive(Debug, Serialize, Deserialize)]
pub struct ShipitSettings {
    pub agent: String,
    pub commits: String,
}

impl Default for ShipitSettings {
    fn default() -> Self {
        Self {
            agent: "shipit".to_string(),
            commits: "custom".to_string(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[derive(Default)]
pub struct PlatformSettings {
    pub domain: String,
    pub token: String,
}

