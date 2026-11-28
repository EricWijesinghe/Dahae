// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

use crate::{config::Config, providers::Providers, realtime::Hub};
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    pub providers: Arc<Providers>,
    pub hub: Hub,
    pub ai_debounce: crate::ai::Debounce,
    pub address_search: Arc<crate::routes::address::SearchGate>,
    pub review_reports: Arc<crate::ratelimit::UserLimiter>,
    pub lockout: Arc<crate::lockout::Lockout>,
    /// The country pack this install runs, merged and validated at boot.
    pub pack: Arc<crate::pack::Pack>,
    /// Login and OTP reads match the E.164 phone copies (`phone::reads_ready`, set once per start).
    pub phone_e164_reads: Arc<std::sync::atomic::AtomicBool>,
}
