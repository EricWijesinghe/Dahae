// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! The address port: keyword or postcode search, and reverse lookup for pin-first markets. KR shim: `juso.rs`.

use super::Fut;

pub struct AddressQuery {
    pub keyword: String,
    pub page: u32,
    /// Search by postcode first (GB, NL, JP).
    pub postcode_first: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LatLng {
    pub lat: f64,
    pub lng: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AddressHit {
    pub label: String,
    pub postcode: String,
    /// The pack's service-area code (KR: the five-digit 시군구 code).
    pub area_code: String,
    pub at: Option<LatLng>,
}

pub trait AddressLookup: Send + Sync {
    fn search(&self, q: AddressQuery) -> Fut<'_, Vec<AddressHit>>;
    /// Pin-first markets; most adapters have none.
    fn reverse(&self, _at: LatLng) -> Fut<'_, Option<AddressHit>> {
        super::payments::ready(Ok(None))
    }
}
