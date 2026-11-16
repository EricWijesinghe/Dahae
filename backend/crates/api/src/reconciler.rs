// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Background recovery for checkouts whose confirm or webhook never arrived (money taken,
//! booking missing), and for compensating refunds that failed (`refund_queue`). Runs on every
//! replica: each pass claims rows with `FOR UPDATE SKIP LOCKED` and stamps them with a lease, and
//! settlement itself is idempotent (`fulfil` re-locks the checkout row).

use crate::{
    error::ApiResult,
    lifecycle::next_tick,
    metrics::job_pass,
    routes::{cash_receipts, checkout_recon},
    state::AppState,
};
use std::time::Duration;
use tokio::task::JoinHandle;

/// Dev mode always runs it (the fake gateway answers); production needs PortOne configured.
/// Only against a real PortOne account: in dev mode the fake gateway reports every
/// checkout as paid, so reconciling would turn abandoned checkouts into bookings.
pub fn enabled(_dev_mode: bool, portone_configured: bool) -> bool {
    portone_configured
}

/// Starts the periodic pass. Each tick re-checks the live keys, so PortOne entered later in the
/// setup wizard switches recovery on without a restart. Errors are logged and the loop goes on.
pub fn spawn(state: AppState) -> JoinHandle<()> {
    let every = Duration::from_secs(state.config.reconcile_interval_secs);
    let mut stop = state.hub.drain_signal();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(every);
        while next_tick(&mut tick, &mut stop).await {
            if !enabled(state.config.dev_mode, state.providers.integ().portone.api_secret.is_some()) {
                job_pass("reconciler", true);
                continue;
            }
            let (checked, ok) = recon_pass(&state).await;
            if checked > 0 {
                tracing::info!(checked, "checkout reconcile pass");
            }
            // The diff has its own outcome: a failed daily pull is logged, it does not fail this job.
            log_step("payment diff pass", checkout_recon::payment_diff_pass(&state).await);
            let receipts_ok = receipt_cancel_pass(&state).await;
            job_pass("reconciler", ok && receipts_ok);
        }
    })
}

/// Open checkouts, then provider-paid checkouts left unfinished for a day. Each step runs even
/// when the other failed. Returns the checked count (diff mismatches excluded) and whether both succeeded.
async fn recon_pass(state: &AppState) -> (usize, bool) {
    let open = log_step("checkout reconcile pass", checkout_recon::reconcile_pass(state).await);
    let stale = log_step("stale reconcile pass", checkout_recon::reconcile_stale(state).await);
    tally(&[open, stale])
}

/// Logs a failed step and turns its outcome into `Some(count)` or `None`.
fn log_step(step: &'static str, result: ApiResult<usize>) -> Option<usize> {
    match result {
        Ok(n) => Some(n),
        Err(e) => {
            tracing::warn!(error = %e, step, "reconcile step failed");
            None
        }
    }
}

/// Sums the counts of the steps that ran and reports whether every step succeeded.
fn tally(steps: &[Option<usize>]) -> (usize, bool) {
    (steps.iter().flatten().sum(), steps.iter().all(Option::is_some))
}

/// Retries cash-receipt cancels that a refund could not record. Errors are logged; the pass fails.
async fn receipt_cancel_pass(state: &AppState) -> bool {
    match cash_receipts::retry_cancels(state).await {
        Ok(_) => true,
        Err(e) => {
            tracing::warn!(error = %e, "cash receipt cancel retry failed");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{enabled, tally};

    #[test]
    fn a_failed_step_keeps_the_others_counts_and_fails_the_job() {
        assert_eq!(tally(&[Some(2), None]), (2, false));
        assert_eq!(tally(&[Some(1), Some(3)]), (4, true));
        assert_eq!(tally(&[None, None]), (0, false));
    }

    #[test]
    fn runs_only_with_a_real_portone_account() {
        assert!(!enabled(true, false), "dev fakes must never auto-settle abandoned checkouts");
        assert!(enabled(false, true));
        assert!(enabled(true, true));
        assert!(!enabled(false, false));
    }
}
