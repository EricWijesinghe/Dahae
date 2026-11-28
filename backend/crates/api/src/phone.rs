// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Phone numbers in E.164 (+821012345678), parsed with the install's pack region. Step one (migration 0507):
//! every writer stores the E.164 copy next to the phone it already stores, and a startup job fills the rows
//! written before. Step two: once [`reads_ready`] holds, login and OTP reads match the copy ([`lookup`]); writers
//! keep filling both columns, so the previous release, which reads only the old ones, still signs everyone in.

use phonenumber::{Mode, country};
use sqlx::PgPool;

/// The E.164 form of a number as entered or stored (`01012345678` in KR), or None when it is not a valid
/// number for `region` (the pack id, an ISO 3166 country code) or is a placeholder such as `deleted:<id>`.
pub fn e164(raw: &str, region: &str) -> Option<String> {
    let region: country::Id = region.parse().ok()?;
    let number = phonenumber::parse(Some(region), raw).ok()?;
    number.is_valid().then(|| number.format().mode(Mode::E164).to_string())
}

/// One stored phone column and the key the backfill pages by.
struct Column {
    table: &'static str,
    key: &'static str,
    key_type: &'static str,
    phone: &'static str,
}

/// Every phone column with an E.164 copy (`<phone>_e164`, migration 0507).
const COLUMNS: &[Column] = &[
    Column {
        table: "users",
        key: "id",
        key_type: "uuid",
        phone: "phone",
    },
    Column {
        table: "pros",
        key: "id",
        key_type: "uuid",
        phone: "phone",
    },
    Column {
        table: "team_members",
        key: "id",
        key_type: "uuid",
        phone: "phone",
    },
    Column {
        table: "otp_codes",
        key: "id",
        key_type: "uuid",
        phone: "phone",
    },
    Column {
        table: "referrals",
        key: "referred_user_id",
        key_type: "uuid",
        phone: "referred_phone",
    },
    Column {
        table: "notification_outbox",
        key: "id",
        key_type: "bigint",
        phone: "phone",
    },
    Column {
        table: "staff",
        key: "id",
        key_type: "uuid",
        phone: "phone",
    },
    Column {
        table: "staff",
        key: "id",
        key_type: "uuid",
        phone: "alert_phone",
    },
];

/// Rows read and written per statement by the startup backfill.
pub const BACKFILL_BATCH: i64 = 1000;

/// Fills the E.164 copy of every stored phone that has none: rows written before 0507, by an older release
/// after a rollback, or cleared because an older release changed the phone. Filled rows are skipped, so a rerun
/// changes nothing and a run cut off part way is finished by the next one. Each write checks that the phone is
/// still the one parsed and the copy still empty, so concurrent runs (several replicas booting) and live writers
/// never lose an update. Placeholders (empty, `deleted:<id>`, `hmac:<digest>`) are left without a copy.
/// Returns the number of rows filled.
/// shortcut: runs once per start, so a row an older replica writes after that waits for the next start. The read
/// switch ([`reads_ready`]) is decided once per start after this run; the previous release (1.6.6) writes both
/// columns, but a phone a replica older than 0507 writes while reads are switched is found only after the next start.
pub async fn backfill(db: &PgPool, region: &str, batch: i64) -> Result<u64, sqlx::Error> {
    let mut filled = 0;
    for column in COLUMNS {
        filled += scan_column(db, region, column, batch, true).await?;
    }
    Ok(filled)
}

