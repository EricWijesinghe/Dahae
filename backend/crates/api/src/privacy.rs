// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! PIPA (개인정보 보호법) customer rights: consent records, the data export (열람), and the account
//! deletion cascade. Consent rows are append-only evidence; the newest row per kind is the current state.

use crate::{
    crypto,
    error::{ApiError, ApiResult},
    providers::Gateway,
    state::AppState,
};
use axum::{Json, extract::State};
use chrono::{DateTime, Utc};
use domain::booking::BookingStatus;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

/// Version of the texts customers agreed to before published texts existed (the KR pack's `privacy.consent_version`,
/// which a test keeps equal): what releases before per-document versions send and check. The consent rules read the
/// pack and the published texts ([`consent_versions`]), never this constant, so neither a change here nor a new pack
/// template version moves what a kind needs.
pub const CONSENT_VERSION: &str = "2026-11-17";

/// The legal document behind each consent kind (the pack's `privacy.consent_kinds[].doc`).
pub const CONSENT_DOCS: [(&str, &str); 5] = [
    ("terms", "terms"),
    ("privacy", "privacy"),
    ("overseas_transfer", "privacy"),
    ("marketing", "privacy"),
    (LOCATION, "location_terms"),
];

/// The language whose published texts set the consent versions: the pack's first locale, the text that prevails.
pub const LEGAL_LANG: &str = "ko";

/// Kind and whether the service cannot run without it once signed up. Overseas transfer, marketing and location
/// are withdrawable (the public notice promises it); withdrawing overseas transfer stops push and the AI chat.
pub const CONSENTS: [(&str, bool); 5] = [
    ("terms", true),
    ("privacy", true),
    ("overseas_transfer", false),
    ("marketing", false),
    (LOCATION, false),
];

/// Kinds a sign-up must grant. Overseas transfer is required to sign up but withdrawable afterwards.
pub const SIGNUP_REQUIRED: [&str; 3] = ["terms", "privacy", "overseas_transfer"];

/// 위치기반서비스 이용약관 (위치정보법): asked in the app before the first "use my current location".
pub const LOCATION: &str = "location";

/// Messages that are advertising under 정보통신망법: sent only while `marketing` is granted.
/// Transactional kinds (booking, payment, chat) never match.
pub fn is_marketing(kind: &str) -> bool {
    kind == "marketing" || kind.starts_with("marketing.")
}

#[derive(Serialize, sqlx::FromRow)]
struct Latest {
    kind: String,
    version: String,
    granted: bool,
    created_at: DateTime<Utc>,
}

/// Latest row of each consent kind for a customer.
async fn latest(db: &PgPool, user: Uuid) -> sqlx::Result<Vec<Latest>> {
    sqlx::query_as("SELECT DISTINCT ON (kind) kind, version, granted, created_at FROM user_consents WHERE user_id = $1 ORDER BY kind, created_at DESC, id DESC")
        .bind(user)
        .fetch_all(db)
        .await
}

/// One published text in force (in [`LEGAL_LANG`]), as the consent rules read it.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Published {
    doc: String,
    version: String,
    requires_reconsent: bool,
    effective_at: chrono::NaiveDate,
}

/// What the pack says about texts nobody published: the version customers agreed to before published texts existed,
/// and each document's current template version in [`LEGAL_LANG`] (the text the website shows while none is published).
#[derive(Debug, Clone, Default)]
pub struct PackTexts {
    consent_version: String,
    templates: Vec<(String, String)>,
}

impl PackTexts {
    fn of(p: &crate::pack::Pack) -> Self {
        let templates = crate::legal::active_templates(p)
            .unwrap_or_default()
            .into_iter()
            .filter(|t| t.lang == LEGAL_LANG)
            .map(|t| (t.doc, t.version))
            .collect();
        Self {
            consent_version: p.privacy.consent_version.clone(),
            templates,
        }
    }

    /// The same pack with `doc`'s template at `version` (a release that changed the text).
    #[cfg(test)]
    fn with_template(mut self, doc: &str, version: &str) -> Self {
        self.templates.retain(|(d, _)| d != doc);
        self.templates.push((doc.into(), version.into()));
        self
    }

    /// What a kind of `doc` needs while no text of it in force asks again: with texts recorded, the version agreed to
    /// before them (later texts that did not ask again are corrections of it); with none, the pack text shown.
    fn need(&self, doc: &str, recorded: bool) -> &str {
        match self.templates.iter().find(|(d, _)| d == doc) {
            Some((_, v)) if !recorded => v,
            _ => &self.consent_version,
        }
    }
}

/// The pack these consent kinds mirror (`CONSENT_DOCS`, [`LEGAL_LANG`]; `pack_validation` keeps them equal).
/// shortcut: the KR pack for every install, until consent kinds come from the install's pack (`pack.privacy`).
static PACK_TEXTS: std::sync::LazyLock<PackTexts> = std::sync::LazyLock::new(|| {
    crate::pack::load("KR").map(|p| PackTexts::of(&p)).unwrap_or_else(|_| PackTexts {
        consent_version: CONSENT_VERSION.into(),
        templates: Vec::new(),
    })
});

/// How long after a re-consent text is published a signup may still name the text it replaced (an app opened before
/// the publish); the customer is then asked again at once. After that only the texts in force are accepted.
pub const SIGNUP_GRACE_DAYS: i64 = 30;

fn key(v: &str) -> Option<(chrono::NaiveDate, u32)> {
    crate::legal::version_key(v)
}

/// The published texts in force, from which each kind's versions follow, and the pack's word on the rest.
#[derive(Debug, Clone)]
pub struct Versions(Vec<Published>, PackTexts);

impl Default for Versions {
    fn default() -> Self {
        Self(Vec::new(), PACK_TEXTS.clone())
    }
}

impl Versions {
    /// These texts read against `pack` in place of the KR pack.
    #[cfg(test)]
    fn with_pack(self, pack: PackTexts) -> Self {
        Self(self.0, pack)
    }

