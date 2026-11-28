// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! `PayoutRail` over today's Toss payouts (지급대행): sellers with KYC at Toss, JWE-encrypted calls. Every call goes
//! through the same `Providers` method the payout routes and the sync use (dev fake included), so behaviour is
//! unchanged. The `manual` rail is in `manual.rs`.

use super::payments::ready;
use super::payouts::{KycStatus, PayeeDraft, PayeeRef, PayoutRail, Transfer, TransferStatus};
use super::{AnyAdapter, Fut, R};
use crate::integrations::{Field, Kind};
use crate::providers::{
    ProviderError, Providers,
    payout::{self, Business, SellerDraft},
};
use domain::money::Money;
use serde_json::{Map, Value};
use std::sync::Arc;

/// What a Toss seller is registered with (`routes::payouts::seller_draft` validates the same inputs). `bank` is the
/// bank's name as a pro types it (`providers::payout::BANKS`); `business_type` is `individual` or `company`.
pub const PAYEE_FIELDS: [Field; 9] = [
    field("bank", Kind::Text, true),
    field("account", Kind::Secret, true),
    field("holder", Kind::Text, true),
    field("business_type", Kind::Text, true),
    field("name", Kind::Text, false),
    field("email", Kind::Text, true),
    field("phone", Kind::Text, true),
    field("representative", Kind::Text, false),
    field("registration_no", Kind::Text, false),
];

const fn field(name: &'static str, kind: Kind, required: bool) -> Field {
    Field { name, kind, required }
}

pub struct TossPayouts {
    p: Providers,
}

pub fn build(p: &Providers, _code: &'static str) -> AnyAdapter {
    AnyAdapter::Payout(Arc::new(TossPayouts { p: p.clone() }))
}

fn text(f: &Map<String, Value>, name: &str) -> String {
    f.get(name).and_then(Value::as_str).map(str::trim).unwrap_or_default().to_string()
}

fn required(f: &Map<String, Value>, name: &str) -> R<String> {
    Some(text(f, name))
        .filter(|v| !v.is_empty())
        .ok_or(ProviderError::Declined("invalid_payee".into()))
}

/// The seller draft for a payee. Refuses a missing field or an unknown bank, as the route does.
pub fn seller_draft(d: &PayeeDraft) -> R<SellerDraft> {
    let f = &d.fields;
    let bank_code = payout::bank_code(&required(f, "bank")?).ok_or(ProviderError::Declined("unsupported_bank".into()))?;
    let (email, phone) = (required(f, "email")?, required(f, "phone")?);
    let business = match required(f, "business_type")?.as_str() {
        "individual" => Business::Individual {
            name: required(f, "name")?,
            email,
            phone,
        },
        "company" => Business::Company {
            name: Some(text(f, "name")).filter(|n| !n.is_empty()),
            representative: required(f, "representative")?,
            registration_no: required(f, "registration_no")?.chars().filter(char::is_ascii_digit).collect(),
            email,
            phone,
        },
        _ => return Err(ProviderError::Declined("invalid_business_type".into())),
    };
    Ok(SellerDraft {
        ref_id: d.ref_id.clone(),
        business,
        bank_code,
        account: required(f, "account")?,
        holder: required(f, "holder")?,
    })
}

/// Toss seller statuses: only `APPROVED` may be paid without a limit. `APPROVAL_REQUIRED`, `PARTIALLY_APPROVED`,
/// `KYC_REQUIRED` and anything new wait for the pro to finish KYC at Toss.
pub fn kyc(status: &str) -> KycStatus {
    if status == "APPROVED" { KycStatus::Approved } else { KycStatus::Pending }
}

impl PayoutRail for TossPayouts {
    fn payee_schema(&self) -> &'static [Field] {
        &PAYEE_FIELDS
    }

    /// Registers a Toss seller; the payee ref is Toss's seller id.
    fn register_payee(&self, d: PayeeDraft) -> Fut<'_, PayeeRef> {
        Box::pin(async move {
            let draft = seller_draft(&d)?;
            let (id, _status) = self.p.register_seller(&draft).await?;
            Ok(PayeeRef(id))
        })
    }

    fn kyc_status<'a>(&'a self, p: &'a PayeeRef) -> Fut<'a, KycStatus> {
        Box::pin(async move { self.p.seller_status(&p.0).await.map(|(_, status)| kyc(&status)) })
    }

    /// Toss pays the account registered on the seller: a transfer carries only its amount and memo.
    fn send(&self, t: Transfer) -> Fut<'_, String> {
        Box::pin(async move {
            let transfer = payout::Transfer {
                amount: t.amount.0,
                bank_code: "",
                account: String::new(),
                holder: String::new(),
                memo: t.memo,
            };
            self.p.send_transfer(&t.idem, &t.ref_id, &t.payee.0, &transfer).await
        })
    }

    fn status<'a>(&'a self, id: &'a str) -> Fut<'a, TransferStatus> {
        Box::pin(self.p.transfer_status(id))
    }

    /// shortcut: today only the wizard's connection test calls `/v2/balances`, for its status code; read the amount
    /// when a payout screen shows it.
    fn balance(&self) -> Fut<'_, Money> {
        ready(Err(ProviderError::NotConfigured("payout_balance")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn draft(fields: Value) -> PayeeDraft {
        PayeeDraft {
            ref_id: "mk0123456789abr1".into(),
            fields: fields.as_object().cloned().unwrap_or_default(),
        }
    }

    #[test]
    fn payee_fields_become_a_seller_draft() {
        let individual =
            json!({ "bank": "국민", "account": "123", "holder": "김", "business_type": "individual", "name": "김", "email": "a@b.c", "phone": "01012345678" });
        let d = seller_draft(&draft(individual)).expect("draft");
        assert_eq!((d.ref_id.as_str(), d.bank_code, d.account.as_str()), ("mk0123456789abr1", "004", "123"));
        let company = json!({ "bank": "국민", "account": "1", "holder": "h", "business_type": "company", "representative": "r", "registration_no": "123-45-67890", "email": "e", "phone": "p" });
        let d = seller_draft(&draft(company)).expect("company");
        assert!(matches!(d.business, Business::Company { ref registration_no, name: None, .. } if registration_no == "1234567890"));
        assert!(seller_draft(&draft(json!({ "bank": "국민" }))).is_err(), "missing fields");
        let unknown = json!({ "bank": "nowhere", "account": "1", "holder": "h", "business_type": "individual", "name": "n", "email": "e", "phone": "p" });
        assert!(matches!(seller_draft(&draft(unknown)), Err(ProviderError::Declined(m)) if m == "unsupported_bank"));
    }

    #[test]
    fn only_an_approved_seller_counts_as_kyc_done() {
        assert_eq!(kyc("APPROVED"), KycStatus::Approved);
        for s in ["APPROVAL_REQUIRED", "PARTIALLY_APPROVED", "KYC_REQUIRED", "UNKNOWN"] {
            assert_eq!(kyc(s), KycStatus::Pending, "{s}");
        }
    }
}