/// Whether login reads may move to the E.164 copies. Both must hold:
/// - the backfill is complete: no stored phone that parses for `region` lacks its copy (one it could not write
///   because another row holds the number counts as missing);
/// - the previous release still signs in: no row has a copy without the phone that release reads.
///
/// A phone that does not parse has no copy and needs none: [`lookup`] matches it on the old column.
pub async fn reads_ready(db: &PgPool, region: &str) -> Result<bool, sqlx::Error> {
    for c in COLUMNS {
        if scan_column(db, region, c, BACKFILL_BATCH, false).await? > 0 {
            return Ok(false);
        }
        let (t, p) = (c.table, c.phone);
        let orphan = format!("SELECT EXISTS (SELECT 1 FROM {t} WHERE {p}_e164 IS NOT NULL AND ({p} IS NULL OR {p} = ''))");
        if sqlx::query_scalar::<_, bool>(&orphan).fetch_one(db).await? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The column a login or OTP read matches, and the value: the E.164 copy once reads switched and the number
/// parses for `region`, the stored phone otherwise. Both names exist on every table in [`COLUMNS`] with `phone`.
pub fn lookup(e164_reads: bool, phone: &str, region: &str) -> (&'static str, String) {
    match e164_reads.then(|| e164(phone, region)).flatten() {
        Some(e) => ("phone_e164", e),
        None => ("phone", phone.to_string()),
    }
}

/// The backfill's page query for one column: `$1` is the batch size and, when `after` is set, `$2` the last key
/// of the previous page. It sorts by the key itself (not its text form, where "10" < "2") and has no
/// parameter-dependent OR, so the primary key index serves both the order and the range, also in a generic plan.
fn page_sql(c: &Column, after: bool) -> String {
    let (t, k, kt, p) = (c.table, c.key, c.key_type, c.phone);
    let bound = if after { format!("AND {t}.{k} > $2::{kt}") } else { String::new() };
    format!(
        "SELECT {t}.{k}::text AS key, {p} FROM {t} WHERE {p}_e164 IS NULL AND {p} <> '' AND {p} NOT LIKE '%:%'
           {bound} ORDER BY {t}.{k} LIMIT $1"
    )
}

/// The page query `backfill` runs for `table`.`phone`, for plan checks in tests.
#[doc(hidden)]
pub fn backfill_page_sql(table: &str, phone: &str, after: bool) -> Option<String> {
    COLUMNS.iter().find(|c| c.table == table && c.phone == phone).map(|c| page_sql(c, after))
}

/// Pages through one column's rows without a copy. Writing, it fills them and returns how many; otherwise it
/// returns how many of the first page with any parse, without writing (0 means none is left to fill).
async fn scan_column(db: &PgPool, region: &str, c: &Column, batch: i64, write: bool) -> Result<u64, sqlx::Error> {
    let (t, k, kt, p) = (c.table, c.key, c.key_type, c.phone);
    let (first, next) = (page_sql(c, false), page_sql(c, true));
    let update = format!(
        "UPDATE {t} SET {p}_e164 = v.e FROM unnest($1::text[], $2::text[], $3::text[]) AS v(k, p, e)
         WHERE {t}.{k} = v.k::{kt} AND {t}.{p} = v.p AND {t}.{p}_e164 IS NULL"
    );
    let mut after: Option<String> = None;
    let mut filled = 0;
    loop {
        let rows: Vec<(String, String)> = match &after {
            None => sqlx::query_as(&first).bind(batch).fetch_all(db).await?,
            Some(a) => sqlx::query_as(&next).bind(batch).bind(a).fetch_all(db).await?,
        };
        let full = rows.len() as i64 == batch;
        let Some((last, _)) = rows.last() else { break };
        after = Some(last.clone());
        let parsed: Vec<(String, String, String)> = rows
            .into_iter()
            .filter_map(|(key, phone)| e164(&phone, region).map(|e| (key, phone, e)))
            .collect();
        if write {
            filled += write_batch(db, &update, &parsed, t).await?;
        } else if !parsed.is_empty() {
            return Ok(parsed.len() as u64);
        }
        if !full {
            break;
        }
    }
    Ok(filled)
}

/// One statement for the batch. If a copy would collide with another row's (a unique index), the batch is
/// written row by row instead and the colliding rows are left empty and logged, so one bad row never stops the boot.
async fn write_batch(db: &PgPool, update: &str, rows: &[(String, String, String)], table: &str) -> Result<u64, sqlx::Error> {
    match run_update(db, update, rows).await {
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            let mut filled = 0;
            for row in rows {
                match run_update(db, update, std::slice::from_ref(row)).await {
                    Ok(n) => filled += n,
                    Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
                        tracing::warn!(table, key = %row.0, "E.164 phone left empty: another row already holds the number");
                    }
                    Err(e) => return Err(e),
                }
            }
            Ok(filled)
        }
        other => other,
    }
}

async fn run_update(db: &PgPool, update: &str, rows: &[(String, String, String)]) -> Result<u64, sqlx::Error> {
    let keys: Vec<&str> = rows.iter().map(|r| r.0.as_str()).collect();
    let phones: Vec<&str> = rows.iter().map(|r| r.1.as_str()).collect();
    let e164s: Vec<&str> = rows.iter().map(|r| r.2.as_str()).collect();
    Ok(sqlx::query(update).bind(keys).bind(phones).bind(e164s).execute(db).await?.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::{e164, lookup};

    #[test]
    fn korean_mobiles_as_stored_become_e164() {
        for (stored, want) in [
            ("01012345678", "+821012345678"),
            ("0111234567", "+82111234567"),
            ("01612345678", "+821612345678"),
            ("01912345678", "+821912345678"),
            ("010-1234-5678", "+821012345678"),
            ("+82 10 1234 5678", "+821012345678"),
        ] {
            assert_eq!(e164(stored, "KR").as_deref(), Some(want), "{stored}");
        }
    }

    #[test]
    fn placeholders_and_garbage_have_no_e164() {
        for raw in [
            "",
            "deleted:0194e3a2-7b1c-7000-8000-000000000001",
            "hmac:abcdef0123",
            "123",
            "not a phone",
            "+dwPAA;phone-context=AA",
            ".;phone-context=",
        ] {
            assert_eq!(e164(raw, "KR"), None, "{raw}");
        }
    }

    #[test]
    fn reads_match_the_copy_only_once_switched_and_parsed() {
        assert_eq!(lookup(false, "01012345678", "KR"), ("phone", "01012345678".to_string()));
        assert_eq!(lookup(true, "01012345678", "KR"), ("phone_e164", "+821012345678".to_string()));
        assert_eq!(lookup(true, "123", "KR"), ("phone", "123".to_string()), "no copy to match: the old column");
    }

    #[test]
    fn region_comes_from_the_pack() {
        assert_eq!(e164("07400 123456", "GB").as_deref(), Some("+447400123456"));
        assert_eq!(e164("01012345678", "_base"), None, "a pack id that is no country parses nothing");
    }
}
