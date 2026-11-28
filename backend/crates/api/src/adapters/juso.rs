// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! `AddressLookup` over today's 도로명주소 search (`crate::address`): the same keyword rules, page bounds and
//! provider parsing as `GET /address/search`. The route keeps its rate limit and cache in front.

use super::address::{AddressHit, AddressLookup, AddressQuery};
use super::{AnyAdapter, Fut};
use crate::address::{self, Hit, Invalid};
use crate::providers::{ProviderError, Providers};
use std::sync::Arc;

pub struct Juso {
    p: Providers,
    base: String,
}

pub fn build(p: &Providers, _code: &'static str) -> AnyAdapter {
    AnyAdapter::Address(Arc::new(Juso::at(p, address::JUSO_API)))
}

impl Juso {
    /// Juso at another endpoint. Tests point it at a mock.
    pub fn at(p: &Providers, base: &str) -> Self {
        Self {
            p: p.clone(),
            base: base.to_string(),
        }
    }
}

/// Juso has a postcode but no coordinates; the area code is the five-digit 시군구 code.
fn hit(h: Hit) -> AddressHit {
    AddressHit {
        label: h.road_address,
        postcode: h.zip_code,
        area_code: h.district_code,
        at: None,
    }
}

impl AddressLookup for Juso {
    /// Juso searches postcodes and keywords alike, so `postcode_first` changes nothing.
    fn search(&self, q: AddressQuery) -> Fut<'_, Vec<AddressHit>> {
        Box::pin(async move {
            let key = self.p.integ().juso_confm_key.clone().ok_or(ProviderError::NotConfigured("juso"))?;
            let keyword = address::clean_keyword(&q.keyword).map_err(|Invalid(code)| ProviderError::Declined(code.into()))?;
            if !(1..=address::MAX_PAGE).contains(&q.page) {
                return Err(ProviderError::Declined("invalid_page".into()));
            }
            let hits = address::search_at(self.p.http(), &self.base, &key, &keyword, q.page).await?;
            Ok(hits.into_iter().map(hit).collect())
        })
    }
}