    fn doc(kind: &str) -> Option<&'static str> {
        CONSENT_DOCS.iter().find(|(k, _)| *k == kind).map(|(_, d)| *d)
    }

    fn texts<'a>(&'a self, kind: &str) -> impl Iterator<Item = &'a Published> + 'a {
        let doc = Self::doc(kind);
        self.0.iter().filter(move |p| Some(p.doc.as_str()) == doc)
    }

    /// What a kind needs: the newest text of its document in force that asked for re-consent, else the pack's
    /// ([`PackTexts::need`]); an unknown kind needs a version nobody holds.
    pub fn of(&self, kind: &str) -> &str {
        let Some(doc) = Self::doc(kind) else {
            return "";
        };
        self.texts(kind)
            .filter(|p| p.requires_reconsent)
            .map(|p| p.version.as_str())
            .max_by_key(|v| key(v))
            .unwrap_or_else(|| self.1.need(doc, self.texts(kind).next().is_some()))
    }

    /// The newest text in force of a kind's document: what the website and the apps show, and what a consent to "the
    /// text shown now" records. Never older than what the kind needs.
    pub fn current(&self, kind: &str) -> &str {
        let need = self.of(kind);
        self.texts(kind)
            .map(|p| p.version.as_str())
            .filter(|v| key(v) >= key(need))
            .max_by_key(|v| key(v))
            .unwrap_or(need)
    }

    /// Whether a grant naming `version` holds: the text the kind needs, or a later published text of the same document
    /// that did not ask again (a correction).
    pub fn holds(&self, kind: &str, version: &str) -> bool {
        let need = self.of(kind);
        !need.is_empty() && (version == need || self.texts(kind).any(|p| p.version == version && key(version) >= key(need)))
    }

    /// Every version a grant of `kind` may name and still hold.
    pub fn accepted(&self, kind: &str) -> Vec<String> {
        let mut out = vec![self.of(kind).to_string()];
        out.extend(self.texts(kind).filter(|p| self.holds(kind, &p.version)).map(|p| p.version.clone()));
        out.sort();
        out.dedup();
        out
    }

    /// The newest text shown of any kind. Apps before per-document versions send one version for every kind (the one
    /// `GET /me/consents` reports), so it stands for "the texts shown now" for each kind.
    pub fn bundle(&self) -> &str {
        CONSENT_DOCS.iter().map(|(k, _)| self.current(k)).max_by_key(|v| key(v)).unwrap_or_default()
    }

    /// A signup naming an older text of the kind's document (one published, or the one agreed to before published
    /// texts) is recorded while the re-consent text that replaced it has been in force under [`SIGNUP_GRACE_DAYS`],
    /// counted from its effective date (a text announced ahead replaced nothing before it).
    fn replaced_lately(&self, kind: &str, version: &str, now: DateTime<Utc>) -> bool {
        let known = version == self.1.consent_version || self.texts(kind).any(|p| p.version == version);
        let replaced_by = self
            .texts(kind)
            .filter(|p| p.requires_reconsent && key(&p.version) > key(version))
            .min_by_key(|p| key(&p.version));
        known && key(version).is_some() && replaced_by.is_some_and(|p| (now.date_naive() - p.effective_at).num_days() < SIGNUP_GRACE_DAYS)
    }
}

/// The versions each kind needs and records, from the texts in force in [`LEGAL_LANG`]: a text published with a later
/// `effective_at` changes nothing until that day (install time zone), so customers are not asked to accept it early.
pub async fn consent_versions<'e>(db: impl sqlx::PgExecutor<'e>) -> sqlx::Result<Versions> {
    let rows: Vec<Published> = sqlx::query_as(&format!(
        "SELECT doc, version, requires_reconsent, effective_at FROM legal_documents
         WHERE lang = $1 AND effective_at <= {}",
        crate::util::LOCAL_TODAY
    ))
    .bind(LEGAL_LANG)
    .fetch_all(db)
    .await?;
    Ok(Versions(rows, PACK_TEXTS.clone()))
}

/// A consent counts only if it was given for the version its kind needs now or a later correction of it.
fn effective(rows: &[Latest], versions: &Versions, kind: &str) -> bool {
    rows.iter().any(|r| r.kind == kind && r.granted && versions.holds(kind, &r.version))
}

/// Whether the customer's latest choice for `kind` is a grant for the version it needs now.
async fn allowed(db: &PgPool, user: Uuid, kind: &str) -> sqlx::Result<bool> {
    let versions = consent_versions(db).await?;
    Ok(effective(&latest(db, user).await?, &versions, kind))
}

/// Whether a marketing message may go to this customer now.
pub async fn marketing_allowed(db: &PgPool, user: Uuid) -> sqlx::Result<bool> {
    allowed(db, user, "marketing").await
}

/// Whether this customer's data may go abroad now (Google FCM push, Anthropic AI chat).
pub async fn overseas_allowed(db: &PgPool, user: Uuid) -> sqlx::Result<bool> {
    allowed(db, user, "overseas_transfer").await
}

#[derive(Serialize)]
pub struct ConsentItem {
    kind: &'static str,
    required: bool,
    granted: bool,
    granted_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
pub struct ConsentState {
    version: String,
    items: Vec<ConsentItem>,
    /// True while a required consent is missing or was given for an older policy version, or the overseas
    /// transfer grant is for an older version (a withdrawal is never asked again).
    reconsent_required: bool,
}

fn state_of(rows: &[Latest], versions: &Versions) -> ConsentState {
    let items: Vec<ConsentItem> = CONSENTS
        .iter()
        .map(|&(kind, required)| ConsentItem {
            kind,
            required,
            granted: effective(rows, versions, kind),
            granted_at: rows.iter().find(|r| r.kind == kind).map(|r| r.created_at),
        })
        .collect();
    // Overseas transfer is withdrawable, but a grant for an older policy text is asked again rather than lapsing:
    // otherwise push and the AI chat would switch off without the customer ever being told.
    let stale_overseas = rows
        .iter()
        .any(|r| r.kind == "overseas_transfer" && r.granted && !versions.holds("overseas_transfer", &r.version));
    let reconsent_required = stale_overseas || items.iter().any(|i| i.required && !i.granted);
    ConsentState {
        version: versions.bundle().to_string(),
        items,
        reconsent_required,
    }
}

pub async fn consent_state(db: &PgPool, user: Uuid) -> ApiResult<ConsentState> {
    let versions = consent_versions(db).await?;
    Ok(state_of(&latest(db, user).await?, &versions))
}

/// A consent the customer is asked to give again: a required kind not held at its current version, or an optional
/// kind granted for an older version of its document (it counts as withdrawn until accepted again). `version` is the
/// text shown now, which is what accepting records.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Pending {
    pub kind: &'static str,
    pub doc: &'static str,
    pub version: String,
    pub required: bool,
}

fn pending_of(rows: &[Latest], versions: &Versions) -> Vec<Pending> {
    CONSENTS
        .iter()
        .zip(CONSENT_DOCS)
        .filter(|&(&(kind, required), _)| {
            let stale_grant = rows.iter().any(|r| r.kind == kind && r.granted && !versions.holds(kind, &r.version));
            stale_grant || (required && !effective(rows, versions, kind))
        })
        .map(|(&(kind, required), (_, doc))| Pending {
            kind,
            doc,
            version: versions.current(kind).to_string(),
            required,
        })
        .collect()
}

/// The customer's pending consents (shown on `GET /me`).
pub async fn pending_consents(db: &PgPool, user: Uuid) -> ApiResult<Vec<Pending>> {
    let versions = consent_versions(db).await?;
    Ok(pending_of(&latest(db, user).await?, &versions))
}

#[derive(Deserialize)]
pub struct ConsentChange {
    kind: String,
    granted: bool,
    version: String,
}

#[derive(Deserialize)]
pub struct ConsentUpdate {
    consents: Vec<ConsentChange>,
}

/// Records the customer's choices. Each change must name the current policy version, so a consent is
/// always tied to the text the customer saw. The required consents must still be granted afterwards.
pub async fn update_consents(db: &PgPool, user: Uuid, update: ConsentUpdate) -> ApiResult<ConsentState> {
    if update.consents.is_empty() || update.consents.len() > CONSENTS.len() {
        return Err(ApiError::BadRequest("invalid_consents"));
    }
    let versions = consent_versions(db).await?;
    let changes = current_changes(&update.consents, &versions, None)?;
    let mut rows = latest(db, user).await?;
    for c in &changes {
        rows.retain(|r| r.kind != c.kind);
        rows.push(Latest {
            kind: c.kind.clone(),
            version: c.version.clone(),
            granted: c.granted,
            created_at: Utc::now(),
        });
    }
    // The location terms are their own agreement under 위치정보법: recording them alone never depends on the PIPA consents.
    let location_only = changes.iter().all(|c| c.kind == LOCATION);
    if !location_only && CONSENTS.iter().any(|(k, required)| *required && !effective(&rows, &versions, k)) {
        return Err(ApiError::BadRequest("required_consent"));
    }
    let mut tx = db.begin().await?;
    insert_rows(&mut tx, user, &changes).await?;
    tx.commit().await?;
    consent_state(db, user).await
}

