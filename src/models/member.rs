use rusqlite::Row;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use super::wallet::to_sqlite_err;
use crate::error::AppError;

// ─── MemberRole ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MemberRole {
    Owner,
    Member,
}

impl fmt::Display for MemberRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MemberRole::Owner  => write!(f, "owner"),
            MemberRole::Member => write!(f, "member"),
        }
    }
}

impl FromStr for MemberRole {
    type Err = AppError;
    fn from_str(s: &str) -> crate::error::Result<Self> {
        match s {
            "owner"  => Ok(MemberRole::Owner),
            "member" => Ok(MemberRole::Member),
            _        => Err(AppError::Parse(format!("Unknown member role: {s}"))),
        }
    }
}

// ─── Gender ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Gender {
    Male,
    Female,
    Other,
}

impl fmt::Display for Gender {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Gender::Male   => write!(f, "male"),
            Gender::Female => write!(f, "female"),
            Gender::Other  => write!(f, "other"),
        }
    }
}

impl FromStr for Gender {
    type Err = AppError;
    fn from_str(s: &str) -> crate::error::Result<Self> {
        match s {
            "male"   => Ok(Gender::Male),
            "female" => Ok(Gender::Female),
            "other"  => Ok(Gender::Other),
            _        => Err(AppError::Parse(format!("Unknown gender: {s}"))),
        }
    }
}

// ─── Member ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Member {
    pub id:             String,
    pub family_id:      Option<String>,
    pub full_name:      String,
    pub birth_date:     Option<i64>,
    pub gender:         Option<Gender>,
    pub phone:          Option<String>,
    pub id_number:      Option<String>,
    pub id_issue_date:  Option<i64>,
    pub id_issue_place: Option<String>,
    pub address:        Option<String>,
    pub role:           MemberRole,
    pub avatar_emoji:   Option<String>,
    pub note:           Option<String>,
    pub created_at:     i64,
}

impl Member {
    pub fn from_row(row: &Row) -> rusqlite::Result<Self> {
        let role_str: String = row.get("role")?;
        let gender_str: Option<String> = row.get("gender")?;

        Ok(Member {
            id:             row.get("id")?,
            family_id:      row.get("family_id")?,
            full_name:      row.get("full_name")?,
            birth_date:     row.get("birth_date")?,
            gender:         gender_str
                                .as_deref()
                                .map(Gender::from_str)
                                .transpose()
                                .map_err(to_sqlite_err)?,
            phone:          row.get("phone")?,
            id_number:      row.get("id_number")?,
            id_issue_date:  row.get("id_issue_date")?,
            id_issue_place: row.get("id_issue_place")?,
            address:        row.get("address")?,
            role:           MemberRole::from_str(&role_str).map_err(to_sqlite_err)?,
            avatar_emoji:   row.get("avatar_emoji")?,
            note:           row.get("note")?,
            created_at:     row.get("created_at")?,
        })
    }
}

// ─── NewMember ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewMember {
    pub family_id:      Option<String>,
    pub full_name:      String,
    pub birth_date:     Option<i64>,
    pub gender:         Option<Gender>,
    pub phone:          Option<String>,
    pub id_number:      Option<String>,
    pub id_issue_date:  Option<i64>,
    pub id_issue_place: Option<String>,
    pub address:        Option<String>,
    pub role:           MemberRole,
    pub avatar_emoji:   Option<String>,
    pub note:           Option<String>,
}
