//! Rollback safety: the previous image must start against a database a newer release already migrated.

use sqlx::{PgPool, migrate::MigrateError};

/// A version no shipped migration uses; stands for a migration added by a newer release.
const FUTURE_VERSION: i64 = 999_999;

#[sqlx::test(migrations = "../../migrations")]
async fn migrator_tolerates_a_migration_from_a_newer_release(db: PgPool) {
    sqlx::query(
        "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
         VALUES ($1, 'from a newer release', TRUE, decode('00', 'hex'), 0)",
    )
    .bind(FUTURE_VERSION)
    .execute(&db)
    .await
    .expect("insert future migration row");

    // The default migrator refuses to run: this is what broke rollbacks.
    let strict = sqlx::migrate!("../../migrations").run(&db).await;
    assert!(matches!(strict, Err(MigrateError::VersionMissing(FUTURE_VERSION))), "{strict:?}");

    api::migrate(&db).await.expect("startup migrator ignores the unknown row");
    assert!(api::lifecycle::migrations_applied(&db).await, "readiness still passes");
    let kept: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM _sqlx_migrations WHERE version = $1)")
        .bind(FUTURE_VERSION)
        .fetch_one(&db)
        .await
        .expect("read back");
    assert!(kept, "the newer release's row is left alone");
}

/// v1.4 queues a compensating refund without the `cancellable` column that 0330 added after v1.4 was cut.
#[sqlx::test(migrations = "../../migrations")]
async fn previous_release_can_still_queue_a_refund(db: PgPool) {
    sqlx::query("INSERT INTO refund_queue (provider, tx_id, amount, next_at) VALUES ('portone', 'tx-v14', 5000, now())")
        .execute(&db)
        .await
        .expect("a v1.4 insert must not hit the NOT NULL on cancellable");
    let balance: i64 = sqlx::query_scalar("SELECT cancellable FROM refund_queue WHERE tx_id = 'tx-v14'")
        .fetch_one(&db)
        .await
        .expect("read back");
    assert_eq!(balance, 5000, "the gateway balance defaults to the refunded amount");
}

/// v1.4 still mounts the token-based staff password reset, so its table must survive the v1.5 migrations.
#[sqlx::test(migrations = "../../migrations")]
async fn staff_password_reset_table_survives_for_the_previous_release(db: PgPool) {
    let exists: Option<String> = sqlx::query_scalar("SELECT to_regclass('staff_password_resets')::text")
        .fetch_one(&db)
        .await
        .expect("regclass");
    assert!(exists.is_some(), "v1.4 INSERT/UPDATE on staff_password_resets would fail");
}
