// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! The payout port: paying pros out through a PSP that keeps their KYC. Today's Toss payouts still run through
//! `Providers` (`send_transfer`, `transfer_status`); a shim lands with the next payout adapter.

use super::Fut;
use crate::integrations::Field;
pub use crate::providers::payout::TransferStatus;
use domain::money::Money;
use serde_json::{Map, Value};

/// A payee to register, in the shape `payee_schema` asks for (IBAN, KR bank and account, M-Pesa MSISDN …).
pub struct PayeeDraft {
    /// Our id for the payee (the pro), sent as the PSP's reference.
    pub ref_id: String,
    pub fields: Map<String, Value>,
}

/// The PSP's id for a registered payee.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PayeeRef(pub String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KycStatus {
    Pending,
    Approved,
    Rejected(String),
}

/// One transfer. `idem` stays the same for every retry of one payout.
pub struct Transfer {
    pub idem: String,
    pub ref_id: String,
    pub payee: PayeeRef,
    pub amount: Money,
    pub memo: String,
}

pub trait PayoutRail: Send + Sync {
    fn payee_schema(&self) -> &'static [Field];
    /// KYC stays at the PSP.
    fn register_payee(&self, p: PayeeDraft) -> Fut<'_, PayeeRef>;
    fn kyc_status<'a>(&'a self, p: &'a PayeeRef) -> Fut<'a, KycStatus>;
    /// Returns the provider's transfer id.
    fn send(&self, t: Transfer) -> Fut<'_, String>;
    fn status<'a>(&'a self, id: &'a str) -> Fut<'a, TransferStatus>;
    fn balance(&self) -> Fut<'_, Money>;
}
