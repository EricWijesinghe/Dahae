// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! The tax-document port: cash receipts, tax invoices, e-receipts and credit notes issued at a national system or
//! provider. Today's PortOne cash receipts still run through `providers::portone_cash_receipt`.

use super::Fut;
use domain::money::Money;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocKind {
    CashReceipt,
    TaxInvoice,
    EReceipt,
    CreditNote,
}

pub struct TaxDoc {
    pub kind: DocKind,
    /// Our document id.
    pub id: String,
    pub total: Money,
    pub tax: Money,
    pub currency: String,
    /// The buyer's identifier (phone, business number, tax id), when the document names one.
    pub buyer: Option<String>,
    /// Chained numbering (ZATCA, PT): our sequence number and the previous document's hash.
    pub seq_no: i64,
    pub prev_hash: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IssuedDoc {
    /// The provider's id, which `cancel` takes.
    pub id: String,
    /// The number printed on the document, when the system assigns one.
    pub number: Option<String>,
}

pub trait InvoiceSink: Send + Sync {
    fn kinds(&self) -> &[DocKind];
    fn issue(&self, d: TaxDoc) -> Fut<'_, IssuedDoc>;
    fn cancel<'a>(&'a self, id: &'a str, reason: &'a str) -> Fut<'a, ()>;
}
