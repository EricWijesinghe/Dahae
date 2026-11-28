// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! `Messenger` over today's Solapi code (SMS and KakaoTalk AlimTalk). Every send goes through the same `Providers`
//! method the jobs use (dev logging, AlimTalk-then-SMS fallback, health records), so behaviour is unchanged.

use super::messaging::{ChannelCaps, ChannelKind, Messenger, OutMsg, SendRef};
use super::payments::ready;
use super::{AnyAdapter, Fut};
use crate::providers::{ProviderError, Providers};
use domain::money::Money;
use std::sync::Arc;

/// SMS (LMS beyond 45 characters, `providers::sms`) from the registered sender, and AlimTalk on approved templates.
const CHANNELS: [ChannelCaps; 2] = [
    ChannelCaps {
        kind: ChannelKind::Sms,
        preapproved_templates: false,
        max_chars: None,
        sender_registration: true,
    },
    ChannelCaps {
        kind: ChannelKind::AlimTalk,
        preapproved_templates: true,
        max_chars: None,
        sender_registration: true,
    },
];

pub struct SolapiMessenger {
    p: Providers,
}

pub fn build(p: &Providers, _code: &'static str) -> AnyAdapter {
    AnyAdapter::Msg(Arc::new(SolapiMessenger { p: p.clone() }))
}

/// Solapi takes domestic numbers in national form: `+821012345678` -> `01012345678`. Anything else is passed on
/// as given, as the callers pass it today.
pub fn national(to: &str) -> String {
    to.strip_prefix("+82").map_or_else(|| to.to_string(), |rest| format!("0{rest}"))
}

fn var<'a>(m: &'a OutMsg, name: &str) -> &'a str {
    m.vars.get(name).map_or("", String::as_str)
}

impl Messenger for SolapiMessenger {
    fn channels(&self) -> &[ChannelCaps] {
        &CHANNELS
    }

    /// SMS sends `fallback_text`. AlimTalk is `Providers::send_customer_message` for the template key, filled from
    /// the `brand`, `title` and `body` vars; it falls back to SMS itself. Solapi's message id is not read today.
    fn send(&self, m: OutMsg) -> Fut<'_, SendRef> {
        Box::pin(async move {
            let to = national(&m.to_e164);
            match m.channel {
                ChannelKind::Sms => self.p.send_sms(&to, &m.fallback_text).await,
                ChannelKind::AlimTalk => {
                    self.p
                        .send_customer_message(&to, &m.template_key, var(&m, "brand"), var(&m, "title"), var(&m, "body"))
                        .await
                }
                ChannelKind::WhatsApp | ChannelKind::Line | ChannelKind::Viber | ChannelKind::Email => Err(ProviderError::NotConfigured("solapi_channel")),
            }
            .map(|()| SendRef(String::new()))
        })
    }

    /// The balance is only read by the setup wizard's connection test, never as a number.
    fn balance(&self) -> Fut<'_, Option<Money>> {
        ready(Ok(None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn e164_numbers_go_to_solapi_in_national_form() {
        assert_eq!(national("+821012345678"), "01012345678");
        assert_eq!(national("+8221234567"), "021234567");
        assert_eq!(national("01012345678"), "01012345678", "a stored national number passes as today");
    }
}
