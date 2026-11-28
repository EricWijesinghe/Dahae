// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! In-memory fakes of the six ports besides payments (`fake.rs`): messaging, payouts, sign-in, address search, tax
//! documents and identity. Like the fake gateways they hold no keys, reach nothing and refuse to exist outside dev
//! mode (§8.4). The contract suite (`tests/contract_ports.rs`) runs each port's checks against its fake and its KR
//! shim, so a fresh-country flow can run end to end before a real provider ships.

use super::address::{AddressHit, AddressLookup, AddressQuery};
use super::identity::{IdPurpose, IdSession, IdentityInfo, IdentityVerifier, Subject};
use super::invoice::{DocKind, InvoiceSink, IssuedDoc, TaxDoc};
use super::login::{SocialUser, SocialVerifier};
use super::messaging::{ChannelCaps, ChannelKind, Messenger, OutMsg, SendRef};
use super::payments::ready;
use super::payouts::{KycStatus, PayeeDraft, PayeeRef, PayoutRail, Transfer, TransferStatus};
use super::{Fut, R};
use crate::integrations::Field;
use crate::providers::ProviderError;
use domain::money::Money;
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

/// Token prefix the fake sign-in accepts: `fake:<provider user id>`.
pub const FAKE_TOKEN: &str = "fake:";
/// What the fake payout account starts with.
pub const FAKE_PAYOUT_BALANCE: Money = Money(10_000_000);

fn dev_only(dev_mode: bool, what: &'static str) -> R<()> {
    if dev_mode { Ok(()) } else { Err(ProviderError::NotConfigured(what)) }
}

/// A lock whose holder panicked still guards whole entries only.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn declined<T>(code: &str) -> R<T> {
    Err(ProviderError::Declined(code.into()))
}

// ---- messaging ----

const FAKE_CHANNELS: [ChannelCaps; 3] = [
    ChannelCaps {
        kind: ChannelKind::Sms,
        preapproved_templates: false,
        max_chars: None,
        sender_registration: false,
    },
    ChannelCaps {
        kind: ChannelKind::AlimTalk,
        preapproved_templates: true,
        max_chars: None,
        sender_registration: false,
    },
    ChannelCaps {
        kind: ChannelKind::WhatsApp,
        preapproved_templates: true,
        max_chars: None,
        sender_registration: false,
    },
];

/// One message the fake accepted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sent {
    pub to_e164: String,
    pub channel: ChannelKind,
    pub template_key: String,
    pub text: String,
}

pub struct FakeMessenger {
    sent: Mutex<Vec<Sent>>,
}

impl FakeMessenger {
    pub fn new(dev_mode: bool) -> R<Self> {
        dev_only(dev_mode, "fake_messenger")?;
        Ok(Self { sent: Mutex::default() })
    }

    /// Every message accepted so far, oldest first.
    pub fn sent(&self) -> Vec<Sent> {
        lock(&self.sent).clone()
    }
}

impl Messenger for FakeMessenger {
    fn channels(&self) -> &[ChannelCaps] {
        &FAKE_CHANNELS
    }

    /// Refuses a channel it does not offer, a number not in E.164, and a template channel without a template key.
    fn send(&self, m: OutMsg) -> Fut<'_, SendRef> {
        let Some(caps) = FAKE_CHANNELS.iter().find(|c| c.kind == m.channel) else {
            return ready(Err(ProviderError::NotConfigured("fake_channel")));
        };
        if !m.to_e164.starts_with('+') || m.to_e164.len() < 8 {
            return ready(declined("invalid_number"));
        }
        if caps.preapproved_templates && m.template_key.is_empty() {
            return ready(declined("template_required"));
        }
        let mut sent = lock(&self.sent);
        sent.push(Sent {
            to_e164: m.to_e164,
            channel: m.channel,
            template_key: m.template_key,
            text: m.fallback_text,
        });
        ready(Ok(SendRef(format!("fake-msg-{}", sent.len()))))
    }

    fn balance(&self) -> Fut<'_, Option<Money>> {
        ready(Ok(None))
    }
}

// ---- payouts ----

#[derive(Default)]
struct PayoutState {
    payees: Vec<String>,
    /// Transfers by idempotency key: (provider id, amount).
    transfers: HashMap<String, (String, i64)>,
}

pub struct FakePayouts {
    state: Mutex<PayoutState>,
}

impl FakePayouts {
    pub fn new(dev_mode: bool) -> R<Self> {
        dev_only(dev_mode, "fake_payouts")?;
        Ok(Self { state: Mutex::default() })
    }
}

