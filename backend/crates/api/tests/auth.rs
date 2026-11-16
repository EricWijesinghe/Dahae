//! Customer signup consents over the real router and a migrated database: a new account needs every
//! required consent, and the grants are recorded with the policy version and time. Needs DATABASE_URL (sqlx::test).

mod common;

use api::privacy::CONSENT_VERSION;
use axum::http::StatusCode;
use common::call;
use serde_json::{Value, json};
use sqlx::PgPool;

async fn request_code(app: &axum::Router, phone: &str) -> String {
    let (st, sent) = call(app, "POST", "/v1/auth/otp", None, Some(json!({ "phone": phone }))).await;
    assert_eq!(st, StatusCode::OK, "{sent}");
    sent["dev_code"].as_str().expect("dev code").to_string()
}

fn grant(kind: &str, granted: bool) -> Value {
    json!({ "kind": kind, "granted": granted, "version": CONSENT_VERSION })
}

async fn verify(app: &axum::Router, phone: &str, code: &str, consents: Option<Value>) -> (StatusCode, Value) {
    let mut body = json!({ "phone": phone, "code": code, "audience": "customer" });
    if let Some(c) = consents {
        body["consents"] = c;
    }
    call(app, "POST", "/v1/auth/verify", None, Some(body)).await
}

