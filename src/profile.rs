use anyhow::{bail, Result};
use serde::Deserialize;
use std::str::FromStr;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileName {
    Safe,
    Balanced,
    Discord,
    Roblox,
    General,
    Aggressive,
}

impl ProfileName {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Balanced => "balanced",
            Self::Discord => "discord",
            Self::Roblox => "roblox",
            Self::General => "general",
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
            "roblox" => Ok(Self::Roblox),
            "general" => Ok(Self::General),
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
    pub hostlist: Option<String>,
    pub tcp_args: Vec<String>,
    pub udp_args: Vec<String>,
}

impl Profile {
    pub fn validate(&self, expected: ProfileName) -> Result<()> {
        if self.name != expected.as_str() {
            bail!("profil adı dosya adıyla eşleşmiyor");
        }
        if let Some(path) = &self.hostlist {
            const ALLOWED: [&str; 3] = [
                "/usr/share/turkdpi/discord-hosts.txt",
                "/usr/share/turkdpi/roblox-hosts.txt",
                "/usr/share/turkdpi/services-hosts.txt",
            ];
            if !ALLOWED.contains(&path.as_str()) {
                bail!("hostlist yolu izin verilen yol değil");
            }
        }
        for arg in self.tcp_args.iter().chain(self.udp_args.iter()) {
            if !arg.starts_with("--") || arg.contains('\0') || arg.len() > 256 {
                bail!("geçersiz nfqws argümanı");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_profiles_are_whitelisted() {
        for (name, expected) in [
            ("safe", ProfileName::Safe),
            ("balanced", ProfileName::Balanced),
            ("discord", ProfileName::Discord),
            ("roblox", ProfileName::Roblox),
            ("general", ProfileName::General),
            ("aggressive", ProfileName::Aggressive),
        ] {
            assert_eq!(
                name.parse::<ProfileName>().expect("allowed profile"),
                expected
            );
        }
        assert!("custom-shell-value".parse::<ProfileName>().is_err());
    }
}
