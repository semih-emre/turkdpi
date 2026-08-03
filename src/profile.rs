use anyhow::{bail, Result};
use serde::Deserialize;
use std::str::FromStr;

#[derive(Clone, Copy)]
pub enum ProfileName {
    Safe,
    Balanced,
    Discord,
    Aggressive,
}

impl ProfileName {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Balanced => "balanced",
            Self::Discord => "discord",
            Self::Aggressive => "aggressive",
        }
    }
}

impl FromStr for ProfileName {
    type Err = anyhow::Error;
    fn from_str(value: &str) -> Result<Self> {
        match value {
            "safe" => Ok(Self::Safe),
            "balanced" => Ok(Self::Balanced),
            "discord" => Ok(Self::Discord),
            "aggressive" => Ok(Self::Aggressive),
            _ => bail!("izin verilmeyen profil"),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub name: String,
    pub description: String,
    pub hostlist: String,
    pub tcp_args: Vec<String>,
    pub udp_args: Vec<String>,
}

impl Profile {
    pub fn validate(&self, expected: ProfileName) -> Result<()> {
        if self.name != expected.as_str() {
            bail!("profil adı dosya adıyla eşleşmiyor");
        }
        if self.hostlist != "/usr/share/turkdpi/discord-hosts.txt" {
            bail!("hostlist yolu izin verilen yol değil");
        }
        for arg in self.tcp_args.iter().chain(self.udp_args.iter()) {
            if !arg.starts_with("--") || arg.contains('\0') || arg.len() > 256 {
                bail!("geçersiz nfqws argümanı");
            }
        }
        Ok(())
    }
}
