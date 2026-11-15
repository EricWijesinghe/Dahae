//! The approval migration must move invites that were waiting before it shipped, on a database that already has them.

use sqlx::{PgPool, migrate::Migrator};
use std::path::Path;
use uuid::Uuid;

/// Version of `0335_member_approval.sql`; every earlier migration is applied first.
const APPROVAL_VERSION: i64 = 335;

#[sqlx::test(migrations = false)]
async fn invites_waiting_before_the_approval_migration_move_to_pending_approval(db: PgPool) {
    let mut before = Migrator::new(Path::new("../../migrations")).await.expect("migrator");
    before.migrations = before.migrations.iter().filter(|m| m.version < APPROVAL_VERSION).cloned().collect::<Vec<_>>().into();
    before.run(&db).await.expect("schema before the approval migration");

    let pro: Uuid = sqlx::query_scalar("INSERT INTO pros (phone) VALUES ('01099990001') RETURNING id")
        .fetch_one(&db)
        .await
        .expect("pro row");
    let invited: Uuid = sqlx::query_scalar(
        "INSERT INTO team_members (pro_id, name, phone, role, invite_status) VALUES ($1, 'Waiting', '01099990002', 'member', 'invited') RETURNING id",
    )
    .bind(pro)
    .fetch_one(&db)
    .await
    .expect("invited member");

    sqlx::migrate!("../../migrations").run(&db).await.expect("approval migration runs on existing invites");

    let status: String = sqlx::query_scalar("SELECT invite_status FROM team_members WHERE id = $1")
        .bind(invited)
        .fetch_one(&db)
        .await
        .expect("read back");
    assert_eq!(status, "pending_approval");
}
