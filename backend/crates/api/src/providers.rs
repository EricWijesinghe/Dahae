// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Adapters for paid external services. Dev mode uses in-process fakes so the
//! whole system runs offline; production adapters plug in behind the same API.

pub mod apple;
pub mod fcm;
pub mod payout;
pub mod portone;
pub mod portone_cash_receipt;
pub mod sms;
pub mod social;
pub mod toss_jwe;
pub mod toss_payments;
pub mod toss_webhook;

use crate::entropy;
use crate::{
    config::Config,
    crypto,
    error::ApiError,
    integrations::{Integrations, PayoutMode},
    ops,
};
use payout::{Transfer, TransferStatus};
use reqwest::{Request, StatusCode};
use serde_json::Value;
use sqlx::PgPool;
use std::{
    sync::{Arc, RwLock},
    time::Duration,
};
use uuid::Uuid;

const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("declined: {0}")]
    Declined(String),
    #[error("provider not configured: {0}")]
    NotConfigured(&'static str),
    #[error("provider request failed: {0}")]
    Upstream(String),
    #[error("not found")]
    NotFound,
}

impl From<ProviderError> for ApiError {
    fn from(e: ProviderError) -> Self {
        match e {
            ProviderError::Declined(m) => ApiError::PaymentFailed(m),
            ProviderError::NotFound => ApiError::NotFound,
            ProviderError::NotConfigured(_) => ApiError::Unavailable("provider_not_configured"),
            ProviderError::Upstream(cause) => {
                // The mapped 503 carries no cause; this is the only place it is still known.
                tracing::warn!(%cause, "provider request failed");
                ApiError::Unavailable("provider_unreachable")
            }
        }
    }
}

/// Gateway code (and `payments.provider`) of Toss Payments one-time payments, on the window MID.
pub const TOSS: &str = "tosspayments";
/// `payments.provider` of a saved-card charge through Toss: it is made on the 자동결제 (billing) MID.
pub const TOSS_BILLING: &str = "tosspayments_billing";

/// The payment gateway a checkout or saved card is pinned to. New ones follow `Integrations::gateway_for`
/// (checkouts) and `Integrations::billing_gateway` (saved cards); rows never move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gateway {
    Portone,
    TossPayments,
}

impl Gateway {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Portone => "portone",
            Self::TossPayments => TOSS,
        }
    }

    pub fn parse(code: &str) -> Option<Self> {
        [Self::Portone, Self::TossPayments].into_iter().find(|g| g.code() == code)
    }

    /// `payments.provider` of a saved-card charge: Toss bills on its own MID.
    pub const fn billing_provider(self) -> &'static str {
        match self {
            Self::Portone => "portone",
            Self::TossPayments => TOSS_BILLING,
        }
    }
}

/// A saved card as stored: its gateway, the sealed key and Toss's customerKey.
pub struct SavedCard {
    pub gateway: Gateway,
    pub sealed_key: String,
    pub customer_key: Option<String>,
}

/// The Toss MID a stored provider code was charged on; None for any other provider.
fn toss_mid(provider: &str) -> Option<toss_payments::Mid> {
    match provider {
        TOSS => Some(toss_payments::Mid::Window),
        TOSS_BILLING => Some(toss_payments::Mid::Billing),
        _ => None,
    }
}

/// Billing key that the fake gateway always declines, for testing failure UX.
pub const FAKE_DECLINE_KEY: &str = "fake_decline";

/// What the payment provider says about one payment (amount in won).
pub struct PaymentInfo {
    pub status: String,
    pub total: i64,
    /// Amount the provider has taken back so far (-1 when it reports none).
    pub cancelled: i64,
    pub currency: String,
    /// Provider's method object; for virtual accounts it carries the account details.
    pub method: Value,
}

/// Identity verification result (PASS / 본인인증).
pub struct IdentityInfo {
    /// The verification id the provider reports; must equal the id we asked about.
    pub id: String,
    pub status: String,
    pub ci: Option<String>,
    pub name: Option<String>,
}

/// Keys in use right now; replaced as a whole when the owner saves the setup wizard.
struct Live {
    integ: Arc<Integrations>,
    fcm: Option<fcm::Fcm>,
}

impl Live {
    fn new(integ: Integrations) -> Self {
        let fcm = integ.fcm.clone().map(fcm::Fcm::new);
        Self { integ: Arc::new(integ), fcm }
    }
}

