//! Cleaner verification checklist and employee profile. Console only: the owner ticks the checklist
//! before verifying a pro, and keeps the employee details on the profile.

use super::{audit, uploads};
use crate::{
    auth::Admin,
    crypto,
    error::{ApiError, ApiResult},
    state::AppState,
    util::own_upload,
};
use axum::{
    Json,
    extract::{Path, State},
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use uuid::Uuid;

/// Items the owner ticks before verifying a pro. Adding one here makes it required everywhere.
pub const CHECKLIST_ITEMS: [&str; 4] = ["identity", "tax_type", "experience", "emergency_contact"];
const BIO_MAX_CHARS: usize = 500;
const EMERGENCY_CONTACT_MAX_CHARS: usize = 30;
const EMPLOYMENT_TYPES: [&str; 3] = ["freelance", "employee", "contract"];

pub type Checklist = BTreeMap<String, bool>;

/// Rejects items the checklist does not know. A checklist may be partial while the owner works on it.
pub fn validate_checklist(list: &Checklist) -> ApiResult<()> {
    if list.keys().all(|k| CHECKLIST_ITEMS.contains(&k.as_str())) {
        Ok(())
    } else {
        Err(ApiError::BadRequest("invalid_checklist"))
    }
}

/// True when every required item is ticked. A missing item counts as open.
pub fn checklist_complete(list: &Checklist) -> bool {
    CHECKLIST_ITEMS.iter().all(|item| list.get(*item) == Some(&true))
}

fn checklist_json(list: &Checklist) -> Value {
    Value::Object(list.iter().map(|(k, v)| (k.clone(), Value::Bool(*v))).collect())
}

/// Stores the checklist as the pro's current verification record. Runs inside the status transaction.
pub(crate) async fn record_checklist(db: impl sqlx::PgExecutor<'_>, pro: Uuid, staff: Uuid, list: &Checklist) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO pro_verifications (pro_id, checklist, decided_by, decided_at) VALUES ($1, $2, $3, now())
         ON CONFLICT (pro_id) DO UPDATE SET checklist = EXCLUDED.checklist, decided_by = EXCLUDED.decided_by, decided_at = EXCLUDED.decided_at",
    )
    .bind(pro)
    .bind(checklist_json(list))
    .bind(staff)
    .execute(db)
    .await?;
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileBody {
    photo_url: Option<String>,
    bio: Option<String>,
    employment_type: Option<String>,
    emergency_contact: Option<String>,
    hired_on: Option<NaiveDate>,
}

/// Profile fields after validation, ready to store. The emergency contact is still in plain text here.
#[derive(Debug, PartialEq)]
pub struct Profile {
    pub photo_url: Option<String>,
    pub bio: String,
    pub employment_type: Option<String>,
    pub emergency_contact: Option<String>,
    pub hired_on: Option<NaiveDate>,
}

fn valid_contact(contact: &str) -> bool {
    contact.chars().count() <= EMERGENCY_CONTACT_MAX_CHARS
        && contact.chars().any(|c| c.is_ascii_digit())
        && contact.chars().all(|c| c.is_ascii_digit() || matches!(c, '-' | '+' | ' '))
}

/// Checks each field at the trust boundary. Photos must be our own uploads, as for every other photo.
pub fn clean_profile(body: ProfileBody, base: &str) -> ApiResult<Profile> {
    let photo_url = body
        .photo_url
        .map(|u| own_upload(base, &u).ok_or(ApiError::BadRequest("invalid_photo")))
        .transpose()?;
    let bio = body.bio.unwrap_or_default().trim().to_string();
    if bio.chars().count() > BIO_MAX_CHARS {
        return Err(ApiError::BadRequest("invalid_bio"));
    }
    let employment_type = match body.employment_type {
        Some(t) if EMPLOYMENT_TYPES.contains(&t.as_str()) => Some(t),
        Some(_) => return Err(ApiError::BadRequest("invalid_employment_type")),
        None => None,
    };
    let emergency_contact = body.emergency_contact.map(|c| c.trim().to_string()).filter(|c| !c.is_empty());
    if emergency_contact.as_deref().is_some_and(|c| !valid_contact(c)) {
        return Err(ApiError::BadRequest("invalid_emergency_contact"));
    }
    Ok(Profile {
        photo_url,
        bio,
        employment_type,
        emergency_contact,
        hired_on: body.hired_on,
    })
}

/// Decrypts the emergency contact for the console and records that someone looked at it.
async fn reveal_contact(s: &AppState, actor: Uuid, pro: Uuid, enc: Option<String>) -> ApiResult<Option<String>> {
    let Some(enc) = enc else { return Ok(None) };
    let plain =
        crypto::decrypt(&s.config.data_key, crypto::aad::PRO_EMERGENCY_CONTACT, &enc).ok_or(ApiError::Internal("cannot decrypt emergency contact".into()))?;
    audit(&s.db, "staff", Some(actor), "pro.profile.reveal", &pro.to_string(), json!({})).await?;
    Ok(Some(plain))
}

pub async fn get_profile(State(s): State<AppState>, Admin(a): Admin, Path(id): Path<Uuid>) -> ApiResult<Json<Value>> {
    let status: String = sqlx::query_scalar("SELECT status FROM pros WHERE id = $1")
        .bind(id)
        .fetch_optional(&s.db)
        .await?
        .ok_or(ApiError::NotFound)?;
    let verification: Option<(Value, DateTime<Utc>)> = sqlx::query_as("SELECT checklist, decided_at FROM pro_verifications WHERE pro_id = $1")
        .bind(id)
        .fetch_optional(&s.db)
        .await?;
    let (checklist, decided_at) = verification.map_or((json!({}), None), |(c, at)| (c, Some(at)));
    let row: Option<(Option<String>, String, Option<String>, Option<String>, Option<NaiveDate>)> =
        sqlx::query_as("SELECT photo_url, bio, employment_type, emergency_contact_enc, hired_on FROM pro_profiles WHERE pro_id = $1")
            .bind(id)
            .fetch_optional(&s.db)
            .await?;
    let profile = match row {
        Some((photo_url, bio, employment_type, enc, hired_on)) => {
            let emergency_contact = reveal_contact(&s, a.id, id, enc).await?;
            json!({ "photo_url": photo_url, "bio": bio, "employment_type": employment_type, "emergency_contact": emergency_contact, "hired_on": hired_on })
        }
        None => Value::Null,
    };
    Ok(Json(
        json!({ "status": status, "items": CHECKLIST_ITEMS, "checklist": checklist, "decided_at": decided_at, "profile": profile }),
    ))
}

/// The URL shape is checked in `clean_profile`; this checks the file was really uploaded.
async fn check_photo(s: &AppState, photo: Option<&str>) -> ApiResult<()> {
    if let Some(path) = photo
        && !uploads::owned(&s.db, None, &[path.to_string()]).await?
    {
        return Err(ApiError::BadRequest("invalid_photo"));
    }
    Ok(())
}

pub async fn put_profile(State(s): State<AppState>, Admin(a): Admin, Path(id): Path<Uuid>, Json(body): Json<ProfileBody>) -> ApiResult<Json<Value>> {
    let p = clean_profile(body, &s.config.public_base_url)?;
    check_photo(&s, p.photo_url.as_deref()).await?;
    let contact_enc = p
        .emergency_contact
        .as_deref()
        .map(|c| crypto::encrypt(&s.config.data_key, crypto::aad::PRO_EMERGENCY_CONTACT, c));
    let mut tx = s.db.begin().await?;
    let written = sqlx::query(
        "INSERT INTO pro_profiles (pro_id, photo_url, bio, employment_type, emergency_contact_enc, hired_on, updated_at)
         SELECT id, $2, $3, $4, $5, $6, now() FROM pros WHERE id = $1
         ON CONFLICT (pro_id) DO UPDATE SET photo_url = EXCLUDED.photo_url, bio = EXCLUDED.bio, employment_type = EXCLUDED.employment_type,
           emergency_contact_enc = EXCLUDED.emergency_contact_enc, hired_on = EXCLUDED.hired_on, updated_at = now()",
    )
    .bind(id)
    .bind(p.photo_url)
    .bind(p.bio)
    .bind(p.employment_type)
    .bind(contact_enc)
    .bind(p.hired_on)
    .execute(&mut *tx)
    .await?;
    if written.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    audit(&mut *tx, "staff", Some(a.id), "pro.profile", &id.to_string(), json!({})).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "http://127.0.0.1:8080";

    fn body() -> ProfileBody {
        ProfileBody {
            photo_url: None,
            bio: None,
            employment_type: None,
            emergency_contact: None,
            hired_on: None,
        }
    }

    #[test]
    fn a_checklist_is_complete_only_when_every_item_is_ticked() {
        let mut list: Checklist = CHECKLIST_ITEMS.iter().map(|k| (k.to_string(), true)).collect();
        assert!(checklist_complete(&list));
        list.insert("tax_type".into(), false);
        assert!(!checklist_complete(&list));
        list.remove("tax_type");
        assert!(!checklist_complete(&list), "a missing item is open");
    }

    #[test]
    fn unknown_checklist_items_are_rejected() {
        let list: Checklist = [("identity".to_string(), true), ("vibes".to_string(), true)].into();
        assert!(validate_checklist(&list).is_err());
        assert!(validate_checklist(&Checklist::new()).is_ok(), "an empty checklist is a valid save");
    }

    #[test]
    fn profile_photo_must_be_our_upload() {
        let mut b = body();
        b.photo_url = Some("https://evil.example/x.jpg".into());
        assert!(clean_profile(b, BASE).is_err());
        let mut b = body();
        b.photo_url = Some(format!("{BASE}/files/0123456789abcdef0123456789abcdef.jpg"));
        assert_eq!(
            clean_profile(b, BASE).expect("ok").photo_url.as_deref(),
            Some("/files/0123456789abcdef0123456789abcdef.jpg")
        );
    }

    #[test]
    fn profile_employment_type_is_one_of_the_known_kinds() {
        let mut b = body();
        b.employment_type = Some("owner".into());
        assert!(clean_profile(b, BASE).is_err());
        let mut b = body();
        b.employment_type = Some("contract".into());
        assert_eq!(clean_profile(b, BASE).expect("ok").employment_type.as_deref(), Some("contract"));
    }

    #[test]
    fn emergency_contact_is_a_phone_like_number_or_absent() {
        for bad in ["call me", "+", "010-1234-56789012345678901234567890"] {
            let mut b = body();
            b.emergency_contact = Some(bad.into());
            assert!(clean_profile(b, BASE).is_err(), "{bad} should be refused");
        }
        let mut b = body();
        b.emergency_contact = Some("   ".into());
        assert_eq!(clean_profile(b, BASE).expect("ok").emergency_contact, None, "blank means none");
        let mut b = body();
        b.emergency_contact = Some(" 010-1234-5678 ".into());
        assert_eq!(clean_profile(b, BASE).expect("ok").emergency_contact.as_deref(), Some("010-1234-5678"));
    }

    #[test]
    fn bio_is_capped() {
        let mut b = body();
        b.bio = Some("가".repeat(BIO_MAX_CHARS + 1));
        assert!(clean_profile(b, BASE).is_err());
    }
}
