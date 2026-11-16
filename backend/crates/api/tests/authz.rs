// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Cross-owner authorization table over the real router and a migrated database. Every owner-scoped `:id`
//! route in bookings, chat, cases and partner jobs is called by a signed-in user who does not own the record.
//! The answer must be 403 or 404 (409 for a job another pro already claimed), and the record must not change.
//! The owner's own requests are checked first, so a mistyped path cannot pass as a refusal.
//! Needs DATABASE_URL (sqlx::test).

mod common;

use axum::http::StatusCode;
use common::{Customer, book, call, call_keyed, customer, seed_catalog, verified_pro};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

const REFUSED: &[StatusCode] = &[StatusCode::FORBIDDEN, StatusCode::NOT_FOUND];
/// Accept is a first-come claim, not an owner route: a job another pro already took answers 409 `already_taken`.
const REFUSED_OR_TAKEN: &[StatusCode] = &[StatusCode::FORBIDDEN, StatusCode::NOT_FOUND, StatusCode::CONFLICT];
/// Quote routes answer 409 (`request_closed`, `quote_locked`) to a pro with no open quote on the job. The same answer
/// comes back for a missing booking, so it reveals nothing about the record.
const REFUSED_OR_CLOSED: &[StatusCode] = &[StatusCode::FORBIDDEN, StatusCode::NOT_FOUND, StatusCode::CONFLICT];

/// Who the intruder is.
#[derive(Clone, Copy)]
enum Who {
    Customer,
    Pro,
}

/// The records the probes aim at, and the intruder's payment card.
struct Fixtures {
    booking: Uuid,
    thread: Uuid,
    card: Uuid,
}

/// One request the intruder makes. `{id}` is the booking (or job) id and `{thread}` the chat thread id.
struct Probe {
    who: Who,
    method: &'static str,
    path: &'static str,
    body: fn(&Fixtures) -> Option<Value>,
    /// Owner-scoped routes that replay on an Idempotency-Key need one.
    keyed: bool,
    expect: &'static [StatusCode],
}

fn no_body(_: &Fixtures) -> Option<Value> {
    None
}

fn probe(who: Who, method: &'static str, path: &'static str, body: fn(&Fixtures) -> Option<Value>) -> Probe {
    Probe {
        who,
        method,
        path,
        body,
        keyed: false,
        expect: REFUSED,
    }
}

fn probes() -> Vec<Probe> {
    use Who::{Customer as C, Pro as P};
    let pay = |f: &Fixtures| Some(json!({ "payment_method_id": f.card }));
    let case = |_: &Fixtures| Some(json!({ "kind": "damage", "summary": "확인 요청", "photos": [] }));
    let quote = |_: &Fixtures| Some(json!({ "amount": 100_000, "note": "" }));
    vec![
        // Bookings (customer side).
        probe(C, "GET", "/v1/bookings/{id}", no_body),
        probe(C, "GET", "/v1/bookings/{id}/cancel-preview", no_body),
        Probe {
            keyed: true,
            ..probe(C, "POST", "/v1/bookings/{id}/cancel", no_body)
        },
        probe(C, "PUT", "/v1/bookings/{id}/household", |_| Some(json!({}))),
        Probe {
            keyed: true,
            ..probe(C, "POST", "/v1/bookings/{id}/deposit", pay)
        },
        Probe {
            keyed: true,
            ..probe(C, "POST", "/v1/bookings/{id}/balance", pay)
        },
        probe(C, "PUT", "/v1/bookings/{id}/notes", |_| Some(json!({ "entry": "현관 비번 없음" }))),
        probe(C, "POST", "/v1/bookings/{id}/reschedule", |_| {
            Some(json!({ "visit_date": "2031-01-06", "start_hour": 9 }))
        }),
        Probe {
            keyed: true,
            ..probe(C, "POST", "/v1/bookings/{id}/review", |_| Some(json!({ "rating": 5, "body": "좋아요" })))
        },
        probe(C, "POST", "/v1/bookings/{id}/tip", |f| {
            Some(json!({ "amount": 1000, "payment_method_id": f.card }))
        }),
        probe(C, "POST", "/v1/bookings/{id}/cases", case),
        // Chat threads (members only).
        probe(C, "GET", "/v1/chat/threads/{thread}/messages", no_body),
        probe(C, "POST", "/v1/chat/threads/{thread}/messages", |_| {
            Some(json!({ "kind": "text", "text": "끼어들기" }))
        }),
        probe(C, "POST", "/v1/chat/threads/{thread}/read", no_body),
        // Partner jobs.
        probe(P, "GET", "/v1/partner/jobs/{id}", no_body),
        Probe {
            expect: REFUSED_OR_TAKEN,
            ..probe(P, "POST", "/v1/partner/jobs/{id}/accept", no_body)
        },
        Probe {
            expect: REFUSED_OR_CLOSED,
            ..probe(P, "POST", "/v1/partner/jobs/{id}/quote", quote)
        },
        Probe {
            expect: REFUSED_OR_CLOSED,
            ..probe(P, "PUT", "/v1/partner/jobs/{id}/quote", quote)
        },
        Probe {
            expect: REFUSED_OR_CLOSED,
            ..probe(P, "DELETE", "/v1/partner/jobs/{id}/quote", no_body)
        },
        probe(P, "POST", "/v1/partner/jobs/{id}/assign", |_| Some(json!({ "member_id": Uuid::nil() }))),
        probe(P, "PUT", "/v1/partner/jobs/{id}/memo", |_| Some(json!({ "memo": "메모" }))),
        probe(P, "POST", "/v1/partner/jobs/{id}/complete", |_| {
            Some(json!({ "before_photos": [], "after_photos": [], "checklist": [] }))
        }),
        probe(P, "POST", "/v1/partner/jobs/{id}/reschedule", |_| Some(json!({ "accept": true }))),
        probe(P, "POST", "/v1/partner/jobs/{id}/cases", case),
        probe(P, "GET", "/v1/partner/jobs/{id}/entry-code", no_body),
        probe(P, "POST", "/v1/partner/jobs/{id}/check-in", no_body),
        probe(P, "POST", "/v1/partner/jobs/{id}/check-out", no_body),
    ]
}

