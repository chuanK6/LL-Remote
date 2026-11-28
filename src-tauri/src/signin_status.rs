use crate::{
    error::Result,
    storage::{atomic_write, digest, Store},
};
use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SigninStatus {
    username: String,
    #[serde(flatten)]
    dates: Dates,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Dates {
    expires_at: Option<DateTime<FixedOffset>>,
    available_at: Option<DateTime<FixedOffset>>,
}

#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebsiteDates {
    pub username: String,
    expires_at: Option<String>,
    available_at: Option<String>,
}

pub fn account_key(username: &str) -> String {
    username
        .trim()
        .strip_prefix("+86")
        .unwrap_or(username.trim())
        .to_ascii_lowercase()
}

pub fn load(store: &Store, username: String) -> Result<Option<SigninStatus>> {
    let (dates, _) = crate::storage::read_json::<Dates>(&path(store, &username))?;
    Ok((dates.expires_at.is_some() || dates.available_at.is_some())
        .then_some(SigninStatus { username, dates }))
}

pub fn save(store: &Store, page: WebsiteDates) -> Result<bool> {
    let path = path(store, &page.username);
    let expires_at = parse_date(page.expires_at.as_deref());
    let available_at = parse_date(page.available_at.as_deref());
    if expires_at.is_none() && available_at.is_none() {
        return Ok(false);
    }
    let (previous, _) = crate::storage::read_json::<Dates>(&path).unwrap_or_default();
    let expiry_changed = expires_at.is_some() && expires_at != previous.expires_at;
    let dates = Dates {
        expires_at: expires_at.or(previous.expires_at),
        available_at: if expiry_changed {
            available_at
        } else {
            available_at.or(previous.available_at)
        },
    };
    if dates == previous {
        return Ok(false);
    }
    atomic_write(&path, &serde_json::to_vec(&dates)?)?;
    Ok(true)
}

fn path(store: &Store, username: &str) -> PathBuf {
    store
        .root
        .join("accounts")
        .join(digest(&account_key(username)))
        .join("signin.json")
}

fn parse_date(date: Option<&str>) -> Option<DateTime<FixedOffset>> {
    let date = date?;
    let time = NaiveDateTime::parse_from_str(date, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(date, "%Y-%m-%d %H:%M"))
        .ok()?;
    FixedOffset::east_opt(8 * 3600)?
        .from_local_datetime(&time)
        .single()
}

#[cfg(test)]
mod tests {
    use super::parse_date;

    #[test]
    fn reads_website_dates_in_beijing_time() {
        assert_eq!(
            parse_date(Some("2026-11-27 13:00:00"))
                .unwrap()
                .to_rfc3339(),
            "2026-11-27T13:00:00+08:00"
        );
        assert_eq!(
            parse_date(Some("2026-11-27 11:00")).unwrap().to_rfc3339(),
            "2026-11-27T11:00:00+08:00"
        );
        assert!(parse_date(Some("2026-02-30 01:43:00")).is_none());
    }
}