/// Checks each change (a known kind, at most once each) and returns it with the version to record. A change naming
/// [`Versions::bundle`] records the kind's text shown now; one naming a text of its document that holds (the one it
/// needs or a later correction) is recorded as sent. At signup (`signup_at` given) an older text replaced lately is
/// recorded as sent too, and asked again at once; anything else is stale.
fn current_changes(consents: &[ConsentChange], versions: &Versions, signup_at: Option<DateTime<Utc>>) -> ApiResult<Vec<ConsentChange>> {
    let mut out = Vec::with_capacity(consents.len());
    for (i, c) in consents.iter().enumerate() {
        let Some(&(kind, _)) = CONSENT_DOCS.iter().find(|(k, _)| *k == c.kind) else {
            return Err(ApiError::BadRequest("invalid_consent_kind"));
        };
        let version = if c.version == versions.bundle() {
            versions.current(kind).to_string()
        } else if versions.holds(kind, &c.version) || signup_at.is_some_and(|now| versions.replaced_lately(kind, &c.version, now)) {
            c.version.clone()
        } else {
            return Err(ApiError::BadRequest("stale_consent_version"));
        };
        if consents[..i].iter().any(|p| p.kind == c.kind) {
            return Err(ApiError::BadRequest("duplicate_consent_kind"));
        }
        out.push(ConsentChange {
            kind: c.kind.clone(),
            granted: c.granted,
            version,
        });
    }
    Ok(out)
}

/// Records a new customer's consents inside the signup transaction, so a refused signup leaves no account. Signup
/// must grant every required kind; optional ones may be unticked. An app that still shows the text a re-consent publish
/// replaced in the last [`SIGNUP_GRACE_DAYS`] can sign up: its version is recorded as sent, and the customer is asked
/// again at once.
pub async fn record_signup(tx: &mut Transaction<'_, Postgres>, user: Uuid, consents: &[ConsentChange]) -> ApiResult<()> {
    let versions = consent_versions(&mut **tx).await?;
    let changes = check_signup_consents(consents, &versions, Utc::now())?;
    insert_rows(tx, user, &changes).await
}

/// A signup's changes with the versions to record; every required kind must be granted.
fn check_signup_consents(consents: &[ConsentChange], versions: &Versions, now: DateTime<Utc>) -> ApiResult<Vec<ConsentChange>> {
    let changes = current_changes(consents, versions, Some(now))?;
    if SIGNUP_REQUIRED.iter().any(|k| !changes.iter().any(|c| c.kind == *k && c.granted)) {
        return Err(ApiError::BadRequest("required_consent"));
    }
    Ok(changes)
}

#[derive(Deserialize)]
pub struct AcceptItem {
    kind: String,
    version: String,
}

#[derive(Deserialize)]
pub struct ConsentAccept {
    consents: Vec<AcceptItem>,
}

/// `POST /me/consents/accept`: the customer agrees to the texts named (each kind at its current version), for
/// example the pending ones `GET /me` lists. Only grants, so the required kinds can be accepted one at a time.
pub async fn accept_consents(
    State(s): State<AppState>,
    crate::auth::Customer(a): crate::auth::Customer,
    Json(b): Json<ConsentAccept>,
) -> ApiResult<Json<Value>> {
    if b.consents.is_empty() || b.consents.len() > CONSENTS.len() {
        return Err(ApiError::BadRequest("invalid_consents"));
    }
    let grants: Vec<ConsentChange> = b
        .consents
        .into_iter()
        .map(|c| ConsentChange {
            kind: c.kind,
            granted: true,
            version: c.version,
        })
        .collect();
    let changes = current_changes(&grants, &consent_versions(&s.db).await?, None)?;
    let mut tx = s.db.begin().await?;
    insert_rows(&mut tx, a.id, &changes).await?;
    tx.commit().await?;
    Ok(Json(serde_json::json!({ "pending_consents": pending_consents(&s.db, a.id).await? })))
}

