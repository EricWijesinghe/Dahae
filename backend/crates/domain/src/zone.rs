// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! The install's time zone. Business dates (today, a visit day, a report day) and wall-clock hours
//! (a slot, quiet hours) are read in it. Pure: callers pass the zone and the instant. Zone rules come from
//! the IANA data bundled with `jiff`; instants and dates stay chrono types everywhere else.

use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Utc};
use jiff::{Timestamp, civil};
use std::sync::LazyLock;

/// A named IANA time zone.
pub type Tz = jiff::tz::TimeZone;

/// Name of the zone of an install that has not chosen one, so every install from before time zones behaves
/// exactly as it did.
pub const DEFAULT_NAME: &str = "Asia/Seoul";

static DEFAULT: LazyLock<Tz> = LazyLock::new(|| get(DEFAULT_NAME).expect("the bundled time zone data has the default zone"));

/// The default zone ([`DEFAULT_NAME`]).
pub fn default_tz() -> Tz {
    DEFAULT.clone()
}

/// The zone with this IANA name (`Europe/London`), or `None` when the bundled data has no such zone.
pub fn get(name: &str) -> Option<Tz> {
    Tz::get(name).ok()
}

/// The IANA name of a zone from [`get`] (`dahae.tz` on every pool connection).
pub fn name(tz: &Tz) -> &str {
    tz.iana_name().unwrap_or(DEFAULT_NAME)
}

// shortcut: instants outside jiff's range (years beyond +-9999) saturate; no business date gets near them.
fn timestamp(t: DateTime<Utc>) -> Timestamp {
    let nanos = i32::try_from(t.timestamp_subsec_nanos()).unwrap_or(0);
    Timestamp::new(t.timestamp(), nanos).unwrap_or(if t.timestamp() < 0 { Timestamp::MIN } else { Timestamp::MAX })
}

/// Wall-clock time in `tz` at `now`.
pub fn local(tz: &Tz, now: DateTime<Utc>) -> NaiveDateTime {
    now.naive_utc() + Duration::seconds(i64::from(tz.to_offset(timestamp(now)).seconds()))
}

/// Calendar date in `tz` at `now`.
pub fn today(tz: &Tz, now: DateTime<Utc>) -> NaiveDate {
    local(tz, now).date()
}

fn to_civil(local: NaiveDateTime) -> Option<civil::DateTime> {
    let small = |v: u32| i8::try_from(v).ok();
    let nanos = i32::try_from(local.nanosecond() % 1_000_000_000).ok()?;
    let year = i16::try_from(local.year()).ok()?;
    civil::DateTime::new(
        year,
        small(local.month())?,
        small(local.day())?,
        small(local.hour())?,
        small(local.minute())?,
        small(local.second())?,
        nanos,
    )
    .ok()
}

/// UTC instant of a wall-clock time in `tz`. In a daylight-saving overlap the earlier instant wins; a time skipped
/// by a daylight-saving gap moves forward by the gap (jiff's "compatible" rule, as in RFC 5545).
pub fn to_utc(tz: &Tz, local: NaiveDateTime) -> DateTime<Utc> {
    to_civil(local)
        .and_then(|c| tz.to_ambiguous_timestamp(c).compatible().ok())
        .and_then(|ts| DateTime::from_timestamp(ts.as_second(), u32::try_from(ts.subsec_nanosecond()).ok()?))
        // shortcut: a wall-clock time outside jiff's range (years beyond +-9999) is read as UTC; none is reachable.
        .unwrap_or_else(|| local.and_utc())
}

/// The UTC range `[start, end)` of one calendar day in `tz` (23 or 25 hours long on a daylight-saving change).
pub fn day_bounds(tz: &Tz, day: NaiveDate) -> (DateTime<Utc>, DateTime<Utc>) {
    let start = to_utc(tz, day.and_time(NaiveTime::MIN));
    (start, to_utc(tz, (day + Duration::days(1)).and_time(NaiveTime::MIN)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).expect("rfc3339").with_timezone(&Utc)
    }

    fn wall(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").expect("local")
    }

    fn london() -> Tz {
        get("Europe/London").expect("bundled")
    }

    #[test]
    fn default_zone_is_utc_plus_nine_all_year() {
        let tz = default_tz();
        assert_eq!(name(&tz), DEFAULT_NAME);
        for at in ["2026-01-15T14:59:59Z", "2026-07-15T14:59:59Z", "2026-03-29T01:30:00Z", "2026-10-25T01:30:00Z"] {
            let t = utc(at);
            assert_eq!(local(&tz, t), t.naive_utc() + Duration::hours(9), "{at}");
            assert_eq!(to_utc(&tz, local(&tz, t)), t, "{at}");
            assert_eq!(
                today(&tz, t + Duration::seconds(1)),
                (t + Duration::hours(9) + Duration::seconds(1)).date_naive()
            );
        }
    }

    #[test]
    fn london_follows_daylight_saving() {
        let tz = london();
        assert_eq!(name(&tz), "Europe/London");
        assert_eq!(today(&tz, utc("2026-07-15T23:30:00Z")).to_string(), "2026-07-16", "BST is UTC+1");
        assert_eq!(today(&tz, utc("2026-12-15T23:30:00Z")).to_string(), "2026-12-15", "GMT is UTC+0");
        assert_eq!(to_utc(&tz, wall("2026-07-15 10:00")), utc("2026-07-15T09:00:00Z"));
        // 2026-03-29 01:30 does not exist (clocks go 01:00 -> 02:00): it lands at 02:30 BST.
        assert_eq!(to_utc(&tz, wall("2026-03-29 01:30")), utc("2026-03-29T01:30:00Z"));
        // 2026-10-25 01:30 happens twice: the first (BST) one.
        assert_eq!(to_utc(&tz, wall("2026-10-25 01:30")), utc("2026-10-25T00:30:00Z"));
        let (start, end) = day_bounds(&tz, NaiveDate::from_ymd_opt(2026, 3, 29).expect("date"));
        assert_eq!((start, end - start), (utc("2026-03-29T00:00:00Z"), Duration::hours(23)));
    }

    #[test]
    fn default_day_bounds_start_at_fifteen_utc_the_day_before() {
        let (start, end) = day_bounds(&default_tz(), NaiveDate::from_ymd_opt(2026, 11, 16).expect("date"));
        assert_eq!((start, end), (utc("2026-11-15T15:00:00Z"), utc("2026-11-16T15:00:00Z")));
    }

    #[test]
    fn unknown_names_are_refused() {
        assert!(get("Mars/Olympus").is_none());
        assert!(get("").is_none());
    }
}