impl PayoutRail for FakePayouts {
    fn payee_schema(&self) -> &'static [Field] {
        &[]
    }

    fn register_payee(&self, d: PayeeDraft) -> Fut<'_, PayeeRef> {
        let id = format!("fake-payee-{}", d.ref_id);
        lock(&self.state).payees.push(id.clone());
        ready(Ok(PayeeRef(id)))
    }

    fn kyc_status<'a>(&'a self, p: &'a PayeeRef) -> Fut<'a, KycStatus> {
        let known = lock(&self.state).payees.contains(&p.0);
        ready(if known { Ok(KycStatus::Approved) } else { Err(ProviderError::NotFound) })
    }

    /// A retry with the same `idem` returns the first transfer's id and moves nothing again.
    fn send(&self, t: Transfer) -> Fut<'_, String> {
        let mut s = lock(&self.state);
        if let Some((id, _)) = s.transfers.get(&t.idem) {
            return ready(Ok(id.clone()));
        }
        if !s.payees.contains(&t.payee.0) {
            return ready(declined("payee_not_found"));
        }
        let spent: i64 = s.transfers.values().map(|(_, a)| a).sum();
        if t.amount.0 <= 0 || t.amount.0 > FAKE_PAYOUT_BALANCE.0 - spent {
            return ready(declined("invalid_amount"));
        }
        let id = format!("fake-payout-{}", s.transfers.len() + 1);
        s.transfers.insert(t.idem, (id.clone(), t.amount.0));
        ready(Ok(id))
    }

    fn status<'a>(&'a self, id: &'a str) -> Fut<'a, TransferStatus> {
        let known = lock(&self.state).transfers.values().any(|(t, _)| t == id);
        ready(if known { Ok(TransferStatus::Paid) } else { Err(ProviderError::NotFound) })
    }

    fn balance(&self) -> Fut<'_, Money> {
        let spent: i64 = lock(&self.state).transfers.values().map(|(_, a)| a).sum();
        ready(Ok(Money(FAKE_PAYOUT_BALANCE.0 - spent)))
    }
}

// ---- sign-in ----

pub struct FakeSocial {
    revoked: Mutex<Vec<String>>,
}

impl FakeSocial {
    pub fn new(dev_mode: bool) -> R<Self> {
        dev_only(dev_mode, "fake_social")?;
        Ok(Self { revoked: Mutex::default() })
    }

    /// The provider user ids revoked so far.
    pub fn revoked(&self) -> Vec<String> {
        lock(&self.revoked).clone()
    }
}

impl SocialVerifier for FakeSocial {
    fn verify<'a>(&'a self, token: &'a str, _nonce: Option<&'a str>) -> Fut<'a, SocialUser> {
        ready(match token.strip_prefix(FAKE_TOKEN).filter(|id| !id.is_empty() && id.len() <= 128) {
            Some(id) => Ok(SocialUser {
                provider_user_id: id.into(),
                email: None,
                audience: None,
                refresh_token: None,
            }),
            None => declined("invalid_token"),
        })
    }

    fn revoke<'a>(&'a self, u: &'a SocialUser) -> Fut<'a, ()> {
        lock(&self.revoked).push(u.provider_user_id.clone());
        ready(Ok(()))
    }
}

// ---- address ----

pub struct FakeAddress;

/// What the fake knows: two addresses in two service areas.
fn known() -> [AddressHit; 2] {
    let hit = |label: &str, postcode: &str, area: &str| AddressHit {
        label: label.into(),
        postcode: postcode.into(),
        area_code: area.into(),
        at: None,
    };
    [
        hit("1 Fake Street, Testville", "TE1 1ST", "11680"),
        hit("2 Sample Road, Mockton", "MO2 2CK", "11650"),
    ]
}

impl FakeAddress {
    pub fn new(dev_mode: bool) -> R<Self> {
        dev_only(dev_mode, "fake_address")?;
        Ok(Self)
    }
}

impl AddressLookup for FakeAddress {
    /// Matches the keyword in the label or the postcode, ten to a page. Refuses one or no characters.
    fn search(&self, q: AddressQuery) -> Fut<'_, Vec<AddressHit>> {
        let k = q.keyword.trim().to_lowercase();
        if k.chars().count() < 2 {
            return ready(declined("invalid_query"));
        }
        if q.page == 0 {
            return ready(declined("invalid_page"));
        }
        let hits = known()
            .into_iter()
            .filter(|h| h.label.to_lowercase().contains(&k) || h.postcode.to_lowercase() == k)
            .filter(|_| q.page == 1)
            .collect();
        ready(Ok(hits))
    }
}

// ---- tax documents ----

pub struct FakeInvoices {
    /// Issued documents by our id: (number, cancelled).
    docs: Mutex<HashMap<String, (String, bool)>>,
}

impl FakeInvoices {
    pub fn new(dev_mode: bool) -> R<Self> {
        dev_only(dev_mode, "fake_invoices")?;
        Ok(Self { docs: Mutex::default() })
    }
}

