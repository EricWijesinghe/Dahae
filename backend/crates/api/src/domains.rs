// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Service domains after a console domain change (settings row `domains`). At start the API records the host of
//! `PUBLIC_BASE_URL` as `current`; a different stored `current` moves into `aliases`. A former domain keeps serving
//! everything (installed apps post, open sockets and check payment pages on it) until the owner retires it.
//! Caddy asks `/internal/tls-ask` before it gets a certificate for a host that is not the main domain.

use crate::{
    app_links::NAVER_CALLBACK_PATH,
    auth::Admin,
    config::Config,
    error::{ApiError, ApiResult},
    host_jobs::valid_domain,
    owner,
    routes::audit,
    state::AppState,
};
use axum::{
    Json, Router,
    extract::{ConnectInfo, Path, Query, State},
    http::{HeaderMap, StatusCode},
    routing::{delete, get, put},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::{
    collections::BTreeMap,
    net::SocketAddr,
    sync::Mutex,
    time::{Duration, Instant},
};

const SETTINGS_KEY: &str = "domains";
const ASK_TTL: Duration = Duration::from_secs(60);
/// Alias hosts per database (the test binaries share one process), read at most once a minute per node.
static ASK_CACHE: Mutex<Option<(String, Instant, Vec<String>)>> = Mutex::new(None);

/// The stored row. The same shape is what other packages read (`store_build` snapshot).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Domains {
    #[serde(default)]
    pub current: Option<String>,
    #[serde(default)]
    pub aliases: Vec<Alias>,
    #[serde(default)]
    pub retired: Vec<Retired>,
    #[serde(default)]
    pub checklist: BTreeMap<String, bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Alias {
    pub host: String,
    pub since: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Retired {
    pub host: String,
    pub at: DateTime<Utc>,
}

#[derive(Deserialize)]
struct Ask {
    domain: String,
}

#[derive(Deserialize)]
struct Tick {
    done: bool,
}

/// Top level, never proxied by Caddy (`@internal`): only the web container calls it.
/// Caddy's certificate question. Not rate limited: Caddy's ask calls share one address, and a flood of random SNI
/// names must not starve real aliases of certificates. The answer is cached and only Caddy can reach it.
pub const TLS_ASK_PATH: &str = "/internal/tls-ask";

pub fn routes() -> Router<AppState> {
    // The literal (not TLS_ASK_PATH) lets tests/api_contract.rs find the route.
    Router::new().route("/internal/tls-ask", get(tls_ask))
}

/// Owner console (mounted under `/admin`).
pub fn admin_routes() -> Router<AppState> {
    Router::new()
        .route("/domains", get(list))
        .route("/domains/{host}", delete(retire))
        .route("/domains/checklist/{id}", put(tick))
}

/// Host of the public base URL when it is a real domain (dev bases such as 127.0.0.1 are skipped).
pub fn base_host(base_url: &str) -> Option<String> {
    let url = reqwest::Url::parse(base_url).ok()?;
    url.host_str().map(str::to_ascii_lowercase).filter(|h| valid_domain(h))
}

/// The row after the service moved to `host`: the old current becomes an alias (once), the new host stops being
/// an alias or retired, and the checklist starts over. Unchanged when `host` is already current.
pub fn with_current(mut d: Domains, host: &str, now: DateTime<Utc>) -> Domains {
    match d.current.as_deref() {
        Some(c) if c == host => return d,
        Some(c) => {
            let old = c.to_string();
            if !d.aliases.iter().any(|a| a.host == old) {
                d.aliases.push(Alias { host: old, since: now });
            }
            d.checklist.clear();
        }
        None => {}
    }
    d.aliases.retain(|a| a.host != host);
    d.retired.retain(|r| r.host != host);
    d.current = Some(host.to_string());
    d
}

/// As [with_current] after `dahae domain` moved back from `failed`: the failed host never becomes a former domain
/// (no certificate, no app link host, no CORS, no "domain changed" on the store build card).
pub fn after_rollback(d: Domains, host: &str, failed: &str, now: DateTime<Utc>) -> Domains {
    let was_failed = d.current.as_deref() == Some(failed);
    let mut d = with_current(d, host, now);
    if was_failed {
        d.aliases.retain(|a| a.host != failed);
    }
    d
}

/// Moves `host` from the aliases to the retired list. `None` when it is not an alias.
pub fn without_alias(mut d: Domains, host: &str, now: DateTime<Utc>) -> Option<Domains> {
    let i = d.aliases.iter().position(|a| a.host == host)?;
    d.aliases.remove(i);
    d.retired.retain(|r| r.host != host);
    d.retired.push(Retired {
        host: host.to_string(),
        at: now,
    });
    Some(d)
}

/// `dahae domain` writes the host it is about to move to here (root-owned, mounted read-only into the API) and checks
/// it over HTTPS before it switches anything, so a move that would fail never takes the service off its address.
const PENDING_FILE: &str = "out/domain-pending";
/// Longer than the CLI's whole HTTPS wait (about 10 minutes); a file left behind by a stopped move ends with it.
const PENDING_TTL: Duration = Duration::from_secs(15 * 60);

/// The pending host (or its `www.` form) may get a certificate while the file is fresh.
fn pending_allows(content: &str, age: Duration, domain: &str) -> bool {
    let host = content.trim();
    age < PENDING_TTL && valid_domain(host) && ask_allows(&[host.to_string()], domain)
}

fn pending_allowed(jobs_dir: &std::path::Path, domain: &str) -> bool {
    let path = jobs_dir.join(PENDING_FILE);
    let age = std::fs::metadata(&path).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok());
    age.is_some_and(|a| pending_allows(&std::fs::read_to_string(&path).unwrap_or_default(), a, domain))
}

/// Caddy may get a certificate for an alias or its `www.` form. The current, retired and unknown hosts are refused.
pub fn ask_allows(aliases: &[String], domain: &str) -> bool {
    let d = domain.trim_end_matches('.').to_ascii_lowercase();
    let bare = d.strip_prefix("www.").unwrap_or(&d);
    aliases.iter().any(|a| *a == d || a == bare)
}

/// `https://<alias>` for each alias, so old-origin web apps can still load from the canonical origin.
pub fn alias_origins(d: &Domains) -> Vec<String> {
    d.aliases.iter().map(|a| format!("https://{}", a.host)).collect()
}

/// Called once before the router is built: records the current domain and adds the aliases to CORS.
/// A database error is logged and the API starts without the aliases' CORS entries.
pub async fn startup(db: &PgPool, config: &mut Config) {
    let Some(host) = base_host(&config.public_base_url) else {
        return;
    };
    let failed = rolled_back_from(&config.host_jobs_dir);
    match update(db, |d| {
        Ok(match &failed {
            Some(f) => after_rollback(d, &host, f, Utc::now()),
            None => with_current(d, &host, Utc::now()),
        })
    })
    .await
    {
        Ok(d) => {
            let _ = crate::util::FORMER_BASES.set(alias_origins(&d));
            for o in alias_origins(&d) {
                if !config.allowed_origins.contains(&o) {
                    config.allowed_origins.push(o);
                }
            }
        }
        Err(e) => tracing::warn!(error = %e, "service domains not recorded; former domains get no CORS"),
    }
}

/// `dahae domain` writes the host it tried here before it moves back (root-owned, read-only in the API).
const ROLLED_BACK_FILE: &str = "out/domain-rolled-back";

fn rolled_back_from(jobs_dir: &std::path::Path) -> Option<String> {
    let path = jobs_dir.join(ROLLED_BACK_FILE);
    let age = std::fs::metadata(&path).and_then(|m| m.modified()).ok()?.elapsed().ok()?;
    let host = std::fs::read_to_string(&path).ok()?.trim().to_string();
    (age < PENDING_TTL && valid_domain(&host)).then_some(host)
}

pub async fn load(db: &PgPool) -> sqlx::Result<Domains> {
    let v: Option<Value> = sqlx::query_scalar("SELECT value FROM settings WHERE key = $1")
        .bind(SETTINGS_KEY)
        .fetch_optional(db)
        .await?;
    Ok(v.map(parse).unwrap_or_default())
}

fn parse(v: Value) -> Domains {
    serde_json::from_value(v).unwrap_or_else(|e| {
        tracing::warn!(error = %e, "domains row unreadable; treated as empty");
        Domains::default()
    })
}

/// Read, change and write the row under a row lock, so two nodes starting at once record one move.
async fn update(db: &PgPool, f: impl FnOnce(Domains) -> ApiResult<Domains>) -> ApiResult<Domains> {
    let mut tx = db.begin().await?;
    sqlx::query("INSERT INTO settings (key, value) VALUES ($1, '{}'::jsonb) ON CONFLICT (key) DO NOTHING")
        .bind(SETTINGS_KEY)
        .execute(&mut *tx)
        .await?;
    let v: Value = sqlx::query_scalar("SELECT value FROM settings WHERE key = $1 FOR UPDATE")
        .bind(SETTINGS_KEY)
        .fetch_one(&mut *tx)
        .await?;
    let before = parse(v);
    let after = f(before.clone())?;
    if after != before {
        sqlx::query("UPDATE settings SET value = $2, updated_at = now() WHERE key = $1")
            .bind(SETTINGS_KEY)
            .bind(sqlx::types::Json(&after))
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    invalidate();
    Ok(after)
}

fn invalidate() {
    if let Ok(mut c) = ASK_CACHE.lock() {
        *c = None;
    }
}

async fn alias_hosts(db: &PgPool) -> sqlx::Result<Vec<String>> {
    let key = db.connect_options().get_database().unwrap_or_default().to_string();
    if let Ok(c) = ASK_CACHE.lock()
        && let Some((k, t, hosts)) = c.as_ref()
        && *k == key
        && t.elapsed() < ASK_TTL
    {
        return Ok(hosts.clone());
    }
    let hosts: Vec<String> = load(db).await?.aliases.into_iter().map(|a| a.host).collect();
    if let Ok(mut c) = ASK_CACHE.lock() {
        *c = Some((key, Instant::now(), hosts.clone()));
    }
    Ok(hosts)
}

async fn tls_ask(State(s): State<AppState>, Query(q): Query<Ask>) -> StatusCode {
    if pending_allowed(&s.config.host_jobs_dir, &q.domain) {
        return StatusCode::OK;
    }
    match alias_hosts(&s.db).await {
        Ok(hosts) if ask_allows(&hosts, &q.domain) => StatusCode::OK,
        Ok(_) => StatusCode::NOT_FOUND,
        Err(e) => {
            tracing::warn!(error = %e, "tls-ask could not read the aliases");
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}

/// Ids of the outside places to update after a move (labels live in the console, by id), with the value to paste.
pub fn checklist_items(cur: &str, aliases: &[Alias]) -> Vec<(&'static str, String)> {
    let base = format!("https://{cur}");
    let old: Vec<&str> = aliases.iter().map(|a| a.host.as_str()).collect();
    // native-keys.sh takes at most three hosts: the current one and the two newest former ones.
    let link_hosts = std::iter::once(cur).chain(old.iter().rev().take(2).copied()).collect::<Vec<_>>().join(",");
    // shortcut: the API does not know the server IP, so this lists the hosts whose records to keep or add; the
    // console shows the IP from the domain-check result.
    let keep = old.iter().map(|h| h.to_string()).chain([format!("www.{cur}")]).collect::<Vec<_>>().join(", ");
    let store = format!("{base}/privacy.html\n{base}/delete-account.html");
    vec![
        ("portone_webhook", format!("{base}/v1/webhooks/portone")),
        ("toss_payments_webhook", format!("{base}/v1/webhooks/tosspayments")),
        ("toss_payouts_webhook", format!("{base}/v1/webhooks/toss")),
        ("pg_site_url", base.clone()),
        ("naver_callback", format!("{base}{NAVER_CALLBACK_PATH}")),
        ("naver_service_url", base.clone()),
        ("kakao_site_domain", base.clone()),
        ("google_consent", format!("{base}/privacy.html")),
        ("search_console", base.clone()),
        ("naver_search_advisor", base.clone()),
        // Sign in with Apple sends account deletions here (App Store Review 5.1.1(v)); DEPLOY.md has owners set it.
        ("apple_notifications", format!("{base}/v1/auth/apple/notifications")),
        ("app_store_urls", format!("{base}/\n{store}")),
        ("play_urls", store.clone()),
        ("ms_store_urls", store),
        ("github_store_vars", format!("API_BASE={base}\nAPP_LINK_DOMAIN={link_hosts}")),
        ("alimtalk_links", base),
        ("dns_keep_old", keep),
        ("version_floor", String::new()),
    ]
}

fn document(d: &Domains, fallback: Option<String>) -> Value {
    let cur = d.current.clone().or(fallback).unwrap_or_default();
    let checklist: Vec<Value> = checklist_items(&cur, &d.aliases)
        .into_iter()
        .map(|(id, value)| json!({ "id": id, "value": value, "done": d.checklist.get(id).copied().unwrap_or(false) }))
        .collect();
    json!({ "current": cur, "aliases": d.aliases, "checklist": checklist })
}

async fn list(State(s): State<AppState>, Admin(_): Admin) -> ApiResult<Json<Value>> {
    let d = load(&s.db).await?;
    Ok(Json(document(&d, base_host(&s.config.public_base_url))))
}

/// Owner code. The alias stops getting certificates; a cached one may answer until it expires, which is harmless.
async fn retire(
    State(s): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Admin(a): Admin,
    Path(host): Path<String>,
) -> ApiResult<Json<Value>> {
    let host = host.to_ascii_lowercase();
    let step = owner::check_owner(&s, a, &headers, Some(peer.ip()), true).await?;
    if !load(&s.db).await?.aliases.iter().any(|x| x.host == host) {
        return Err(ApiError::NotFound);
    }
    owner::burn_totp(&s, a.id, owner::client_ip(&s, Some(peer.ip()), &headers), step).await?;
    let d = update(&s.db, |d| without_alias(d, &host, Utc::now()).ok_or(ApiError::NotFound)).await?;
    audit(&s.db, "staff", Some(a.id), "domains.retire", &host, json!({})).await?;
    Ok(Json(document(&d, base_host(&s.config.public_base_url))))
}

async fn tick(State(s): State<AppState>, Admin(a): Admin, Path(id): Path<String>, Json(body): Json<Tick>) -> ApiResult<Json<Value>> {
    let known = checklist_items("x.invalid", &[]).iter().any(|(k, _)| *k == id);
    if !known {
        return Err(ApiError::NotFound);
    }
    let d = update(&s.db, |mut d| {
        d.checklist.insert(id.clone(), body.done);
        Ok(d)
    })
    .await?;
    audit(&s.db, "staff", Some(a.id), "domains.checklist", &id, json!({ "done": body.done })).await?;
    Ok(Json(document(&d, base_host(&s.config.public_base_url))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(s, 0).expect("time")
    }

    #[test]
    fn first_record_sets_current_and_a_move_keeps_the_old_one_once() {
        let d = with_current(Domains::default(), "a.co.kr", t(1));
        assert_eq!(d.current.as_deref(), Some("a.co.kr"));
        assert!(d.aliases.is_empty());
        let same = with_current(d.clone(), "a.co.kr", t(2));
        assert_eq!(same, d, "same host changes nothing");
        let mut ticked = d;
        ticked.checklist.insert("pg_site_url".into(), true);
        let moved = with_current(ticked, "b.co.kr", t(3));
        assert_eq!(moved.current.as_deref(), Some("b.co.kr"));
        assert_eq!(
            moved.aliases,
            vec![Alias {
                host: "a.co.kr".into(),
                since: t(3)
            }]
        );
        assert!(moved.checklist.is_empty(), "a new move starts a new checklist");
        let again = with_current(moved.clone(), "b.co.kr", t(4));
        assert_eq!(again.aliases.len(), 1);
        let back = with_current(moved, "a.co.kr", t(5));
        assert_eq!(
            back.aliases.iter().map(|a| a.host.as_str()).collect::<Vec<_>>(),
            vec!["b.co.kr"],
            "moving back drops it as an alias"
        );
    }

    #[test]
    fn a_move_back_never_lists_the_failed_host() {
        let d = with_current(with_current(Domains::default(), "a.co.kr", t(1)), "b.co.kr", t(2));
        let back = after_rollback(d.clone(), "a.co.kr", "b.co.kr", t(3));
        assert_eq!(back.current.as_deref(), Some("a.co.kr"));
        assert!(back.aliases.is_empty(), "{:?}", back.aliases);
        let later = after_rollback(back.clone(), "a.co.kr", "b.co.kr", t(4));
        assert_eq!(later, back, "a restart after the move back changes nothing");
        let other = after_rollback(d, "c.co.kr", "x.co.kr", t(5));
        assert_eq!(
            other,
            with_current(
                with_current(with_current(Domains::default(), "a.co.kr", t(1)), "b.co.kr", t(2)),
                "c.co.kr",
                t(5)
            )
        );
    }

    #[test]
    fn retire_moves_an_alias_and_refuses_anything_else() {
        let d = with_current(with_current(Domains::default(), "a.co.kr", t(1)), "b.co.kr", t(2));
        assert!(without_alias(d.clone(), "b.co.kr", t(3)).is_none(), "current is not an alias");
        assert!(without_alias(d.clone(), "c.co.kr", t(3)).is_none());
        let r = without_alias(d, "a.co.kr", t(3)).expect("retired");
        assert!(r.aliases.is_empty());
        assert_eq!(
            r.retired,
            vec![Retired {
                host: "a.co.kr".into(),
                at: t(3)
            }]
        );
    }

    #[test]
    fn ask_allows_aliases_and_their_www_form_only() {
        let aliases = vec!["old.co.kr".to_string()];
        assert!(ask_allows(&aliases, "old.co.kr"));
        assert!(ask_allows(&aliases, "www.old.co.kr"));
        assert!(ask_allows(&aliases, "OLD.co.kr."));
        assert!(!ask_allows(&aliases, "new.co.kr"));
        assert!(!ask_allows(&aliases, "x.old.co.kr"));
        assert!(!ask_allows(&aliases, "www.www.old.co.kr"));
        assert!(!ask_allows(&[], "old.co.kr"));
    }

    #[test]
    fn a_pending_move_gets_a_certificate_for_fifteen_minutes_only() {
        let min = |m: u64| Duration::from_secs(m * 60);
        assert!(pending_allows("new.co.kr\n", min(1), "new.co.kr"));
        assert!(pending_allows("new.co.kr\n", min(1), "www.new.co.kr"));
        assert!(!pending_allows("new.co.kr\n", min(15), "new.co.kr"), "stale file");
        assert!(!pending_allows("new.co.kr\n", min(1), "other.co.kr"));
        assert!(!pending_allows("", min(1), ""), "an empty file allows nothing");
    }

    #[test]
    fn base_host_skips_dev_addresses() {
        assert_eq!(base_host("https://Shop.co.kr").as_deref(), Some("shop.co.kr"));
        assert_eq!(base_host("https://shop.co.kr:8443/x").as_deref(), Some("shop.co.kr"));
        assert_eq!(base_host("http://127.0.0.1:8080"), None);
        assert_eq!(base_host("http://localhost:8080"), None);
        assert_eq!(base_host("not a url"), None);
    }

    #[test]
    fn checklist_values_follow_the_new_domain_and_list_old_hosts_for_app_links() {
        let aliases = vec![Alias {
            host: "old.co.kr".into(),
            since: t(1),
        }];
        let items: BTreeMap<_, _> = checklist_items("new.co.kr", &aliases).into_iter().collect();
        assert_eq!(items["portone_webhook"], "https://new.co.kr/v1/webhooks/portone");
        assert_eq!(items["toss_payments_webhook"], "https://new.co.kr/v1/webhooks/tosspayments");
        assert_eq!(items["toss_payouts_webhook"], "https://new.co.kr/v1/webhooks/toss");
        assert_eq!(items["naver_callback"], "https://new.co.kr/auth/naver/callback");
        assert_eq!(items["github_store_vars"], "API_BASE=https://new.co.kr\nAPP_LINK_DOMAIN=new.co.kr,old.co.kr");
        assert_eq!(items["dns_keep_old"], "old.co.kr, www.new.co.kr");
        assert_eq!(items["apple_notifications"], "https://new.co.kr/v1/auth/apple/notifications");
        assert!(items["app_store_urls"].starts_with("https://new.co.kr/\n"), "support URL first");
        assert_eq!(items.len(), 18);
    }

    #[test]
    fn app_link_hosts_stay_within_the_three_a_store_build_takes() {
        let aliases: Vec<Alias> = ["a.kr", "b.kr", "c.kr"]
            .iter()
            .map(|h| Alias {
                host: (*h).into(),
                since: t(1),
            })
            .collect();
        let items: BTreeMap<_, _> = checklist_items("new.kr", &aliases).into_iter().collect();
        // scripts/ci/native-keys.sh refuses more than three hosts; the newest former hosts stay.
        assert_eq!(items["github_store_vars"], "API_BASE=https://new.kr\nAPP_LINK_DOMAIN=new.kr,c.kr,b.kr");
    }
}