/// Maps a provider result to the ops health record. A declined card or a missing payment means the
/// provider answered, so it counts as healthy.
pub fn outcome<T>(r: &Result<T, ProviderError>) -> Result<(), &'static str> {
    match r {
        Ok(_) | Err(ProviderError::Declined(_) | ProviderError::NotFound) => Ok(()),
        Err(ProviderError::NotConfigured(_)) => Err("not_configured"),
        Err(ProviderError::Upstream(_)) => Err("provider_error"),
    }
}

#[derive(Clone)]
pub struct Providers {
    cfg: Config,
    http: reqwest::Client,
    live: Arc<RwLock<Live>>,
    db: PgPool,
    portone_api: String,
    /// Toss base URL override; None is the real API.
    toss_api: Option<String>,
}

fn random_id(prefix: &str) -> String {
    let mut b = [0u8; 12];
    entropy::fill(&mut b);
    format!("{prefix}_{}", hex::encode(b))
}

impl Providers {
    pub fn new(cfg: Config, db: PgPool) -> Result<Self, reqwest::Error> {
        let http = reqwest::Client::builder().timeout(HTTP_TIMEOUT).build()?;
        let live = Arc::new(RwLock::new(Live::new(Integrations::from_config(&cfg))));
        Ok(Self {
            cfg,
            http,
            live,
            db,
            portone_api: portone::API.into(),
            toss_api: None,
        })
    }

    /// Points the PortOne adapter at another base URL. Integration tests use it for a mock gateway.
    pub fn with_portone_api(mut self, base: &str) -> Self {
        base.trim_end_matches('/').clone_into(&mut self.portone_api);
        self
    }

    /// Points the Toss payout calls at another base URL. Integration tests use it for a mock.
    pub fn with_toss_api(mut self, base: &str) -> Self {
        self.toss_api = Some(base.trim_end_matches('/').to_string());
        self
    }

    pub fn portone_api(&self) -> &str {
        &self.portone_api
    }

    /// Feeds the status page and alert rules. Live traffic only: dev fakes never count as health.
    pub async fn observe(&self, integration: &'static str, outcome: Result<(), &'static str>) {
        if !self.dev() {
            ops::record(&self.db, integration, outcome).await;
        }
    }

    pub fn cfg(&self) -> &Config {
        &self.cfg
    }

    /// Snapshot of the live integration keys.
    pub fn integ(&self) -> Arc<Integrations> {
        // A panic while holding the lock cannot leave Live half-written (it is replaced whole).
        self.live.read().unwrap_or_else(|e| e.into_inner()).integ.clone()
    }

    fn fcm(&self) -> Option<fcm::Fcm> {
        self.live.read().unwrap_or_else(|e| e.into_inner()).fcm.clone()
    }

    /// Switches every provider to new keys (setup wizard save, periodic refresh).
    pub fn set_integrations(&self, integ: Integrations) {
        *self.live.write().unwrap_or_else(|e| e.into_inner()) = Live::new(integ);
    }

