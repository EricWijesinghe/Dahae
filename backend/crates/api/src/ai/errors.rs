//! Failed model calls (transport, non-2xx, unreadable body). They spend no reply cap, so a provider
//! outage gets its own ceiling: otherwise every customer message would still reach the provider.

use crate::ratelimit::within_quota;
use sqlx::PgPool;
use uuid::Uuid;

/// Failed calls one thread may make in an hour before its AI stays quiet until the window clears.
pub const ERROR_THREAD_HOURLY_CAP: i64 = 6;

/// True while the thread and the shop are under their failed-call ceilings. Pure so it can be tested
/// without a database.
pub fn under_error_caps(thread_errors: i64, day_errors: i64, daily: u32) -> bool {
    within_quota(thread_errors, ERROR_THREAD_HOURLY_CAP) && within_quota(day_errors, i64::from(daily))
}

/// Failed calls for this thread in the last hour and for the whole shop in the last day, checked
/// against the thread ceiling and the shop's daily cap.
pub async fn within_error_caps(db: &PgPool, thread: Uuid, daily: u32) -> Result<bool, sqlx::Error> {
    let (thread_errors, day_errors): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM ai_usage WHERE thread_id = $1 AND outcome = 'error' AND created_at > now() - interval '1 hour'),
                (SELECT count(*) FROM ai_usage WHERE outcome = 'error' AND created_at > now() - interval '1 day')",
    )
    .bind(thread)
    .fetch_one(db)
    .await?;
    Ok(under_error_caps(thread_errors, day_errors, daily))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_calls_stop_at_the_thread_and_shop_ceilings() {
        assert!(under_error_caps(0, 0, 1000));
        assert!(under_error_caps(ERROR_THREAD_HOURLY_CAP - 1, 0, 1000));
        assert!(!under_error_caps(ERROR_THREAD_HOURLY_CAP, 0, 1000), "thread ceiling reached");
        assert!(!under_error_caps(0, 1000, 1000), "shop daily cap reached");
    }
}
