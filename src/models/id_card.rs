use rusqlite::Row;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use super::wallet::to_sqlite_err;
use crate::error::AppError;

// ─── IdSide ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IdSide {
    Front,
    Back,
}

impl IdSide {
    pub const ALL: [IdSide; 2] = [IdSide::Front, IdSide::Back];
}

impl fmt::Display for IdSide {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IdSide::Front => write!(f, "front"),
            IdSide::Back  => write!(f, "back"),
        }
    }
}

impl FromStr for IdSide {
    type Err = AppError;
    fn from_str(s: &str) -> crate::error::Result<Self> {
        match s {
            "front" => Ok(IdSide::Front),
            "back"  => Ok(IdSide::Back),
            _       => Err(AppError::Parse(format!("Unknown ID side: {s}"))),
        }
    }
}

// ─── IdCardImage ──────────────────────────────────────────────────────────────

/// One photographed side of a member's CCCD, stored as JPEG bytes
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IdCardImage {
    pub member_id:  String,
    pub side:       IdSide,
    #[serde(with = "base64_bytes")]
    pub jpeg:       Vec<u8>,
    pub updated_at: i64,
}

impl IdCardImage {
    pub fn from_row(row: &Row) -> rusqlite::Result<Self> {
        let side_str: String = row.get("side")?;
        Ok(IdCardImage {
            member_id:  row.get("member_id")?,
            side:       IdSide::from_str(&side_str).map_err(to_sqlite_err)?,
            jpeg:       row.get("jpeg")?,
            updated_at: row.get("updated_at")?,
        })
    }
}

/// `data:image/jpeg;base64,...` for showing the photo in an `img` tag
pub fn jpeg_data_url(jpeg: &[u8]) -> String {
    use base64::Engine;
    format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(jpeg))
}

/// Backup JSON keeps the JPEG bytes as one base64 string instead of a number array
pub(crate) mod base64_bytes {
    use base64::Engine;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&base64::engine::general_purpose::STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(d)?;
        base64::engine::general_purpose::STANDARD.decode(text).map_err(serde::de::Error::custom)
    }
}