async fn insert_rows(tx: &mut Transaction<'_, Postgres>, user: Uuid, consents: &[ConsentChange]) -> ApiResult<()> {
    for c in consents {
        sqlx::query("INSERT INTO user_consents (user_id, kind, version, granted) VALUES ($1,$2,$3,$4)")
            .bind(user)
            .bind(&c.kind)
            .bind(&c.version)
            .bind(c.granted)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

pub async fn get_consents(State(s): State<AppState>, crate::auth::Customer(a): crate::auth::Customer) -> ApiResult<Json<ConsentState>> {
    Ok(Json(consent_state(&s.db, a.id).await?))
}

pub async fn put_consents(
    State(s): State<AppState>,
    crate::auth::Customer(a): crate::auth::Customer,
    Json(u): Json<ConsentUpdate>,
) -> ApiResult<Json<ConsentState>> {
    Ok(Json(update_consents(&s.db, a.id, u).await?))
}

/// 열람 요구: everything the service holds about the customer, without credentials, gateway keys or other customers' data.
/// Shared by the customer's own download and by staff handling a request for that customer. Every section is scoped to
/// `$1` directly or through the customer's own bookings, checkouts or chat threads; replies keep no staff identity.
pub async fn export_doc(db: &PgPool, user: Uuid) -> ApiResult<Value> {
    let doc: Value = sqlx::query_scalar(
        "SELECT jsonb_build_object(
          'exported_at', now(),
          'profile', (SELECT jsonb_build_object('phone', phone, 'name', name, 'email', email, 'locale', locale, 'created_at', created_at, 'identity_verified_at', identity_verified_at) FROM users WHERE id = $1),
          'consents', (SELECT COALESCE(jsonb_agg(jsonb_build_object('kind', kind, 'version', version, 'granted', granted, 'at', created_at) ORDER BY created_at), '[]'::jsonb) FROM user_consents WHERE user_id = $1),
          'addresses', (SELECT COALESCE(jsonb_agg(jsonb_build_object('kind', kind, 'label', label, 'road_address', road_address, 'detail', detail, 'district_code', district_code, 'size_pyeong', size_pyeong, 'household', household, 'created_at', created_at) ORDER BY created_at), '[]'::jsonb) FROM addresses WHERE user_id = $1 AND deleted_at IS NULL),
          'payment_methods', (SELECT COALESCE(jsonb_agg(jsonb_build_object('provider', provider, 'display_name', display_name, 'created_at', created_at) ORDER BY created_at), '[]'::jsonb) FROM payment_methods WHERE user_id = $1 AND deleted_at IS NULL),
          'coupons', (SELECT COALESCE(jsonb_agg(jsonb_build_object('name', name, 'amount', amount, 'service_code', service_code, 'issued_at', issued_at, 'expires_at', expires_at, 'used', used_booking_id IS NOT NULL) ORDER BY issued_at), '[]'::jsonb) FROM coupons WHERE user_id = $1),
          'bookings', (SELECT COALESCE(jsonb_agg(jsonb_build_object('number', number, 'service_code', service_code, 'visit_date', visit_date, 'start_hour', start_hour, 'status', status, 'address', address, 'answers', answers, 'final_price', final_price, 'paid_amount', paid_amount, 'created_at', created_at) ORDER BY created_at), '[]'::jsonb) FROM bookings WHERE user_id = $1),
          'notifications', (SELECT COALESCE(jsonb_agg(jsonb_build_object('kind', kind, 'data', data, 'created_at', created_at) ORDER BY created_at DESC), '[]'::jsonb) FROM (SELECT kind, data, created_at FROM notifications WHERE owner_role = 'customer' AND owner_id = $1 ORDER BY created_at DESC LIMIT 500) n),
          'subscriptions', (SELECT COALESCE(jsonb_agg(jsonb_build_object('service_code', s.service_code, 'unit_index', s.unit_index, 'address', (SELECT a.road_address FROM addresses a WHERE a.id = s.address_id), 'cadence_weeks', s.cadence_weeks, 'weekday', s.weekday, 'start_hour', s.start_hour, 'status', s.status, 'paused_reason', s.paused_reason, 'created_at', s.created_at, 'cancelled_at', s.cancelled_at) ORDER BY s.created_at), '[]'::jsonb) FROM subscriptions s WHERE s.user_id = $1),
          'waitlist', (SELECT COALESCE(jsonb_agg(jsonb_build_object('service_code', w.service_code, 'visit_date', w.visit_date, 'start_hour', w.start_hour, 'end_hour', w.end_hour, 'status', w.status, 'body', w.body, 'joined_at', w.joined_at) ORDER BY w.joined_at), '[]'::jsonb) FROM waitlist_entries w WHERE w.user_id = $1),
          'reviews', (SELECT COALESCE(jsonb_agg(jsonb_build_object('booking_id', r.booking_id, 'rating', r.rating, 'body', r.body, 'photos', r.photos, 'created_at', r.created_at) ORDER BY r.created_at), '[]'::jsonb) FROM reviews r WHERE r.user_id = $1),
          'chat_messages', (SELECT COALESCE(jsonb_agg(jsonb_build_object('thread_id', m.thread_id, 'kind', m.kind, 'body', m.body, 'created_at', m.created_at) ORDER BY m.created_at), '[]'::jsonb) FROM chat_messages m WHERE m.sender = 'customer' AND m.sender_id = $1),
          'uploads', (SELECT COALESCE(jsonb_agg(jsonb_build_object('bytes', u.bytes, 'created_at', u.created_at) ORDER BY u.created_at), '[]'::jsonb) FROM uploads u WHERE u.owner_role = 'customer' AND u.owner_id = $1),
          'points', (SELECT COALESCE(jsonb_agg(jsonb_build_object('kind', p.kind, 'amount', p.amount, 'expires_at', p.expires_at, 'booking_id', p.booking_id, 'reason', p.reason, 'created_at', p.created_at) ORDER BY p.created_at), '[]'::jsonb) FROM points_ledger p WHERE p.user_id = $1),
          'tips', (SELECT COALESCE(jsonb_agg(jsonb_build_object('booking_id', t.booking_id, 'amount', t.amount, 'created_at', t.created_at) ORDER BY t.created_at), '[]'::jsonb) FROM booking_tips t WHERE t.user_id = $1),
          'cash_receipts', (SELECT COALESCE(jsonb_agg(jsonb_build_object('purpose', c.purpose, 'identity_type', c.identity_type, 'identity_last4', c.identity_last4, 'state', c.state, 'issue_number', c.issue_number, 'receipt_url', c.receipt_url, 'created_at', c.created_at) ORDER BY c.created_at), '[]'::jsonb) FROM cash_receipts c JOIN checkouts k ON k.id = c.checkout_id WHERE k.user_id = $1),
          'tax_invoice_requests', (SELECT COALESCE(jsonb_agg(jsonb_build_object('business_number', t.business_number, 'company_name', t.company_name, 'representative', t.representative, 'business_type', t.business_type, 'business_item', t.business_item, 'address', t.address, 'email', t.email, 'state', t.state, 'issued_on', t.issued_on, 'created_at', t.created_at) ORDER BY t.created_at), '[]'::jsonb) FROM tax_invoice_requests t JOIN checkouts k ON k.id = t.checkout_id WHERE k.user_id = $1),
          'referrals', (SELECT COALESCE(jsonb_agg(jsonb_build_object('role', r.role, 'state', r.state, 'created_at', r.created_at, 'rewarded_at', r.rewarded_at) ORDER BY r.created_at), '[]'::jsonb) FROM (SELECT 'referrer' AS role, state, created_at, rewarded_at FROM referrals WHERE referrer_id = $1 UNION ALL SELECT 'referred', state, created_at, rewarded_at FROM referrals WHERE referred_user_id = $1) r),
          'checkouts', (SELECT COALESCE(jsonb_agg(jsonb_build_object('kind', k.kind, 'booking', (SELECT b.number FROM bookings b WHERE b.id = k.booking_id), 'method', k.method, 'amount', k.amount, 'points', k.points, 'status', k.status, 'refund_reason', k.refund_reason, 'created_at', k.created_at) ORDER BY k.created_at), '[]'::jsonb) FROM checkouts k WHERE k.user_id = $1),
          'payments', (SELECT COALESCE(jsonb_agg(jsonb_build_object('booking', b.number, 'kind', p.kind, 'amount', p.amount, 'status', p.status, 'refunded_amount', p.refunded_amount, 'provider', p.provider, 'created_at', p.created_at) ORDER BY p.created_at), '[]'::jsonb) FROM payments p JOIN bookings b ON b.id = p.booking_id WHERE b.user_id = $1),
          'refunds', (SELECT COALESCE(jsonb_agg(jsonb_build_object('amount', r.amount, 'requested_at', r.created_at, 'done_at', r.done_at) ORDER BY r.created_at), '[]'::jsonb) FROM refund_queue r JOIN checkouts k ON k.id = r.checkout_id WHERE k.user_id = $1),
          'reports', (SELECT COALESCE(jsonb_agg(jsonb_build_object('kind', c.kind, 'booking', (SELECT b.number FROM bookings b WHERE b.id = c.booking_id), 'summary', c.summary, 'photos', c.photos, 'status', c.status, 'created_at', c.created_at, 'resolved_at', c.resolved_at) ORDER BY c.created_at), '[]'::jsonb) FROM cases c WHERE c.reporter_role = 'customer' AND c.reporter_id = $1),
          'marketing_sends', (SELECT COALESCE(jsonb_agg(jsonb_build_object('campaign', (SELECT g.name FROM campaigns g WHERE g.id = m.campaign_id), 'channel', m.channel, 'status', m.status, 'created_at', m.created_at) ORDER BY m.created_at), '[]'::jsonb) FROM marketing_sends m WHERE m.user_id = $1),
          'chat_replies', (SELECT COALESCE(jsonb_agg(jsonb_build_object('thread_id', m.thread_id, 'sender', m.sender, 'kind', m.kind, 'body', m.body, 'created_at', m.created_at) ORDER BY m.created_at), '[]'::jsonb) FROM chat_messages m JOIN chat_threads t ON t.id = m.thread_id WHERE t.audience = 'customer' AND t.user_id = $1 AND m.sender <> 'customer')
        )",
    )
    .bind(user)
    .fetch_one(db)
    .await?;
    Ok(doc)
}

pub async fn export(State(s): State<AppState>, crate::auth::Customer(a): crate::auth::Customer) -> ApiResult<Json<Value>> {
    Ok(Json(export_doc(&s.db, a.id).await?))
}

/// Revokes one saved card's key at the gateway it was saved on (`payment_methods.gateway`). Fails closed: an error
/// means the key may still be live, so the caller must keep its local copy and the customer can retry.
pub(crate) async fn revoke_billing_key(s: &AppState, method: Uuid, gateway: &str, stored: &str) -> ApiResult<()> {
    // Dev mode saves no gateway keys (the test cards are local rows), so there is nothing to revoke.
    if s.config.dev_mode {
        return Ok(());
    }
    // Deleting a key needs only that gateway's secret, so a store or channel that is off does not matter. Without
    // it the key stays live at the gateway: refuse, so the card is not erased while still billable.
    let integ = s.providers.integ();
    let gw = Gateway::parse(gateway).unwrap_or(Gateway::Portone);
    let secret_set = match gw {
        Gateway::Portone => integ.portone.api_secret.is_some(),
        Gateway::TossPayments => integ.toss_payments.as_ref().is_some_and(|t| t.billing.is_some()),
    };
    if !secret_set {
        tracing::warn!(payment_method = %method, gateway, "billing key cannot be deleted: the gateway's secret is not configured");
        return Err(ApiError::Unavailable("billing_key_delete_failed"));
    }
    let Some(key) = crypto::open_billing_key(&s.config.data_key, stored) else {
        tracing::error!(payment_method = %method, "saved-card billing key cannot be opened: revoke blocked");
        return Err(ApiError::Unavailable("billing_key_delete_failed"));
    };
    s.providers
        .delete_billing_key(gw, &key)
        .await
        .map_err(|_| ApiError::Unavailable("billing_key_delete_failed"))
}

/// Deletes the customer's gateway keys (saved cards) before personal data is removed. Fails closed:
/// if the gateway refuses, nothing is anonymised and the customer can retry.
async fn delete_billing_keys(s: &AppState, user: Uuid) -> ApiResult<()> {
    let keys: Vec<(Uuid, String, String)> = sqlx::query_as("SELECT id, billing_key, gateway FROM payment_methods WHERE user_id = $1 AND billing_key <> ''")
        .bind(user)
        .fetch_all(&s.db)
        .await?;
    for (id, stored, gateway) in &keys {
        revoke_billing_key(s, *id, gateway, stored).await?;
    }
    Ok(())
}

/// Account deletion (PIPA 파기). Anonymises the user row and removes contact data, sign-in identities (then
/// disconnects them at the provider), addresses, devices, notifications, uploads, gateway keys, review text and
/// photos, waitlist and unpaid-checkout snapshots; ends subscriptions (no further charge or visit is planned) and
/// waitlist places. Kept for legal retention, linked only to the anonymised row: bookings, payments, paid
/// checkouts, tips, cash receipts, tax invoice requests and payouts (전자상거래법 5년, 부가가치세법), review ratings,
/// the consent log (proof of consent), chat text with the sender unlinked (분쟁 처리), and a referral's phone only
/// as a keyed digest (`crypto::phone_digest`) so the one-reward-per-phone rule still holds. A finished booking
/// also loses its street address, answers, visit notes and household (`pseudonymise_closed_bookings`). Safe to run twice.
pub async fn delete_account(s: &AppState, user: Uuid) -> ApiResult<()> {
    delete_billing_keys(s, user).await?;
    let mut tx = s.db.begin().await?;
    let links = crate::providers::social::links(&mut tx, &s.config.data_key, "customer", user).await?;
    // Already anonymised (a replayed Apple notification, or a concurrent deletion that committed first): nothing
    // left to do, so roll back instead of repeating the cascade. Replays must succeed, so this is Ok, not an error.
    if !anonymise_user_row(&mut tx, user).await? {
        return Ok(());
    }
    pseudonymise_referral_phone(&mut tx, s, user).await?;
    remove_sign_in_and_devices(&mut tx, user).await?;
    let offered = end_commitments(&mut tx, user).await?;
    clear_customer_content(&mut tx, user).await?;
    pseudonymise_closed_bookings(&mut *tx, Some(user)).await?;
    tx.commit().await?;
    // Network and disk work after commit: no row lock is held across it.
    crate::providers::social::unlink(&s.providers, "customer", user, links).await;
    delete_uploads(s, user).await?;
    // An offered waitlist seat was being held for this customer: offer it to the next waiter.
    for (date, hour) in offered {
        crate::util::best_effort(
            "waitlist offer on account deletion",
            crate::routes::waitlist::offer_for_slot(s, date, hour as u8),
        )
        .await;
    }
    Ok(())
}

/// Clears the login codes and the contact data on the user row. Returns false when the row was already anonymised.
async fn anonymise_user_row(tx: &mut Transaction<'_, Postgres>, user: Uuid) -> ApiResult<bool> {
    // Login codes are keyed by phone, so they go before the phone is wiped.
    sqlx::query("DELETE FROM otp_codes WHERE phone = (SELECT phone FROM users WHERE id = $1)")
        .bind(user)
        .execute(&mut **tx)
        .await?;
    let anonymised = sqlx::query(
        "UPDATE users SET deleted_at = now(), name = '', email = NULL, phone = 'deleted:' || id::text, phone_e164 = NULL, ci_hash = NULL,
         identity_verified_at = NULL, referral_code = NULL, acquisition = NULL WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(user)
    .execute(&mut **tx)
    .await?;
    Ok(anonymised.rows_affected() > 0)
}

/// A referral keeps one-reward-per-phone only as a keyed digest, so the plain phone is replaced by it.
async fn pseudonymise_referral_phone(tx: &mut Transaction<'_, Postgres>, s: &AppState, user: Uuid) -> ApiResult<()> {
    let referral_phone: Option<String> =
        sqlx::query_scalar("SELECT referred_phone FROM referrals WHERE referred_user_id = $1 AND referred_phone NOT LIKE 'hmac:%' FOR UPDATE")
            .bind(user)
            .fetch_optional(&mut **tx)
            .await?;
    if let Some(phone) = referral_phone {
        sqlx::query("UPDATE referrals SET referred_phone = $2, referred_phone_e164 = NULL WHERE referred_user_id = $1")
            .bind(user)
            .bind(crate::crypto::phone_digest(&s.config.data_key, &phone))
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

/// Sign-in identities, sessions, devices, notifications and refresh tokens. Undelivered notifications go too,
/// or a retry would write a new inbox row after the deletion.
async fn remove_sign_in_and_devices(tx: &mut Transaction<'_, Postgres>, user: Uuid) -> ApiResult<()> {
    // Provider ids and emails; their stored refresh tokens go with them (ON DELETE CASCADE).
    sqlx::query("DELETE FROM user_identities WHERE role = 'customer' AND subject_id = $1")
        .bind(user)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM identity_sessions WHERE user_id = $1")
        .bind(user)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM billing_issues WHERE user_id = $1")
        .bind(user)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM push_tokens WHERE owner_role = 'customer' AND owner_id = $1")
        .bind(user)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM notifications WHERE owner_role = 'customer' AND owner_id = $1")
        .bind(user)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM notification_outbox WHERE owner_role = 'customer' AND owner_id = $1")
        .bind(user)
        .execute(&mut **tx)
        .await?;
    sqlx::query("UPDATE refresh_tokens SET revoked_at = now() WHERE subject = $1 AND revoked_at IS NULL")
        .bind(user)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Ends subscriptions, planned visits and waitlist places. Returns the offered seats, which the caller re-offers
/// once the transaction has committed.
async fn end_commitments(tx: &mut Transaction<'_, Postgres>, user: Uuid) -> ApiResult<Vec<(chrono::NaiveDate, i16)>> {
    sqlx::query(
        "UPDATE subscriptions SET status = 'cancelled', paused_reason = NULL, cancelled_at = now(), updated_at = now()
         WHERE user_id = $1 AND status <> 'cancelled'",
    )
    .bind(user)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "UPDATE subscription_visits SET state = 'cancelled', updated_at = now()
         WHERE state = 'planned' AND subscription_id IN (SELECT id FROM subscriptions WHERE user_id = $1)",
    )
    .bind(user)
    .execute(&mut **tx)
    .await?;
    let offered: Vec<(chrono::NaiveDate, i16)> =
        sqlx::query_as("UPDATE waitlist_entries SET status = 'cancelled' WHERE user_id = $1 AND status = 'offered' RETURNING visit_date, start_hour")
            .bind(user)
            .fetch_all(&mut **tx)
            .await?;
    sqlx::query("UPDATE waitlist_entries SET status = 'cancelled' WHERE user_id = $1 AND status = 'waiting'")
        .bind(user)
        .execute(&mut **tx)
        .await?;
    // The booking request snapshot holds the address and notes.
    sqlx::query("UPDATE waitlist_entries SET body = '{}'::jsonb WHERE user_id = $1")
        .bind(user)
        .execute(&mut **tx)
        .await?;
    Ok(offered)
}

/// Customer-written text, photos, addresses, saved card labels and chat sender links.
async fn clear_customer_content(tx: &mut Transaction<'_, Postgres>, user: Uuid) -> ApiResult<()> {
    // Unsettled checkouts may still be paid (late deposit), so their draft stays until retention clears it.
    sqlx::query(
        "UPDATE checkouts SET draft = NULL, virtual_account = NULL
         WHERE user_id = $1 AND status IN ('failed','cancelled','refunded') AND (draft IS NOT NULL OR virtual_account IS NOT NULL)",
    )
    .bind(user)
    .execute(&mut **tx)
    .await?;
    // Ratings stay for the partner's score; the customer's words and photos go.
    sqlx::query("UPDATE reviews SET body = '', photos = '[]'::jsonb WHERE user_id = $1")
        .bind(user)
        .execute(&mut **tx)
        .await?;
    // A subscription references its address, so that row is emptied instead of deleted.
    sqlx::query("UPDATE addresses SET label = '', road_address = '', detail = '', deleted_at = COALESCE(deleted_at, now()) WHERE user_id = $1")
        .bind(user)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM addresses a WHERE a.user_id = $1 AND NOT EXISTS (SELECT 1 FROM subscriptions s WHERE s.address_id = a.id)")
        .bind(user)
        .execute(&mut **tx)
        .await?;
    sqlx::query("UPDATE payment_methods SET deleted_at = COALESCE(deleted_at, now()), billing_key = '', billing_key_digest = NULL, display_name = '' WHERE user_id = $1")
        .bind(user)
        .execute(&mut **tx)
        .await?;
    // Threads keep their user_id: chat_threads_check requires it for customer threads, and the anonymised user row holds no contact data.
    sqlx::query("UPDATE chat_messages SET sender_id = NULL WHERE sender = 'customer' AND sender_id = $1")
        .bind(user)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Finished bookings (completed or cancelled) of deleted customers lose the street address, the answers,
/// the visit notes (entrance instructions), the household profile and the partner's team memo. Amounts, dates, service code, status,
/// the district and the size stay for the 전자상거래법 record. Answers become `{}` rather than NULL so a
/// completed quote still reads as a quote to the partner. Open bookings are skipped: the visit and the
/// quote-job payout rule still read them. Idempotent: a row is matched only while it still holds personal
/// fields. `user` limits the sweep to one customer (account deletion); `None` sweeps all (`retention::purge`).
pub(crate) async fn pseudonymise_closed_bookings<'c>(conn: impl sqlx::PgExecutor<'c>, user: Option<Uuid>) -> sqlx::Result<u64> {
    let done = sqlx::query(
        "UPDATE bookings SET
           address = jsonb_build_object('district_code', district_code, 'size_pyeong', address -> 'size_pyeong', 'kind', address -> 'kind'),
           answers = CASE WHEN answers IS NULL THEN NULL ELSE '{}'::jsonb END,
           visit_notes = '{}'::jsonb, household = '{}'::jsonb, team_memo = ''
         WHERE status IN ($1, $2)
           AND (answers IS NOT NULL AND answers <> '{}'::jsonb OR address ->> 'road_address' IS NOT NULL
                OR visit_notes <> '{}'::jsonb OR household <> '{}'::jsonb OR team_memo <> '')
           AND user_id IN (SELECT id FROM users WHERE deleted_at IS NOT NULL)
           AND ($3::uuid IS NULL OR user_id = $3)",
    )
    .bind(BookingStatus::Completed.as_i16())
    .bind(BookingStatus::Cancelled.as_i16())
    .bind(user)
    .execute(conn)
    .await?;
    Ok(done.rows_affected())
}

/// The customer's uploaded photos (review, chat, booking): file first, then its row, so a failed removal
/// leaves the row and a second deletion run retries it.
async fn delete_uploads(s: &AppState, user: Uuid) -> ApiResult<()> {
    let files: Vec<(Uuid, String)> = sqlx::query_as("SELECT id, file FROM uploads WHERE owner_role = 'customer' AND owner_id = $1")
        .bind(user)
        .fetch_all(&s.db)
        .await?;
    for (id, file) in files {
        match tokio::fs::remove_file(s.config.upload_dir.join(&file)).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                tracing::warn!(%user, error = %e, "upload not removed on account deletion");
                continue;
            }
        }
        sqlx::query("DELETE FROM uploads WHERE id = $1").bind(id).execute(&s.db).await?;
    }
    Ok(())
}

/// Retention step: partners whose Apple ID was deleted while a job or payout was open are deleted once that
/// clears. Still blocked ones wait for the next run. Returns how many were deleted.
pub async fn finish_pending_partner_deletions(s: &AppState) -> ApiResult<u64> {
    let pending: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM pros WHERE deletion_requested_at IS NOT NULL AND deleted_at IS NULL")
        .fetch_all(&s.db)
        .await?;
    let mut done = 0;
    for id in pending {
        match crate::routes::partner::delete_pro(s, id).await {
            Ok(()) => done += 1,
            Err(ApiError::Conflict(_)) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(done)
}

async fn end_sessions(db: &PgPool, subject: Uuid) -> sqlx::Result<()> {
    sqlx::query("UPDATE refresh_tokens SET revoked_at = now() WHERE subject = $1 AND revoked_at IS NULL")
        .bind(subject)
        .execute(db)
        .await
        .map(|_| ())
}

/// Acts on a verified Apple server-to-server notification, once per `jti` (Apple may deliver it again).
/// `account-delete`: the Apple ID is gone, so the accounts signed in with it are deleted. A partner whose
/// deletion is blocked by an open job or payout is marked and deleted by the retention job once that clears.
/// `consent-revoked`: the user stopped using Apple sign-in here, so their sessions end and the stored Apple
/// token is dropped; the link stays so signing in again reaches the same account. Email relay changes need
/// nothing from us. On any error the `jti` is released so Apple's retry is handled.
pub async fn apple_notification(s: &AppState, n: &crate::providers::apple::Notification) -> ApiResult<()> {
    let claimed: Option<String> = sqlx::query_scalar("INSERT INTO apple_notifications (jti, event_type) VALUES ($1, $2) ON CONFLICT DO NOTHING RETURNING jti")
        .bind(&n.jti)
        .bind(&n.kind)
        .fetch_optional(&s.db)
        .await?;
    if claimed.is_none() {
        return Ok(());
    }
    let handled = handle_apple_event(s, n).await;
    if handled.is_err() {
        sqlx::query("DELETE FROM apple_notifications WHERE jti = $1")
            .bind(&n.jti)
            .execute(&s.db)
            .await?;
    }
    handled
}

async fn handle_apple_event(s: &AppState, n: &crate::providers::apple::Notification) -> ApiResult<()> {
    let accounts: Vec<(Uuid, String, Uuid)> =
        sqlx::query_as("SELECT id, role, subject_id FROM user_identities WHERE provider = 'apple' AND provider_user_id = $1")
            .bind(&n.sub)
            .fetch_all(&s.db)
            .await?;
    for (identity, role, subject) in accounts {
        match (n.kind.as_str(), role.as_str()) {
            ("account-delete" | "account-deleted", "customer") => delete_account(s, subject).await?,
            ("account-delete" | "account-deleted", _) => match crate::routes::partner::delete_pro(s, subject).await {
                Ok(()) => {}
                Err(ApiError::Conflict(reason)) => {
                    tracing::warn!(%subject, reason, "apple account-delete: partner deletion waits for open work to clear");
                    sqlx::query("UPDATE pros SET deletion_requested_at = COALESCE(deletion_requested_at, now()) WHERE id = $1")
                        .bind(subject)
                        .execute(&s.db)
                        .await?;
                }
                Err(e) => return Err(e),
            },
            ("consent-revoked", _) => {
                sqlx::query("DELETE FROM social_tokens WHERE identity_id = $1")
                    .bind(identity)
                    .execute(&s.db)
                    .await?;
                end_sessions(&s.db, subject).await?;
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(kind: &str, granted: bool) -> ConsentChange {
        ConsentChange {
            kind: kind.into(),
            granted,
            version: CONSENT_VERSION.into(),
        }
    }

    fn required_set() -> Vec<ConsentChange> {
        vec![change("terms", true), change("privacy", true), change("overseas_transfer", true)]
    }

    fn refusal<T>(r: ApiResult<T>) -> Option<&'static str> {
        match r {
            Err(ApiError::BadRequest(code)) => Some(code),
            Err(_) => Some("other"),
            Ok(_) => None,
        }
    }

    /// A published text of `doc` in force since `days_ago`.
    fn text(doc: &str, version: &str, reconsent: bool, days_ago: i64) -> Published {
        Published {
            doc: doc.into(),
            version: version.into(),
            requires_reconsent: reconsent,
            effective_at: (Utc::now() - chrono::Duration::days(days_ago)).date_naive(),
        }
    }

    fn texts(t: Vec<Published>) -> Versions {
        Versions(t, PACK_TEXTS.clone())
    }

    /// A new privacy text that asks again, in force today.
    fn new_privacy() -> Versions {
        texts(vec![text("privacy", "2026-12-01", true, 0)])
    }

    /// A release that changes a pack text moves only what a document nobody published needs; with texts recorded, a
    /// kind keeps needing what its customers agreed to, and the constant plays no part.
    #[test]
    fn a_pack_version_bump_moves_only_unpublished_documents() {
        let bumped = PACK_TEXTS.clone().with_template("privacy", "2026-12-01");
        let none = Versions::default().with_pack(bumped.clone());
        assert_eq!((none.of("privacy"), none.current("privacy")), ("2026-12-01", "2026-12-01"), "the text shown");
        assert_eq!(none.of("terms"), CONSENT_VERSION);
        let fix = texts(vec![text("privacy", "2026-12-01", false, 1)]).with_pack(bumped);
        assert_eq!(
            (fix.of("privacy"), fix.current("privacy")),
            (CONSENT_VERSION, "2026-12-01"),
            "a recorded correction"
        );
        let mut agreed = PACK_TEXTS.clone();
        agreed.consent_version = "2026-11-01".into();
        let v = texts(vec![text("privacy", "2026-11-17", false, 1)]).with_pack(agreed);
        assert_eq!(v.of("privacy"), "2026-11-01", "the pack's agreed version, not the constant");
        assert!(v.holds("privacy", "2026-11-01") && v.holds("privacy", "2026-11-17"));
    }

    /// The grace period runs from when the replacing text took effect, not from when it was published.
    #[test]
    fn signup_grace_counts_from_the_effective_date() {
        let mut later = text("privacy", "2026-12-01", true, 0);
        later.effective_at = (Utc::now() - chrono::Duration::days(SIGNUP_GRACE_DAYS - 1)).date_naive();
        assert!(texts(vec![later.clone()]).replaced_lately("privacy", CONSENT_VERSION, Utc::now()));
        later.effective_at = (Utc::now() - chrono::Duration::days(SIGNUP_GRACE_DAYS)).date_naive();
        assert!(!texts(vec![later]).replaced_lately("privacy", CONSENT_VERSION, Utc::now()));
    }

    fn signup(set: &[ConsentChange]) -> ApiResult<Vec<ConsentChange>> {
        check_signup_consents(set, &Versions::default(), Utc::now())
    }

    fn row(kind: &str, granted: bool, version: &str) -> Latest {
        Latest {
            kind: kind.into(),
            version: version.into(),
            granted,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn overseas_grant_for_an_older_policy_text_asks_again() {
        let current = |overseas: Latest| vec![row("terms", true, CONSENT_VERSION), row("privacy", true, CONSENT_VERSION), overseas];
        let v = Versions::default();
        let state_of = |rows: &[Latest]| state_of(rows, &v);
        let stale = state_of(&current(row("overseas_transfer", true, "2000-01-01")));
        assert!(stale.reconsent_required, "push and AI must not switch off silently after a policy change");
        assert!(
            !stale.items.iter().any(|i| i.kind == "overseas_transfer" && i.granted),
            "an old grant does not count"
        );
        assert!(!state_of(&current(row("overseas_transfer", true, CONSENT_VERSION))).reconsent_required);
        assert!(
            !state_of(&current(row("overseas_transfer", false, "2000-01-01"))).reconsent_required,
            "a withdrawal is never asked again"
        );
        assert!(
            !state_of(&current(row("marketing", true, "2000-01-01"))).reconsent_required,
            "an old marketing grant simply lapses"
        );
    }

    /// A new privacy text that asks for re-consent moves privacy, overseas transfer and marketing (its document), not
    /// terms or location.
    #[test]
    fn a_new_privacy_version_asks_again_for_the_kinds_behind_it() {
        let v = new_privacy();
        assert_eq!(v.bundle(), "2026-12-01");
        let rows = [
            row("terms", true, CONSENT_VERSION),
            row("privacy", true, CONSENT_VERSION),
            row("overseas_transfer", true, CONSENT_VERSION),
            row("marketing", true, CONSENT_VERSION),
            row("location", false, CONSENT_VERSION),
        ];
        let s = state_of(&rows, &v);
        assert!(s.reconsent_required);
        assert_eq!(s.version, "2026-12-01");
        let granted: Vec<&str> = s.items.iter().filter(|i| i.granted).map(|i| i.kind).collect();
        assert_eq!(granted, ["terms"], "older grants behind the new text count as withdrawn");
        let pending: Vec<(&str, &str, bool)> = pending_of(&rows, &v).iter().map(|p| (p.kind, p.doc, p.required)).collect();
        assert_eq!(
            pending,
            [
                ("privacy", "privacy", true),
                ("overseas_transfer", "privacy", false),
                ("marketing", "privacy", false)
            ],
            "a withdrawn location consent is not asked"
        );
        assert!(pending_of(&rows, &Versions::default()).is_empty(), "nothing pending on the built-in versions");
    }

    #[test]
    fn a_single_version_from_an_older_app_records_each_kinds_own_version() {
        let v = new_privacy();
        let sent = vec![
            ConsentChange {
                kind: "terms".into(),
                granted: true,
                version: "2026-12-01".into(),
            },
            ConsentChange {
                kind: "privacy".into(),
                granted: true,
                version: "2026-12-01".into(),
            },
        ];
        let out = current_changes(&sent, &v, None).expect("bundle version accepted");
        let recorded: Vec<(&str, &str)> = out.iter().map(|c| (c.kind.as_str(), c.version.as_str())).collect();
        assert_eq!(recorded, [("terms", CONSENT_VERSION), ("privacy", "2026-12-01")]);
        assert_eq!(refusal(current_changes(&[change("privacy", true)], &v, None)), Some("stale_consent_version"));
    }

    #[test]
    fn signup_from_an_app_on_an_earlier_published_text_is_recorded_as_sent() {
        let v = new_privacy();
        let out = check_signup_consents(&required_set(), &v, Utc::now()).expect("older text replaced today accepted");
        assert!(out.iter().all(|c| c.version == CONSENT_VERSION), "recorded as the customer saw it");
        let mut bogus = required_set();
        bogus[1].version = "2020-01-01".into();
        assert_eq!(refusal(check_signup_consents(&bogus, &v, Utc::now())), Some("stale_consent_version"));
    }

    /// Years after a re-consent publish, a signup naming only the replaced text is refused: the account would otherwise
    /// be processed under a policy its customer never agreed to.
    #[test]
    fn signup_names_a_replaced_text_only_within_the_grace_period() {
        let late = texts(vec![text("privacy", "2026-12-01", true, SIGNUP_GRACE_DAYS + 1)]);
        assert_eq!(
            refusal(check_signup_consents(&required_set(), &late, Utc::now())),
            Some("stale_consent_version")
        );
        let twice = texts(vec![text("privacy", "2026-12-01", true, 400), text("privacy", "2027-06-01", true, 1)]);
        let mut set = required_set();
        set[1].version = "2026-12-01".into();
        set[2].version = "2026-12-01".into();
        assert!(check_signup_consents(&set, &twice, Utc::now()).is_ok(), "the text replaced yesterday");
        set[1].version = CONSENT_VERSION.into();
        assert_eq!(
            refusal(check_signup_consents(&set, &twice, Utc::now())),
            Some("stale_consent_version"),
            "a text replaced long ago, even with a newer publish since"
        );
    }

    /// A publish without re-consent (a correction) is the text shown: signup, update and accept naming it record it,
    /// and it holds, so nothing is pending; grants on the text it corrects still hold too.
    #[test]
    fn a_correction_is_recorded_and_holds_without_asking_again() {
        let v = texts(vec![text("privacy", CONSENT_VERSION, true, 10), text("privacy", "2026-11-20", false, 1)]);
        assert_eq!(
            (v.of("privacy"), v.current("privacy"), v.bundle()),
            (CONSENT_VERSION, "2026-11-20", "2026-11-20")
        );
        assert_eq!((v.of("terms"), v.current("terms")), (CONSENT_VERSION, CONSENT_VERSION));
        let mut set = required_set();
        set[1].version = "2026-11-20".into();
        set[2].version = "2026-11-20".into();
        let out = check_signup_consents(&set, &v, Utc::now()).expect("the text shown");
        let recorded: Vec<&str> = out.iter().map(|c| c.version.as_str()).collect();
        assert_eq!(recorded, [CONSENT_VERSION, "2026-11-20", "2026-11-20"]);
        assert!(current_changes(&set[1..], &v, None).is_ok(), "update and accept take it too");
        let rows = [
            row("terms", true, CONSENT_VERSION),
            row("privacy", true, "2026-11-20"),
            row("overseas_transfer", true, CONSENT_VERSION),
            row("marketing", true, "2026-11-20"),
        ];
        assert!(pending_of(&rows, &v).is_empty());
        assert!(!state_of(&rows, &v).reconsent_required);
        assert_eq!(v.accepted("marketing"), [CONSENT_VERSION, "2026-11-20"]);
        assert!(!v.holds("privacy", "2026-11-19"), "a text never published");
    }

    /// Pending names the text shown now (a correction after the re-consent text), which accepting records.
    #[test]
    fn pending_names_the_newest_text_shown() {
        let v = texts(vec![text("privacy", "2026-12-01", true, 2), text("privacy", "2026-12-03", false, 1)]);
        let rows = [row("terms", true, CONSENT_VERSION), row("privacy", true, CONSENT_VERSION)];
        let pending = pending_of(&rows, &v);
        assert_eq!(
            pending[0],
            Pending {
                kind: "privacy",
                doc: "privacy",
                version: "2026-12-03".into(),
                required: true
            }
        );
        assert!(v.holds("privacy", "2026-12-01") && v.holds("privacy", "2026-12-03"));
    }

    #[test]
    fn versions_compare_by_date_then_counter_not_as_text() {
        let v = texts(vec![text("privacy", "2026-12-01.9", true, 2), text("terms", "2026-12-01.10", true, 1)]);
        assert_eq!(v.bundle(), "2026-12-01.10");
        let v = texts(vec![text("privacy", "2026-12-01.10", true, 2), text("privacy", "2026-12-01.9", false, 1)]);
        assert_eq!(v.of("privacy"), "2026-12-01.10");
        assert_eq!(v.current("privacy"), "2026-12-01.10", ".9 is older");
        assert!(!v.holds("privacy", "2026-12-01.9"));
    }

    #[test]
    fn signup_with_required_consents_passes_without_marketing() {
        assert_eq!(refusal(signup(&required_set())), None);
    }

    #[test]
    fn signup_missing_any_required_consent_is_refused() {
        for kind in ["terms", "privacy", "overseas_transfer"] {
            let mut set = required_set();
            set.retain(|c| c.kind != kind);
            assert_eq!(refusal(signup(&set)), Some("required_consent"), "missing {kind}");
        }
    }

    #[test]
    fn signup_with_a_required_consent_declined_is_refused() {
        let mut set = required_set();
        set[2].granted = false;
        assert_eq!(refusal(signup(&set)), Some("required_consent"));
    }

    #[test]
    fn signup_with_stale_policy_version_is_refused() {
        let mut set = required_set();
        set[0].version = "1999-01-01".into();
        assert_eq!(refusal(signup(&set)), Some("stale_consent_version"));
    }

    #[test]
    fn signup_with_unknown_or_repeated_kind_is_refused() {
        let mut unknown = required_set();
        unknown.push(change("ads_everywhere", true));
        assert_eq!(refusal(signup(&unknown)), Some("invalid_consent_kind"));

        let mut repeated = required_set();
        repeated.push(change("terms", false));
        assert_eq!(refusal(signup(&repeated)), Some("duplicate_consent_kind"));
    }

    #[test]
    fn optional_marketing_granted_is_accepted_with_required() {
        let mut set = required_set();
        set.push(change("marketing", true));
        assert_eq!(refusal(signup(&set)), None);
    }
}
