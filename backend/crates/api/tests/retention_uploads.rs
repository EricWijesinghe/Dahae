//! Upload retention and upload writes: a photo that any record still names survives the orphan purge, and an
//! upload that does not commit leaves neither a row nor a file behind. Needs DATABASE_URL (sqlx::test).

mod common;

use api::{build_state, config::Config, retention, routes};
use axum::{
    Router,
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode, header},
};
use chrono::{Duration, Utc};
use common::{book, customer, seed_catalog, test_config, verified_pro};
use serde_json::Value;
use sqlx::PgPool;
use std::{net::SocketAddr, path::PathBuf};
use tower::ServiceExt;

/// Smallest container the metadata stripper accepts: signature, IHDR, IEND.
const PNG: &[u8] =
    b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00IEND\x00\x00\x00\x00";

/// An upload directory private to one test, so counting its files is not disturbed by parallel tests.
fn fresh_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("retention-uploads-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("upload dir");
    dir
}

fn config_in(dir: PathBuf) -> Config {
    let mut cfg = test_config();
    cfg.upload_dir = dir;
    cfg
}

fn app_in(pool: &PgPool, cfg: &Config) -> Router {
    routes::router(build_state(pool.clone(), cfg.clone()).expect("state"))
}

async fn post_png(app: &Router, token: &str) -> (StatusCode, Value) {
    let boundary = "XBOUNDARY";
    let mut body = format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"p.png\"\r\nContent-Type: image/png\r\n\r\n").into_bytes();
    body.extend_from_slice(PNG);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let mut req = Request::builder()
        .method("POST")
        .uri("/v1/uploads")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
        .body(Body::from(body))
        .expect("request");
    req.extensions_mut().insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40_000))));
    let res = app.clone().oneshot(req).await.expect("router is infallible");
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.expect("body");
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

#[sqlx::test(migrations = "../../migrations")]
async fn photos_named_by_any_record_survive_the_orphan_purge(pool: PgPool) {
    seed_catalog(&pool).await;
    let dir = fresh_dir("references");
    let cfg = config_in(dir.clone());
    let app = app_in(&pool, &cfg);
    let c = customer(&app, &pool, "010-5000-0701").await;
    let (booking, _) = book(&app, &c, 10, None).await;
    let (_, pro) = verified_pro(&app, &pool, "010-5000-0702").await;

    // 0-1 completion photos, 2 review photo, 3 business logo, 4 pro photo, 5 content image, 6 unreferenced.
    let names: Vec<String> = (1..=7).map(|i| format!("{i:032x}.jpg")).collect();
    let old = Utc::now() - Duration::days(60);
    for name in &names {
        std::fs::write(dir.join(name), b"photo").expect("file");
        sqlx::query("INSERT INTO uploads (owner_role, owner_id, file, bytes, created_at) VALUES ('customer', $1, $2, 5, $3)")
            .bind(c.user)
            .bind(name)
            .bind(old)
            .execute(&pool)
            .await
            .expect("upload row");
    }
    let path = |i: usize| format!("/files/{}", names[i]);
    sqlx::query("INSERT INTO completion_reports (booking_id, before_photos, after_photos, checklist, submitted_by) VALUES ($1, $2, $3, '{}'::jsonb, $4)")
        .bind(booking)
        .bind(vec![path(0)])
        .bind(vec![path(1)])
        .bind(c.user)
        .execute(&pool)
        .await
        .expect("completion report");
    sqlx::query("INSERT INTO reviews (booking_id, user_id, rating, body, photos) VALUES ($1, $2, 5, 'ok', $3::jsonb)")
        .bind(booking)
        .bind(c.user)
        .bind(format!("[\"{}\"]", path(2)))
        .execute(&pool)
        .await
        .expect("review");
    sqlx::query("INSERT INTO settings (key, value) VALUES ('business', $1::jsonb) ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value")
        .bind(format!("{{\"logo_url\": \"{}\"}}", path(3)))
        .execute(&pool)
        .await
        .expect("logo");
    sqlx::query("INSERT INTO pro_profiles (pro_id, photo_url) VALUES ($1, $2) ON CONFLICT (pro_id) DO UPDATE SET photo_url = EXCLUDED.photo_url")
        .bind(pro)
        .bind(path(4))
        .execute(&pool)
        .await
        .expect("pro photo");
    sqlx::query("INSERT INTO content_items (kind, title, image_url) VALUES ('banner', '{}'::jsonb, $1)")
        .bind(path(5))
        .execute(&pool)
        .await
        .expect("content image");

    let report = retention::purge(&pool, &cfg).await.expect("purge");
    assert_eq!(report.uploads, 1, "only the unreferenced photo is purged");
    let mut left: Vec<String> = sqlx::query_scalar("SELECT file FROM uploads").fetch_all(&pool).await.expect("left");
    left.sort();
    assert_eq!(left, names[..6].to_vec());
    assert!(!dir.join(&names[6]).exists(), "the unreferenced file is removed from disk");
    assert!(names[..6].iter().all(|n| dir.join(n).exists()), "every referenced file stays on disk");
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_commit_that_fails_after_the_write_leaves_no_file(pool: PgPool) {
    seed_catalog(&pool).await;
    let dir = fresh_dir("commit");
    let cfg = config_in(dir.clone());
    let app = app_in(&pool, &cfg);
    let c = customer(&app, &pool, "010-5000-0703").await;
    sqlx::query("CREATE FUNCTION fail_upload_commit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected commit failure'; END $$")
        .execute(&pool)
        .await
        .expect("fn");
    sqlx::query(
        "CREATE CONSTRAINT TRIGGER fail_upload AFTER INSERT ON uploads DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION fail_upload_commit()",
    )
    .execute(&pool)
    .await
    .expect("trigger");

    let (status, _) = post_png(&app, &c.token).await;
    assert!(!status.is_success(), "the upload must fail when its commit fails");
    let files = std::fs::read_dir(&dir).expect("dir").count();
    assert_eq!(files, 0, "a file with no committed row is never purged, so it must not be written");
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM uploads").fetch_one(&pool).await.expect("rows");
    assert_eq!(rows, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_failed_file_write_leaves_no_row(pool: PgPool) {
    seed_catalog(&pool).await;
    let dir = std::env::temp_dir().join("retention-uploads-missing-dir");
    let _ = std::fs::remove_dir_all(&dir);
    let cfg = config_in(dir);
    let app = app_in(&pool, &cfg);
    let c = customer(&app, &pool, "010-5000-0704").await;

    let (status, _) = post_png(&app, &c.token).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "a write into a missing directory fails");
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM uploads").fetch_one(&pool).await.expect("rows");
    assert_eq!(rows, 0, "a failed write leaves no upload row");
}