/// Booking row as stored: any refused request that still wrote would change it.
async fn snapshot(db: &PgPool, booking: Uuid) -> String {
    sqlx::query_scalar("SELECT row_to_json(b)::text FROM bookings b WHERE id = $1")
        .bind(booking)
        .fetch_one(db)
        .await
        .expect("booking snapshot")
}

#[sqlx::test(migrations = "../../migrations")]
async fn cross_owner_requests_are_refused_and_change_nothing(db: PgPool) {
    seed_catalog(&db).await;
    let app = common::app(db.clone());

    let owner = customer(&app, &db, "010-7100-0001").await;
    let (booking, _) = book(&app, &owner, 10, None).await;
    let (owner_pro, _) = verified_pro(&app, &db, "010-7200-0001").await;
    let (st, b) = call(&app, "POST", &format!("/v1/partner/jobs/{booking}/accept"), Some(owner_pro.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "owning pro accepts the job: {b}");
    let (st, thread) = call(
        &app,
        "POST",
        "/v1/chat/threads",
        Some(owner.token.as_str()),
        Some(json!({ "topic": "시작 전 문의", "booking_id": booking })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "owner opens a thread: {thread}");
    let fx = Fixtures {
        booking,
        thread: thread["id"].as_str().expect("thread id").parse().expect("uuid"),
        card: owner.card,
    };

    // The owner reaches what the table will aim at, so a wrong path or id cannot pass as a refusal.
    let intruder: Customer = customer(&app, &db, "010-7100-0002").await;
    let (intruder_pro, _) = verified_pro(&app, &db, "010-7200-0002").await;
    let (st, _) = call(&app, "GET", &format!("/v1/bookings/{booking}"), Some(owner.token.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "owner sees own booking");
    let (st, _) = call(
        &app,
        "GET",
        &format!("/v1/chat/threads/{}/messages", fx.thread),
        Some(owner.token.as_str()),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK, "owner reads own thread");
    let (st, _) = call(&app, "GET", &format!("/v1/partner/jobs/{booking}"), Some(owner_pro.as_str()), None).await;
    assert_eq!(st, StatusCode::OK, "owning pro sees own job");

    let before = snapshot(&db, booking).await;
    // Every probe runs, so one report lists all the routes that let another owner through.
    let mut leaks = Vec::new();
    for (i, p) in probes().iter().enumerate() {
        let token = match p.who {
            Who::Customer => intruder.token.as_str(),
            Who::Pro => intruder_pro.as_str(),
        };
        let uri = p.path.replace("{id}", &fx.booking.to_string()).replace("{thread}", &fx.thread.to_string());
        let key = format!("authz-probe-{i}");
        let keyed = p.keyed.then_some(key.as_str());
        let (st, b) = call_keyed(&app, p.method, &uri, Some(token), (p.body)(&fx), keyed).await;
        if !p.expect.contains(&st) {
            leaks.push(format!("{} {} answered {st}: {b}", p.method, p.path));
        }
    }
    assert!(leaks.is_empty(), "routes that did not refuse another owner:\n{}", leaks.join("\n"));
    assert_eq!(snapshot(&db, booking).await, before, "refused requests changed nothing");
}
