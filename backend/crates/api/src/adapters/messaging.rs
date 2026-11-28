// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! The messaging port: transactional messages (SMS, AlimTalk, WhatsApp, LINE, Viber, email). Today's Solapi code
//! still runs through `Providers::send_customer_message`; its shim lands next.

use super::Fut;
use domain::money::Money;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelKind {
    Sms,
    AlimTalk,
    WhatsApp,
    Line,
    Viber,
    Email,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChannelCaps {
    pub kind: ChannelKind,
    /// Only templates the provider approved beforehand may be sent.
    pub preapproved_templates: bool,
    pub max_chars: Option<u32>,
    /// The sender id or number must be registered with the provider first.
    pub sender_registration: bool,
}

pub struct OutMsg {
    pub to_e164: String,
    pub channel: ChannelKind,
    /// Our template key (`providers::sms` TEMPLATE_KEYS).
    pub template_key: String,
    pub provider_template_id: Option<String>,
    pub vars: BTreeMap<String, String>,
    /// Sent as plain text when the template channel fails.
    pub fallback_text: String,
}

/// The provider's message id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SendRef(pub String);

pub trait Messenger: Send + Sync {
    fn channels(&self) -> &[ChannelCaps];
    fn send(&self, m: OutMsg) -> Fut<'_, SendRef>;
    /// Prepaid balance, when the provider reports one.
    fn balance(&self) -> Fut<'_, Option<Money>>;
}
