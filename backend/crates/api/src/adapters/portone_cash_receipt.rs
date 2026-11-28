// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! `InvoiceSink` over today's PortOne cash receipts (`providers::portone_cash_receipt`): the same request body,
//! the same "already issued: read it back" rule and the same cancel. The issue job in `routes::cash_receipts` keeps
//! its queue, retries and encryption in front.

use super::invoice::{DocKind, InvoiceSink, IssuedDoc, TaxDoc};
use super::payments::ready;
use super::{AnyAdapter, Fut, R};
use crate::config::PortOne;
use crate::providers::{ProviderError, Providers, portone_cash_receipt as pcr};
use std::sync::Arc;

pub struct PortOneCashReceipts {
    p: Providers,
}

pub fn build(p: &Providers, _code: &'static str) -> AnyAdapter {
    AnyAdapter::Invoice(Arc::new(PortOneCashReceipts { p: p.clone() }))
}

/// The PortOne settings and API secret, when cash receipts are switched on and keyed (`routes::cash_receipts::live`).
fn live(p: &Providers) -> R<(PortOne, String)> {
    let portone = p.integ().portone.clone();
    if !portone.cash_receipts {
        return Err(ProviderError::NotConfigured("cash_receipts"));
    }
    let secret = portone.api_secret.clone().ok_or(ProviderError::NotConfigured("portone"))?;
    Ok((portone, secret))
}

impl InvoiceSink for PortOneCashReceipts {
    fn kinds(&self) -> &[DocKind] {
        &[DocKind::CashReceipt]
    }

    /// `d.id` is the PortOne payment the receipt is for; `buyer` the phone or business number it is issued to.
    fn issue(&self, d: TaxDoc) -> Fut<'_, IssuedDoc> {
        if d.kind != DocKind::CashReceipt {
            return ready(Err(ProviderError::NotConfigured("portone_cash_receipt")));
        }
        Box::pin(async move {
            let (portone, secret) = live(&self.p)?;
            // shortcut: installs saved before the `cash_receipt` slot issue through the payment method's channel in
            // the route; a TaxDoc carries no method, so here that slot is required. Add the method if a caller needs it.
            let channel = portone
                .channel_keys
                .get("cash_receipt")
                .ok_or(ProviderError::NotConfigured("portone_channel"))?;
            let identity = d.buyer.as_deref().unwrap_or_default();
            let body = pcr::issue_body(portone.store_id.clone(), &d.id, channel, d.business, identity, d.total.0);
            let issued = pcr::issue(self.p.http(), self.p.portone_api(), &secret, &body).await?;
            Ok(IssuedDoc {
                id: d.id,
                number: Some(issued.issue_number),
            })
        })
    }

    /// Cancels the whole receipt of a payment. PortOne takes no reason.
    fn cancel<'a>(&'a self, id: &'a str, _reason: &'a str) -> Fut<'a, ()> {
        Box::pin(async move {
            let (_, secret) = live(&self.p)?;
            pcr::cancel(self.p.http(), self.p.portone_api(), &secret, id).await
        })
    }
}