    /// The key that opens sealed billing keys (`Config::data_key`), for the dev-only fake gateways.
    pub fn data_key(&self) -> &[u8; 32] {
        &self.cfg.data_key
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    /// Dev mode: every provider call is answered by the in-process fake, so no PortOne keys are needed.
    pub fn dev(&self) -> bool {
        self.cfg.dev_mode
    }

    fn require_dev(&self, what: &'static str) -> Result<(), ProviderError> {
        // shortcut: only the legacy 'pg' gateway has no real adapter; PortOne is wired in portone.rs.
        if self.dev() { Ok(()) } else { Err(ProviderError::NotConfigured(what)) }
    }

    pub async fn send_sms(&self, phone: &str, text: &str) -> Result<(), ProviderError> {
        if !self.dev() {
            let integ = self.integ();
            let cfg = integ.solapi.as_ref().ok_or(ProviderError::NotConfigured("sms"))?;
            let sent = sms::send_sms(&self.http, cfg, phone, text).await;
            self.observe("solapi", outcome(&sent)).await;
            return sent;
        }
        tracing::info!(target: "fake_sms", phone, text, "sms sent");
        Ok(())
    }

    /// Transactional message for an alert kind (see `sms::alert_key`), in Korean.
    /// AlimTalk when a template and channel id are configured. SMS with the same text when there is no
    /// template, or when the AlimTalk call fails. Dev mode only logs.
    pub async fn send_customer_message(&self, phone: &str, kind: &str, brand: &str, title: &str, body: &str) -> Result<(), ProviderError> {
        let integ = self.integ();
        let Some(cfg) = integ.solapi.as_ref() else {
            return Ok(());
        };
        let text = format!("{title}\n{body}");
        let template = cfg.templates.get(kind).zip(cfg.pf_id.as_deref());
        if self.dev() {
            tracing::info!(target: "fake_alimtalk", phone, kind, text, has_template = template.is_some(), "customer message sent");
            return Ok(());
        }
        let Some((template_id, pf_id)) = template else {
            let sent = sms::send_sms(&self.http, cfg, phone, &text).await;
            self.observe("solapi", outcome(&sent)).await;
            return sent;
        };
        let variables = sms::alimtalk_variables(brand, title, body);
        let sent = sms::send_alimtalk(&self.http, cfg, pf_id, template_id, phone, &text, variables).await;
        let final_result = match sent {
            Ok(()) => Ok(()),
            Err(e) => {
                tracing::warn!(error = %e, kind, "alimtalk failed; sending sms");
                sms::send_sms(&self.http, cfg, phone, &text).await
            }
        };
        self.observe("solapi", outcome(&final_result)).await;
        final_result
    }

    /// Exchange the gateway's card-auth result for a reusable billing key.
    /// Dev mode fakes it. PortOne: the key must be `ISSUED` and bound to `customer` and `issue` before it is stored.
    /// Toss: the authKey is exchanged for a billingKey bound to the customerKey `issue`.
    pub async fn issue_billing_key(&self, gw: Gateway, auth_key: &str, customer: &str, issue: &str) -> Result<String, ProviderError> {
        if !self.dev() {
            return match gw {
                Gateway::Portone => {
                    let verified = portone::verify_billing_key(self, auth_key, customer, issue).await;
                    self.observe("portone", outcome(&verified)).await;
                    verified.map(|()| auth_key.to_string())
                }
                Gateway::TossPayments => {
                    let issued = toss_payments::issue_billing_key(self, auth_key, issue).await;
                    self.observe(TOSS, outcome(&issued)).await;
                    issued
                }
            };
        }
        if auth_key == FAKE_DECLINE_KEY {
            return Ok(FAKE_DECLINE_KEY.into());
        }
        Ok(random_id("bk"))
    }

    /// Charges a saved card once (subscription visits, booking payments, tips) through the gateway it was saved on.
    /// Opens the sealed key first. `currency` is the one the payment is stamped with (ISO 4217). Dev mode fakes it;
    /// `FAKE_DECLINE_KEY` declines.
    pub async fn charge_billing(
        &self,
        card: &SavedCard,
        amount: i64,
        currency: &str,
        payment_id: &str,
        order: &str,
        customer: Uuid,
    ) -> Result<String, ProviderError> {
        let Some(billing_key) = crypto::open_billing_key(&self.cfg.data_key, &card.sealed_key) else {
            tracing::error!(%customer, payment_id, "saved-card billing key cannot be opened: charge refused");
            return Err(ProviderError::Declined("billing_key_unreadable".into()));
        };
        if !self.dev() {
            return match card.gateway {
                Gateway::Portone => {
                    let charged = portone::charge_billing(self, &billing_key, payment_id, amount, currency, order, customer).await;
                    self.observe("portone", outcome(&charged)).await;
                    charged
                }
                Gateway::TossPayments => {
                    let customer_key = card.customer_key.as_deref().ok_or(ProviderError::Declined("billing_key_not_bound".into()))?;
                    let charged = toss_payments::charge_billing(self, &billing_key, customer_key, payment_id, amount, currency, order).await;
                    self.observe(TOSS, outcome(&charged)).await;
                    charged
                }
            };
        }
        if billing_key == FAKE_DECLINE_KEY {
            return Err(ProviderError::Declined("card_declined".into()));
        }
        tracing::info!(target: "fake_pg", amount, payment_id, "billed");
        Ok(random_id("tx"))
    }

    /// Stops a saved card at the gateway it was saved on. Takes the opened key.
    pub async fn delete_billing_key(&self, gw: Gateway, key: &str) -> Result<(), ProviderError> {
        let deleted = match gw {
            Gateway::Portone => portone::delete_billing_key(self, key).await,
            Gateway::TossPayments => toss_payments::delete_billing_key(self, key).await,
        };
        self.observe(if gw == Gateway::Portone { "portone" } else { TOSS }, outcome(&deleted)).await;
        deleted
    }

    /// Whether some gateway can save cards: PortOne's billing channel, or Toss (offered) with its 자동결제 key pair.
    pub fn billing_ready(&self) -> bool {
        self.integ().billing_gateway().is_some()
    }

    /// Refunds (or cancels) a payment at the gateway that took it.
    pub async fn refund(&self, provider: &str, tx_id: &str, amount: i64) -> Result<(), ProviderError> {
        self.cancel_at(provider, tx_id, amount, None).await
    }

    /// `refund` that only goes through while the gateway's cancellable balance is still `cancellable`
    /// (what our books say is left). A duplicate or a stale retry is refused instead of refunding twice.
    pub async fn refund_checked(&self, provider: &str, tx_id: &str, amount: i64, cancellable: i64) -> Result<(), ProviderError> {
        self.cancel_at(provider, tx_id, amount, Some(cancellable)).await
    }

    async fn cancel_at(&self, provider: &str, tx_id: &str, amount: i64, cancellable: Option<i64>) -> Result<(), ProviderError> {
        match (provider, toss_mid(provider)) {
            ("portone", _) if !self.dev() => {
                let cancelled = portone::cancel(self, tx_id, amount, cancellable).await;
                self.observe("portone", outcome(&cancelled)).await;
                cancelled
            }
            (_, Some(mid)) if !self.dev() => {
                let cancelled = toss_payments::cancel(self, mid, tx_id, amount, cancellable).await;
                self.observe(TOSS, outcome(&cancelled)).await;
                cancelled
            }
            _ => {
                self.require_dev("payments")?;
                tracing::info!(target: "fake_pg", provider, tx_id, amount, "refunded");
                Ok(())
            }
        }
    }

    /// Fetches a payment from the gateway `provider` names (a `payments.provider` or `checkouts.gateway` code).
    /// Dev mode fakes only `dev:` ids and checkout ids, priced at `dev_total`.
    pub async fn payment(&self, provider: &str, payment_id: &str, dev_total: i64) -> Result<PaymentInfo, ProviderError> {
        if self.dev() {
            return portone::dev_payment(payment_id, dev_total).ok_or(ProviderError::NotFound);
        }
        if let Some(mid) = toss_mid(provider) {
            let found = toss_payments::payment(self, mid, payment_id).await;
            self.observe(TOSS, outcome(&found)).await;
            return found;
        }
        let found = portone::payment(self, payment_id).await;
        self.observe("portone", outcome(&found)).await;
        found
    }

    /// Captures a Toss window payment with the amount we stored (`POST /v1/payments/confirm`). Dev mode: a
    /// `dev:failed` key declines, any other is paid at `amount`.
    pub async fn confirm_toss(&self, payment_key: &str, order_id: &str, amount: i64) -> Result<PaymentInfo, ProviderError> {
        if !self.dev() {
            let confirmed = toss_payments::confirm(self, payment_key, order_id, amount).await;
            self.observe(TOSS, outcome(&confirmed)).await;
            return confirmed;
        }
        if payment_key == "dev:failed" {
            return Err(ProviderError::Declined("dev_declined".into()));
        }
        portone::dev_payment("dev:card", amount).ok_or(ProviderError::NotFound)
    }

    /// Verifies a PortOne identity verification (PASS / 본인인증).
    pub async fn identity(&self, verification_id: &str) -> Result<IdentityInfo, ProviderError> {
        if !self.dev() {
            let verified = portone::identity(self, verification_id).await;
            self.observe("portone", outcome(&verified)).await;
            return verified;
        }
        portone::dev_identity(verification_id).ok_or(ProviderError::NotFound)
    }

    /// Provider for new payouts: `manual` unless the owner configured Toss in the setup wizard.
    pub fn payout_mode(&self) -> PayoutMode {
        self.integ().payout.as_ref().map_or(PayoutMode::Manual, |p| p.mode)
    }

    /// Registers a pro as a Toss seller. Returns (seller id, seller status). Encrypted request and response.
    pub async fn register_seller(&self, draft: &payout::SellerDraft) -> Result<(String, String), ProviderError> {
        if self.dev() {
            return Ok((format!("dev_seller_{}", draft.ref_id), "APPROVED".into()));
        }
        let (secret, key) = self.payout_keys()?;
        let req = payout::seller_request(&self.http, &secret, &key, draft)?;
        let body = self.payout_call(req, Some(&key)).await?;
        payout::parse_seller(&body)
    }

    /// Current Toss seller id and status (plain GET).
    pub async fn seller_status(&self, seller_id: &str) -> Result<(String, String), ProviderError> {
        if self.dev() {
            return Ok((seller_id.to_string(), "APPROVED".into()));
        }
        let (secret, key) = self.payout_keys()?;
        let req = payout::seller_status_request(&self.http, &secret, seller_id)?;
        let body = self.payout_call(req, Some(&key)).await?;
        payout::parse_seller(&body)
    }

    /// Sends a transfer through Toss to `seller_id` (the pro's Toss seller). `idempotency_key` must stay the
    /// same for every retry of one payout. Returns the provider's transfer id.
    pub async fn send_transfer(&self, idempotency_key: &str, ref_id: &str, seller_id: &str, t: &Transfer) -> Result<String, ProviderError> {
        if self.dev() {
            return Ok(format!("dev_{ref_id}"));
        }
        let (secret, key) = self.payout_keys()?;
        let req = payout::send_request(&self.http, &secret, &key, idempotency_key, ref_id, seller_id, t)?;
        let body = self.payout_call(req, Some(&key)).await?;
        payout::parse_sent(&body)
    }

    /// Current state of a Toss transfer, looked up by Toss's payout id (from `send_transfer`).
    pub async fn transfer_status(&self, provider_id: &str) -> Result<TransferStatus, ProviderError> {
        if self.dev() {
            return Ok(TransferStatus::Paid);
        }
        let (secret, key) = self.payout_keys()?;
        let req = payout::status_request(&self.http, &secret, provider_id)?;
        let body = self.payout_call(req, Some(&key)).await.map_err(payout::lookup_error)?;
        Ok(payout::parse_status(&body))
    }

    /// Toss payouts dated `since` or later whose refPayoutId is `ref_id`, as (Toss id, status). For a send that
    /// ended ambiguously, so Toss's id never came back. The dev fake keeps no record of such a send.
    pub async fn find_transfers(&self, ref_id: &str, since: chrono::NaiveDate) -> Result<Vec<(String, TransferStatus)>, ProviderError> {
        if self.dev() {
            return Ok(Vec::new());
        }
        let (secret, key) = self.payout_keys()?;
        let req = payout::list_request(&self.http, &secret, &since.to_string())?;
        let body = self.payout_call(req, Some(&key)).await.map_err(payout::lookup_error)?;
        payout::find_in_list(&body, ref_id)
    }

    /// Harmless read used by the console's sandbox connection test: `GET /v2/balances` with the given key.
    pub async fn payout_balance_check(&self, secret: &str) -> Result<StatusCode, ProviderError> {
        let req = self.toss_target(payout::balance_request(&self.http, secret)?)?;
        let resp = self.http.execute(req).await.map_err(|e| ProviderError::Upstream(e.to_string()))?;
        Ok(resp.status())
    }

    /// Secret (Basic auth) and security key (JWE AES key) from the live Toss settings.
    fn payout_keys(&self) -> Result<(String, [u8; 32]), ProviderError> {
        let p = self.integ().payout.clone().ok_or(ProviderError::NotConfigured("payout"))?;
        let secret = p.secret_key.ok_or(ProviderError::NotConfigured("payout"))?;
        let security = p.security_key.ok_or(ProviderError::NotConfigured("payout"))?;
        let key = toss_jwe::key_bytes(&security).map_err(|_| ProviderError::NotConfigured("payout"))?;
        Ok((secret, key))
    }

    /// Sends a Toss request to the base set by `with_toss_api` (a test mock), else leaves it on Toss.
    fn toss_target(&self, mut req: Request) -> Result<Request, ProviderError> {
        if let Some(base) = &self.toss_api {
            let tail = req.url().as_str().strip_prefix(payout::TOSS_API).unwrap_or_default();
            *req.url_mut() = reqwest::Url::parse(&format!("{base}{tail}")).map_err(|e| ProviderError::Upstream(e.to_string()))?;
        }
        Ok(req)
    }

    async fn payout_call(&self, req: Request, key: Option<&[u8; 32]>) -> Result<serde_json::Value, ProviderError> {
        let req = self.toss_target(req)?;
        let resp = self.http.execute(req).await.map_err(|e| ProviderError::Upstream(e.to_string()))?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| ProviderError::Upstream(e.to_string()))?;
        let body = payout::read_body(&text, key)?;
        if status.is_success() {
            Ok(body)
        } else {
            Err(payout::classify(status, &body))
        }
    }

    pub async fn push(&self, tokens: &[String], title: &str, body: &str, data: &Value) -> Result<(), ProviderError> {
        self.push_collect(tokens, title, body, data).await.map(|_| ())
    }

    /// Pushes to every token. Returns the tokens the push service reports as dead, for the caller to prune.
    pub async fn push_collect(&self, tokens: &[String], title: &str, body: &str, data: &Value) -> Result<Vec<String>, ProviderError> {
        if tokens.is_empty() {
            return Ok(Vec::new());
        }
        if self.dev() {
            tracing::info!(target: "fake_push", count = tokens.len(), title, body, %data, "push sent");
            return Ok(Vec::new());
        }
        let fcm = self.fcm().ok_or(ProviderError::NotConfigured("push"))?;
        let sent = fcm.send(&self.http, tokens, title, body, data).await;
        self.observe("push", outcome(&sent)).await;
        sent
    }
}