impl InvoiceSink for FakeInvoices {
    fn kinds(&self) -> &[DocKind] {
        &[DocKind::CashReceipt, DocKind::TaxInvoice]
    }

    /// Issuing an id again returns the first document, as PortOne reads an issued receipt back.
    fn issue(&self, d: TaxDoc) -> Fut<'_, IssuedDoc> {
        if !self.kinds().contains(&d.kind) {
            return ready(Err(ProviderError::NotConfigured("fake_doc_kind")));
        }
        if d.total.0 <= 0 || d.tax.0 < 0 || d.tax.0 > d.total.0 {
            return ready(declined("invalid_amount"));
        }
        let mut docs = lock(&self.docs);
        let next = format!("FAKE-{:06}", docs.len() + 1);
        let (number, _) = docs.entry(d.id.clone()).or_insert((next, false));
        ready(Ok(IssuedDoc {
            id: d.id,
            number: Some(number.clone()),
        }))
    }

    fn cancel<'a>(&'a self, id: &'a str, _reason: &'a str) -> Fut<'a, ()> {
        ready(match lock(&self.docs).get_mut(id) {
            Some((_, cancelled)) => {
                *cancelled = true;
                Ok(())
            }
            None => Err(ProviderError::NotFound),
        })
    }
}

// ---- identity ----

pub struct FakeIdentity {
    sessions: Mutex<Vec<String>>,
}

impl FakeIdentity {
    pub fn new(dev_mode: bool) -> R<Self> {
        dev_only(dev_mode, "fake_identity")?;
        Ok(Self { sessions: Mutex::default() })
    }
}

impl IdentityVerifier for FakeIdentity {
    /// The customer completes the check at once.
    fn start(&self, _s: Subject, _purpose: IdPurpose) -> Fut<'_, IdSession> {
        let mut sessions = lock(&self.sessions);
        let id = format!("fake-idv-{}", sessions.len() + 1);
        sessions.push(id.clone());
        ready(Ok(IdSession {
            id,
            redirect_url: None,
            sdk_params: None,
        }))
    }

    fn result<'a>(&'a self, id: &'a str) -> Fut<'a, IdentityInfo> {
        let known = lock(&self.sessions).iter().any(|s| s == id);
        ready(if known {
            Ok(IdentityInfo {
                id: id.into(),
                status: "VERIFIED".into(),
                ci: Some(format!("fake-ci:{id}")),
                name: None,
            })
        } else {
            Err(ProviderError::NotFound)
        })
    }
}

// ---- the Dark adapters' stand-ins ----

/// What the Dark adapters (`stripe`, `twilio`, `google_places`, `oidc`) run on until their real shims ship: one
/// process-wide fake each, so a fresh-country flow keeps its state across calls. `adapters::adapter` refuses them
/// outside dev mode before this runs (`AdapterSpec::fake`), so the dev-only constructors cannot fail here.
pub fn build_dark(p: &crate::providers::Providers, code: &'static str) -> super::AnyAdapter {
    use super::AnyAdapter;
    use super::fake::{FakeGateway, Flavour};
    use std::sync::{Arc, OnceLock};
    static PAY: OnceLock<Arc<FakeGateway>> = OnceLock::new();
    static MSG: OnceLock<Arc<FakeMessenger>> = OnceLock::new();
    static ADDRESS: OnceLock<Arc<FakeAddress>> = OnceLock::new();
    static LOGIN: OnceLock<Arc<FakeSocial>> = OnceLock::new();
    const DEV: &str = "a Dark adapter is built only in dev mode";
    match code {
        // Checkout then read, like Stripe Checkout: the payment exists once the customer paid in the window.
        "stripe" => AnyAdapter::Pay(
            PAY.get_or_init(|| Arc::new(FakeGateway::new(Flavour::PortOne, true, *p.data_key(), "dark-fake").expect(DEV)))
                .clone(),
        ),
        "twilio" => AnyAdapter::Msg(MSG.get_or_init(|| Arc::new(FakeMessenger::new(true).expect(DEV))).clone()),
        "google_places" => AnyAdapter::Address(ADDRESS.get_or_init(|| Arc::new(FakeAddress::new(true).expect(DEV))).clone()),
        _ => AnyAdapter::Login(LOGIN.get_or_init(|| Arc::new(FakeSocial::new(true).expect(DEV))).clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_fake_refuses_to_exist_outside_dev_mode() {
        assert!(FakeMessenger::new(false).is_err());
        assert!(FakePayouts::new(false).is_err());
        assert!(FakeSocial::new(false).is_err());
        assert!(FakeAddress::new(false).is_err());
        assert!(FakeInvoices::new(false).is_err());
        assert!(FakeIdentity::new(false).is_err());
        assert!(FakeMessenger::new(true).is_ok() && FakeIdentity::new(true).is_ok());
    }
}
