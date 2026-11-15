//! Migration 0233 over a seeded database: legacy mattress rows gain `unit_counts` only while the
//! mattress service is counts pricing. Owner-edited unit pricing keeps its legacy `unit_index`.

mod common;

use common::seed_catalog;
use sqlx::PgPool;
use uuid::Uuid;

const MIGRATION: &str = include_str!("../../../migrations/0233_mattress_unit_counts.sql");

async fn legacy_mattress_booking(db: &PgPool, phone: &str, unit_index: i16) -> Uuid {
    let user: Uuid = sqlx::query_scalar("INSERT INTO users (phone, name) VALUES ($1, 'legacy') RETURNING id")
        .bind(phone)
        .fetch_one(db)
        .await
        .expect("user");
    sqlx::query_scalar(
        "INSERT INTO bookings (number, user_id, service_code, unit_index, address, district_code, visit_date, start_hour, status, list_price, discount, final_price)
         VALUES ($2, $1, 'mattress', $3, '{}'::jsonb, '11680', '2026-03-05', 10, 3, 118000, 0, 118000) RETURNING id",
    )
    .bind(user)
    .bind(format!("BKM-{phone}"))
    .bind(unit_index)
    .fetch_one(db)
    .await
    .expect("booking")
}

async fn booking_units(db: &PgPool, id: Uuid) -> (Option<i16>, Option<Vec<i16>>) {
    sqlx::query_as("SELECT unit_index, unit_counts FROM bookings WHERE id = $1")
        .bind(id)
        .fetch_one(db)
        .await
        .expect("read booking")
}

#[sqlx::test(migrations = "../../migrations")]
async fn legacy_mattress_row_gains_counts_and_keeps_its_unit_index(pool: PgPool) {
    seed_catalog(&pool).await;
    let id = legacy_mattress_booking(&pool, "01053000001", 1).await;
    sqlx::raw_sql(MIGRATION).execute(&pool).await.expect("migration");
    // unit_index 1 meant two mattresses; the count is kept and the original index is untouched.
    assert_eq!(booking_units(&pool, id).await, (Some(1), Some(vec![2])));
}

#[sqlx::test(migrations = "../../migrations")]
async fn owner_edited_unit_mattress_is_not_migrated(pool: PgPool) {
    seed_catalog(&pool).await;
    sqlx::query("UPDATE services SET pricing = '{\"type\":\"unit\",\"units\":[{\"label_key\":\"count_1\",\"minutes\":60,\"price\":60000}]}'::jsonb WHERE code = 'mattress'")
        .execute(&pool)
        .await
        .expect("owner edit");
    let id = legacy_mattress_booking(&pool, "01053000002", 0).await;
    sqlx::raw_sql(MIGRATION).execute(&pool).await.expect("migration");
    assert_eq!(booking_units(&pool, id).await, (Some(0), None));
}
