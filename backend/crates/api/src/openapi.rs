// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! `GET /v1/openapi.json`: the hand-maintained spec, served to staff admins only (it lists admin routes).
//! The served title carries the business name (`{name} API`); the file keeps the product name.

use crate::{auth::Admin, error::ApiResult, state::AppState};
use axum::{
    extract::State,
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
};
use serde_json::Value;

/// Source of truth, kept inside the crate so the Docker build context includes it.
const SPEC: &str = include_str!("../openapi.json");

/// The spec with `info.title` set to `{name} API`.
pub fn titled(name: &str) -> String {
    match serde_json::from_str::<Value>(SPEC) {
        Ok(mut v) => {
            v["info"]["title"] = Value::String(format!("{name} API"));
            v.to_string()
        }
        Err(_) => SPEC.to_owned(),
    }
}

pub async fn spec(State(s): State<AppState>, _admin: Admin) -> ApiResult<Response> {
    let body = titled(&crate::routes::setup::brand_name(&s.db).await);
    Ok(([(header::CONTENT_TYPE, HeaderValue::from_static("application/json"))], body).into_response())
}

#[cfg(test)]
mod tests {
    #[test]
    fn served_title_names_the_business() {
        let v: serde_json::Value = serde_json::from_str(&super::titled("반짝")).unwrap();
        assert_eq!(v["info"]["title"], "반짝 API");
        assert!(v["paths"].as_object().is_some_and(|p| !p.is_empty()), "the rest is unchanged");
    }
}
