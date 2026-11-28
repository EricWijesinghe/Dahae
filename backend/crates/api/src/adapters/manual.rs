// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! The `manual` payout rail: the owner transfers from their own bank and confirms each payout in the console
//! (`routes::payouts`, requested -> paid). Nothing leaves the server, so the rail only names what happens.

use super::payments::ready;
use super::payouts::{KycStatus, PayeeDraft, PayeeRef, PayoutRail, Transfer, TransferStatus};
use super::{AnyAdapter, Fut};
use crate::integrations::Field;
use crate::providers::{ProviderError, Providers};
use domain::money::Money;
use std::sync::Arc;

pub struct ManualPayouts;

pub fn build(_p: &Providers, _code: &'static str) -> AnyAdapter {
    AnyAdapter::Payout(Arc::new(ManualPayouts))
}

impl PayoutRail for ManualPayouts {
    /// The payout account is the pro's own verified account (`payout_accounts`), not a provider record.
    fn payee_schema(&self) -> &'static [Field] {
        &[]
    }

    fn register_payee(&self, d: PayeeDraft) -> Fut<'_, PayeeRef> {
        ready(Ok(PayeeRef(d.ref_id)))
    }

    /// Staff verify the account before a payout is made; there is no provider KYC.
    fn kyc_status<'a>(&'a self, _p: &'a PayeeRef) -> Fut<'a, KycStatus> {
        ready(Ok(KycStatus::Approved))
    }

    /// Queued for the owner: the id is our own reference.
    fn send(&self, t: Transfer) -> Fut<'_, String> {
        ready(Ok(t.ref_id))
    }

    /// Pending until the owner confirms it in the console, which core records, not the rail.
    fn status<'a>(&'a self, _id: &'a str) -> Fut<'a, TransferStatus> {
        ready(Ok(TransferStatus::Pending))
    }

    fn balance(&self) -> Fut<'_, Money> {
        ready(Err(ProviderError::NotConfigured("manual_balance")))
    }
}