#[sqlx::test(migrations = "../../migrations")]
async fn signup_without_required_consents_is_refused_and_creates_no_account(db: PgPool) {
    let app = common::app(db.clone());
    let phone = "010-7300-0001";
    let code = request_code(&app, phone).await;

    let (st, b) = verify(&app, phone, &code, None).await;
    assert_eq!((st, b["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("required_consent")), "{b}");
    let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(&db).await.expect("count");
    assert_eq!(users, 0, "a refused signup leaves no account");
}

#[sqlx::test(migrations = "../../migrations")]
async fn signup_missing_one_required_consent_is_refused(db: PgPool) {
    let app = common::app(db);
    let phone = "010-7300-0006";
    let code = request_code(&app, phone).await;
    let partial = json!([grant("terms", true), grant("privacy", true)]);
    let (st, b) = verify(&app, phone, &code, Some(partial)).await;
    assert_eq!((st, b["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("required_consent")), "{b}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn declining_a_required_consent_refuses_signup(db: PgPool) {
    let app = common::app(db);
    let phone = "010-7300-0002";
    let code = request_code(&app, phone).await;
    let declined = json!([grant("terms", true), grant("privacy", true), grant("overseas_transfer", false)]);
    let (st, b) = verify(&app, phone, &code, Some(declined)).await;
    assert_eq!((st, b["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("required_consent")), "{b}");
}

#[sqlx::test(migrations = "../../migrations")]
async fn stale_policy_version_at_signup_is_refused(db: PgPool) {
    let app = common::app(db);
    let phone = "010-7300-0003";
    let code = request_code(&app, phone).await;
    let stale = json!([
        { "kind": "terms", "granted": true, "version": "1999-01-01" },
        grant("privacy", true),
        grant("overseas_transfer", true),
    ]);
    let (st, b) = verify(&app, phone, &code, Some(stale)).await;
    assert_eq!(
        (st, b["error"]["code"].as_str()),
        (StatusCode::BAD_REQUEST, Some("stale_consent_version")),
        "{b}"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn signup_with_required_consents_records_each_grant_with_version_and_time(db: PgPool) {
    let app = common::app(db.clone());
    let phone = "010-7300-0004";
    let code = request_code(&app, phone).await;
    let consents = json!([
        grant("terms", true),
        grant("privacy", true),
        grant("overseas_transfer", true),
        grant("marketing", false),
    ]);
    let (st, b) = verify(&app, phone, &code, Some(consents)).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(b["is_new"], json!(true));
    let user: uuid::Uuid = b["subject"].as_str().expect("subject").parse().expect("uuid");

    let rows: Vec<(String, String, bool, bool)> =
        sqlx::query_as("SELECT kind, version, granted, created_at IS NOT NULL FROM user_consents WHERE user_id = $1 ORDER BY id")
            .bind(user)
            .fetch_all(&db)
            .await
            .expect("consent rows");
    let kinds: Vec<(&str, bool)> = rows.iter().map(|r| (r.0.as_str(), r.2)).collect();
    assert_eq!(
        kinds,
        vec![("terms", true), ("privacy", true), ("overseas_transfer", true), ("marketing", false)],
        "every answer is kept as given"
    );
    assert!(
        rows.iter().all(|r| r.1 == CONSENT_VERSION && r.3),
        "each row names the policy version and has a timestamp"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn existing_customer_signs_in_without_consents(db: PgPool) {
    let app = common::app(db);
    let phone = "010-7300-0005";
    let code = request_code(&app, phone).await;
    let (st, b) = verify(
        &app,
        phone,
        &code,
        Some(json!([grant("terms", true), grant("privacy", true), grant("overseas_transfer", true)])),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "{b}");

    let code = request_code(&app, phone).await;
    let (st, b) = verify(&app, phone, &code, None).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(b["is_new"], json!(false), "a returning customer is not asked again at sign-in");
}

async fn social_signup(app: &axum::Router, sub: &str, consents: Option<Value>) -> (StatusCode, Value) {
    let mut body = json!({ "provider": "apple", "token": format!("dev:{sub}"), "audience": "customer" });
    if let Some(c) = consents {
        body["consents"] = c;
    }
    call(app, "POST", "/v1/auth/social", None, Some(body)).await
}

#[sqlx::test(migrations = "../../migrations")]
async fn social_signup_without_required_consents_creates_no_account_or_identity(db: PgPool) {
    let app = common::app(db.clone());
    let (st, b) = social_signup(&app, "social-no-consent", None).await;
    assert_eq!((st, b["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("required_consent")), "{b}");
    let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(&db).await.expect("users");
    let identities: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM user_identities")
        .fetch_one(&db)
        .await
        .expect("identities");
    assert_eq!((users, identities), (0, 0), "a refused social signup leaves no account behind");
}

#[sqlx::test(migrations = "../../migrations")]
async fn social_signup_with_a_declined_required_consent_is_refused(db: PgPool) {
    let app = common::app(db.clone());
    let declined = json!([grant("terms", true), grant("privacy", true), grant("overseas_transfer", false)]);
    let (st, b) = social_signup(&app, "social-declined", Some(declined)).await;
    assert_eq!((st, b["error"]["code"].as_str()), (StatusCode::BAD_REQUEST, Some("required_consent")), "{b}");
    let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(&db).await.expect("users");
    assert_eq!(users, 0, "a declined required consent creates no account");
}

#[sqlx::test(migrations = "../../migrations")]
async fn social_signup_with_required_consents_records_them_and_returning_sign_in_skips_them(db: PgPool) {
    let app = common::app(db.clone());
    let consents = json!([
        grant("terms", true),
        grant("privacy", true),
        grant("overseas_transfer", true),
        grant("marketing", false),
    ]);
    let (st, b) = social_signup(&app, "social-granted", Some(consents)).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(b["is_new"], json!(true));
    let user: uuid::Uuid = b["subject"].as_str().expect("subject").parse().expect("uuid");

    let rows: Vec<(String, String, bool)> = sqlx::query_as("SELECT kind, version, granted FROM user_consents WHERE user_id = $1 ORDER BY id")
        .bind(user)
        .fetch_all(&db)
        .await
        .expect("consent rows");
    assert_eq!(rows.len(), 4, "every answer is kept as given");
    assert!(rows.iter().all(|r| r.1 == CONSENT_VERSION));
    assert_eq!(rows[3], ("marketing".to_string(), CONSENT_VERSION.to_string(), false));

    let (st, b) = social_signup(&app, "social-granted", None).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    assert_eq!(b["is_new"], json!(false), "a returning social customer is not asked again");
}
