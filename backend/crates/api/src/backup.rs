// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Offsite backups. `api backup-upload <file>` (run by `malkkeum backup` after the local dump)
//! records the backup status, encrypts the file with a key derived from DATA_KEY, uploads it to
//! the S3-compatible bucket from the console's `backup` card, and applies the retention window.
//! `api backup-fetch` / `api backup-list` serve `malkkeum restore-offsite`. Nothing is exposed over HTTP.
//!
//! Encrypted file format: "MKB1" | 8-byte random nonce prefix | frames of
//! [flag u8 (1 = last)][len u32 BE][AES-256-GCM ciphertext+tag], 64 KiB plaintext per frame.
//! Nonce = prefix || frame index; the flag is authenticated, so truncation and reordering fail.

use crate::entropy;
use crate::{
    config::Config,
    integrations::{Integrations, S3Config},
    ops,
};
use aes_gcm::{
    Aes256Gcm, Key, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use chrono::{DateTime, Duration, Utc};
use futures_util::StreamExt;
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, postgres::PgPoolOptions};
use std::{
    io::{self, Read, Write},
    path::{Path, PathBuf},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const MAGIC: &[u8; 4] = b"MKB1";
const CHUNK: usize = 64 * 1024;
const TAG_LEN: usize = 16;
const PREFIX_OBJECT: &str = "malkkeum/";
const SUFFIX: &str = ".enc";
const CONNECTION_TEST_OBJECT: &str = "malkkeum/connection-test.txt";
const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const KEY_CONTEXT: &[u8] = b"malkkeum offsite backup v1";
/// A single S3 PUT stops at 5 GiB; bigger archives (the photo tar) go up as multipart parts.
const SINGLE_PUT_MAX: u64 = 5 * 1024 * 1024 * 1024;
const MIN_PART: u64 = 64 * 1024 * 1024;
const MAX_PARTS: u64 = 10_000;
/// EC2 instance metadata service (IMDSv2). The AWS template sets the hop limit to 2 so the API container reaches it.
pub const IMDS_BASE: &str = "http://169.254.169.254";
const IMDS_TOKEN_TTL_SECS: &str = "21600";
const IMDS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
/// Role credentials are fetched again this long before they expire: a photo upload in parts can outlast them.
const ROLE_REFRESH_MARGIN_SECS: i64 = 300;

/// Temporary credentials of the server's instance role.
#[derive(Clone, Debug, PartialEq, Eq)]
struct RoleCreds {
    access: String,
    secret: String,
    token: String,
    expires: DateTime<Utc>,
}

/// Role credentials by the metadata service they came from (one entry on a server; tests run fakes side by side).
static ROLE_CACHE: std::sync::Mutex<Vec<(String, RoleCreds)>> = std::sync::Mutex::new(Vec::new());

/// Cached role credentials, fetched again from `base` when none are cached or they expire within the margin.
async fn role_creds(http: &reqwest::Client, base: &str) -> Result<RoleCreds, Fail> {
    let fresh_until = Utc::now() + Duration::seconds(ROLE_REFRESH_MARGIN_SECS);
    let cached = ROLE_CACHE
        .lock()
        .ok()
        .and_then(|c| c.iter().find(|(b, c)| b == base && c.expires > fresh_until).map(|(_, c)| c.clone()));
    if let Some(c) = cached {
        return Ok(c);
    }
    let creds = fetch_role_creds(http, base).await?;
    if let Ok(mut c) = ROLE_CACHE.lock() {
        c.retain(|(b, _)| b != base);
        c.push((base.to_string(), creds.clone()));
    }
    Ok(creds)
}

/// IMDSv2: a session token first (PUT), then the role name and its credentials with that token.
async fn fetch_role_creds(http: &reqwest::Client, base: &str) -> Result<RoleCreds, Fail> {
    let token = imds_text(
        http.put(format!("{base}/latest/api/token"))
            .header("x-aws-ec2-metadata-token-ttl-seconds", IMDS_TOKEN_TTL_SECS),
    )
    .await?;
    let path = format!("{base}/latest/meta-data/iam/security-credentials/");
    let roles = imds_text(http.get(&path).header("x-aws-ec2-metadata-token", &token)).await?;
    let role = roles
        .lines()
        .next()
        .map(str::trim)
        .filter(|r| !r.is_empty() && r.chars().all(|c| c.is_ascii_alphanumeric() || "+=,.@_-".contains(c)))
        .ok_or(Fail::NoRole)?;
    let body = imds_text(http.get(format!("{path}{role}")).header("x-aws-ec2-metadata-token", &token)).await?;
    parse_role_creds(&body).ok_or(Fail::NoRole)
}

async fn imds_text(req: reqwest::RequestBuilder) -> Result<String, Fail> {
    let resp = req.timeout(IMDS_TIMEOUT).send().await.map_err(|_| Fail::NoRole)?;
    if !resp.status().is_success() {
        return Err(Fail::NoRole);
    }
    resp.text().await.map_err(|_| Fail::NoRole)
}

/// The metadata service's credentials document; anything but `"Code": "Success"` with all four fields is refused.
fn parse_role_creds(body: &str) -> Option<RoleCreds> {
    let v: Value = serde_json::from_str(body).ok()?;
    let field = |k: &str| v.get(k).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_owned);
    if v.get("Code").and_then(Value::as_str) != Some("Success") {
        return None;
    }
    Some(RoleCreds {
        access: field("AccessKeyId")?,
        secret: field("SecretAccessKey")?,
        token: field("Token")?,
        expires: DateTime::parse_from_rfc3339(&field("Expiration")?).ok()?.with_timezone(&Utc),
    })
}

/// Why a remote call failed. `code()` is what the status page and the wizard show.
#[derive(Debug, thiserror::Error)]
pub enum Fail {
    #[error("backup target unreachable")]
    Unreachable,
    #[error("backup target answered {0}")]
    Rejected(u16),
    #[error("local i/o: {0}")]
    Io(#[from] io::Error),
    #[error("backup file damaged or wrong key")]
    Corrupt,
    #[error("invalid listing from backup target")]
    BadListing,
    #[error("backup target did not finish the multipart upload")]
    Incomplete,
    #[error("no instance role credentials from the metadata service")]
    NoRole,
}

impl Fail {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoRole => "no_instance_role",
            Self::Unreachable => "unreachable",
            Self::Rejected(401 | 403) => "invalid_key",
            Self::Rejected(_) | Self::Incomplete => "provider_error",
            Self::Io(_) | Self::Corrupt | Self::BadListing => "backup_failed",
        }
    }
}

/// Key for the backup files, derived from DATA_KEY so the owner keeps one secret to protect.
pub fn file_key(data_key: &[u8; 32]) -> [u8; 32] {
    let mut mac = <Hmac<Sha256> as hmac::KeyInit>::new_from_slice(data_key).expect("HMAC takes any key length");
    mac.update(KEY_CONTEXT);
    mac.finalize().into_bytes().into()
}

fn frame_nonce(prefix: &[u8; 8], index: u32) -> Nonce<aes_gcm::aead::consts::U12> {
    let mut n = [0u8; 12];
    n[..8].copy_from_slice(prefix);
    n[8..].copy_from_slice(&index.to_be_bytes());
    Nonce::from(n)
}

fn corrupt() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "backup file damaged or wrong key")
}

/// Reads up to one chunk; a short result means end of input.
fn read_chunk<R: Read>(input: &mut R) -> io::Result<Vec<u8>> {
    let mut buf = vec![0u8; CHUNK];
    let mut filled = 0;
    while filled < CHUNK {
        let n = input.read(&mut buf[filled..])?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    buf.truncate(filled);
    Ok(buf)
}

/// Streams `input` into the encrypted format. Each frame is written once the next one is read,
/// so the final frame (and only it) carries the last flag.
pub fn encrypt_stream<R: Read, W: Write>(key: &[u8; 32], mut input: R, mut output: W) -> io::Result<()> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let mut prefix = [0u8; 8];
    entropy::fill(&mut prefix);
    output.write_all(MAGIC)?;
    output.write_all(&prefix)?;
    let mut current = read_chunk(&mut input)?;
    let mut index: u32 = 0;
    loop {
        let next = read_chunk(&mut input)?;
        let last = next.is_empty();
        let flag = [u8::from(last)];
        let ct = cipher
            .encrypt(&frame_nonce(&prefix, index), Payload { msg: &current, aad: &flag })
            .map_err(|_| io::Error::other("encryption failed"))?;
        output.write_all(&flag)?;
        output.write_all(&(ct.len() as u32).to_be_bytes())?;
        output.write_all(&ct)?;
        if last {
            return Ok(());
        }
        current = next;
        index = index.checked_add(1).ok_or_else(|| io::Error::other("backup too large"))?;
    }
}

/// Reverses [`encrypt_stream`]. Fails on a wrong key, any changed byte, reordered frames, or a missing last frame.
pub fn decrypt_stream<R: Read, W: Write>(key: &[u8; 32], mut input: R, mut output: W) -> io::Result<()> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let mut head = [0u8; 12];
    input.read_exact(&mut head)?;
    if &head[..4] != MAGIC {
        return Err(corrupt());
    }
    let prefix: [u8; 8] = head[4..].try_into().map_err(|_| corrupt())?;
    let mut index: u32 = 0;
    loop {
        let mut frame = [0u8; 5];
        input.read_exact(&mut frame)?;
        let flag = frame[0];
        let len = u32::from_be_bytes([frame[1], frame[2], frame[3], frame[4]]) as usize;
        if flag > 1 || len > CHUNK + TAG_LEN {
            return Err(corrupt());
        }
        let mut ct = vec![0u8; len];
        input.read_exact(&mut ct)?;
        let plain = cipher
            .decrypt(&frame_nonce(&prefix, index), Payload { msg: &ct, aad: &[flag] })
            .map_err(|_| corrupt())?;
        output.write_all(&plain)?;
        if flag == 1 {
            let mut probe = [0u8; 1];
            return if input.read(&mut probe)? == 0 { Ok(()) } else { Err(corrupt()) };
        }
        index = index.checked_add(1).ok_or_else(corrupt)?;
    }
}

fn hmac256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    let mut mac = <Hmac<Sha256> as hmac::KeyInit>::new_from_slice(key).expect("HMAC takes any key length");
    mac.update(msg);
    mac.finalize().into_bytes().into()
}

fn hex(bytes: impl AsRef<[u8]>) -> String {
    bytes.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

/// AWS Signature V4 signing key for one date, region and service.
pub fn signing_key(secret: &str, date: &str, region: &str, service: &str) -> [u8; 32] {
    let k = hmac256(format!("AWS4{secret}").as_bytes(), date.as_bytes());
    let k = hmac256(&k, region.as_bytes());
    let k = hmac256(&k, service.as_bytes());
    hmac256(&k, b"aws4_request")
}

/// Credentials and scope for one signed request.
pub struct Sigv4<'a> {
    pub access: &'a str,
    pub secret: &'a str,
    pub region: &'a str,
    pub service: &'a str,
}

/// Builds the `Authorization` header value. `headers` are (lowercase name, value), sorted by name.
pub fn authorization(c: &Sigv4, method: &str, canonical_uri: &str, query: &str, headers: &[(&str, &str)], payload_hash: &str, amz_date: &str) -> String {
    let date = &amz_date[..8];
    let signed: Vec<&str> = headers.iter().map(|(k, _)| *k).collect();
    let signed_list = signed.join(";");
    let canon_headers: String = headers.iter().map(|(k, v)| format!("{k}:{}\n", v.trim())).collect();
    let canonical = format!("{method}\n{canonical_uri}\n{query}\n{canon_headers}\n{signed_list}\n{payload_hash}");
    let scope = format!("{date}/{}/{}/aws4_request", c.region, c.service);
    let to_sign = format!("AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}", hex(Sha256::digest(canonical.as_bytes())));
    let key = signing_key(c.secret, date, c.region, c.service);
    let sig = hex(hmac256(&key, to_sign.as_bytes()));
    format!("AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_list}, Signature={sig}", c.access)
}

/// RFC 3986 encoding of a path, keeping '/'.
fn encode_path(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Host part of the endpoint, for the Host header and the signature.
fn host_of(endpoint: &str) -> &str {
    endpoint.trim_start_matches("https://").split('/').next().unwrap_or_default()
}

/// Path-style request to the bucket. `name` is an object key, or empty for the bucket itself.
struct Req<'a> {
    cfg: &'a S3Config,
    method: &'static str,
    name: &'a str,
    query: &'a str,
    payload_hash: &'a str,
    body: Option<(reqwest::Body, u64)>,
}

async fn send(http: &reqwest::Client, r: Req<'_>) -> Result<reqwest::Response, Fail> {
    let cfg = r.cfg;
    let path = if r.name.is_empty() {
        format!("/{}", cfg.bucket)
    } else {
        format!("/{}/{}", cfg.bucket, r.name)
    };
    let canonical_uri = encode_path(&path);
    let amz_date = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let host = host_of(&cfg.endpoint).to_string();
    let length = r.body.as_ref().map(|(_, n)| n.to_string());
    let role = match &cfg.instance_role {
        Some(base) => Some(role_creds(http, base).await?),
        None => None,
    };
    let (access, secret) = role
        .as_ref()
        .map_or((cfg.access_key.as_str(), cfg.secret_key.as_str()), |c| (c.access.as_str(), c.secret.as_str()));
    let mut headers: Vec<(&str, &str)> = vec![
        ("host", host.as_str()),
        ("x-amz-content-sha256", r.payload_hash),
        ("x-amz-date", amz_date.as_str()),
    ];
    if let Some(len) = &length {
        headers.push(("content-length", len.as_str()));
    }
    // Temporary role credentials only work with their session token, signed like the other headers.
    if let Some(c) = &role {
        headers.push(("x-amz-security-token", c.token.as_str()));
    }
    headers.sort_by(|a, b| a.0.cmp(b.0));
    let auth = authorization(
        &Sigv4 {
            access,
            secret,
            region: &cfg.region,
            service: "s3",
        },
        r.method,
        &canonical_uri,
        r.query,
        &headers,
        r.payload_hash,
        &amz_date,
    );
    let url = if r.query.is_empty() {
        format!("{}{}", cfg.endpoint, canonical_uri)
    } else {
        format!("{}{}?{}", cfg.endpoint, canonical_uri, r.query)
    };
    let method = reqwest::Method::from_bytes(r.method.as_bytes()).map_err(|_| Fail::Corrupt)?;
    let mut req = http
        .request(method, url)
        .header("x-amz-date", &amz_date)
        .header("x-amz-content-sha256", r.payload_hash)
        .header("authorization", auth);
    if let Some(c) = &role {
        req = req.header("x-amz-security-token", &c.token);
    }
    if let Some((body, n)) = r.body {
        req = req.header("content-length", n.to_string()).body(body);
    }
    let resp = req.send().await.map_err(|_| Fail::Unreachable)?;
    if resp.status().is_success() {
        Ok(resp)
    } else {
        Err(Fail::Rejected(resp.status().as_u16()))
    }
}

/// Hashes a local file (streamed) and returns (hex sha256, length).
async fn hash_file(path: PathBuf) -> Result<(String, u64), Fail> {
    tokio::task::spawn_blocking(move || {
        let mut f = std::fs::File::open(&path)?;
        let len = f.metadata()?.len();
        let mut h = Sha256::new();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = io::Read::read(&mut f, &mut buf)?;
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
        }
        Ok((hex(h.finalize()), len))
    })
    .await
    .map_err(|_| Fail::Corrupt)?
}

fn needs_multipart(len: u64) -> bool {
    len > SINGLE_PUT_MAX
}

/// Part size for a multipart upload: at least 64 MiB, and small enough that the part count stays under 10 000.
fn part_size(len: u64) -> u64 {
    MIN_PART.max(len.div_ceil(MAX_PARTS))
}

/// Percent-encodes a query value (upload ids can hold characters that need escaping).
fn encode_query(s: &str) -> String {
    encode_path(s).replace('/', "%2F")
}

/// `CompleteMultipartUpload` body listing every part's number and ETag.
fn complete_xml(parts: &[(u32, String)]) -> String {
    let list: String = parts
        .iter()
        .map(|(n, etag)| format!("<Part><PartNumber>{n}</PartNumber><ETag>{etag}</ETag></Part>"))
        .collect();
    format!("<CompleteMultipartUpload>{list}</CompleteMultipartUpload>")
}

/// Uploads a local file as object `name`. Archives over 5 GiB go up in parts; a failed upload is aborted.
pub async fn put_file(http: &reqwest::Client, cfg: &S3Config, name: &str, path: &Path) -> Result<(), Fail> {
    let (hash, len) = hash_file(path.to_path_buf()).await?;
    if needs_multipart(len) {
        return put_multipart(http, cfg, name, path, len).await;
    }
    let file = tokio::fs::File::open(path).await?;
    send(
        http,
        Req {
            cfg,
            method: "PUT",
            name,
            query: "",
            payload_hash: &hash,
            body: Some((reqwest::Body::from(file), len)),
        },
    )
    .await?;
    Ok(())
}

/// Multipart upload: start, one PUT per part (each hashed as it is read), then complete. Any failure aborts
/// the upload so the target does not keep orphan parts.
async fn put_multipart(http: &reqwest::Client, cfg: &S3Config, name: &str, path: &Path, len: u64) -> Result<(), Fail> {
    let started = send(
        http,
        Req {
            cfg,
            method: "POST",
            name,
            query: "uploads=",
            payload_hash: EMPTY_SHA256,
            body: None,
        },
    )
    .await?;
    let init = started.text().await.map_err(|_| Fail::Unreachable)?;
    let id = tag(&init, "UploadId").ok_or(Fail::BadListing)?.to_string();
    match upload_parts(http, cfg, name, path, len, &id).await {
        Ok(parts) => finish_multipart(http, cfg, name, &id, &parts).await,
        Err(e) => {
            let _ = abort_multipart(http, cfg, name, &id).await;
            Err(e)
        }
    }
}

async fn upload_parts(http: &reqwest::Client, cfg: &S3Config, name: &str, path: &Path, len: u64, id: &str) -> Result<Vec<(u32, String)>, Fail> {
    let mut file = tokio::fs::File::open(path).await?;
    let size = part_size(len);
    let mut left = len;
    let mut number: u32 = 1;
    let mut parts = Vec::new();
    while left > 0 {
        let take = left.min(size);
        let mut buf = vec![0u8; take as usize];
        file.read_exact(&mut buf).await?;
        let hash = hex(Sha256::digest(&buf));
        let query = format!("partNumber={number}&uploadId={}", encode_query(id));
        let resp = send(
            http,
            Req {
                cfg,
                method: "PUT",
                name,
                query: &query,
                payload_hash: &hash,
                body: Some((reqwest::Body::from(buf), take)),
            },
        )
        .await?;
        let etag = resp.headers().get("etag").and_then(|v| v.to_str().ok()).ok_or(Fail::Incomplete)?;
        parts.push((number, etag.to_string()));
        left -= take;
        number += 1;
    }
    Ok(parts)
}

async fn finish_multipart(http: &reqwest::Client, cfg: &S3Config, name: &str, id: &str, parts: &[(u32, String)]) -> Result<(), Fail> {
    let body = complete_xml(parts);
    let hash = hex(Sha256::digest(body.as_bytes()));
    let query = format!("uploadId={}", encode_query(id));
    let resp = send(
        http,
        Req {
            cfg,
            method: "POST",
            name,
            query: &query,
            payload_hash: &hash,
            body: Some((reqwest::Body::from(body.clone()), body.len() as u64)),
        },
    )
    .await?;
    // S3 can answer a failed completion with 200 and an <Error> body.
    let answer = resp.text().await.map_err(|_| Fail::Unreachable)?;
    if answer.contains("<Error>") { Err(Fail::Incomplete) } else { Ok(()) }
}

async fn abort_multipart(http: &reqwest::Client, cfg: &S3Config, name: &str, id: &str) -> Result<(), Fail> {
    let query = format!("uploadId={}", encode_query(id));
    send(
        http,
        Req {
            cfg,
            method: "DELETE",
            name,
            query: &query,
            payload_hash: EMPTY_SHA256,
            body: None,
        },
    )
    .await?;
    Ok(())
}

/// Downloads object `name` into the local file `out`.
pub async fn get_file(http: &reqwest::Client, cfg: &S3Config, name: &str, out: &Path) -> Result<(), Fail> {
    let resp = send(
        http,
        Req {
            cfg,
            method: "GET",
            name,
            query: "",
            payload_hash: EMPTY_SHA256,
            body: None,
        },
    )
    .await?;
    let mut file = tokio::fs::File::create(out).await?;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        file.write_all(&chunk.map_err(|_| Fail::Unreachable)?).await?;
    }
    file.flush().await?;
    Ok(())
}

async fn delete_object(http: &reqwest::Client, cfg: &S3Config, name: &str) -> Result<(), Fail> {
    send(
        http,
        Req {
            cfg,
            method: "DELETE",
            name,
            query: "",
            payload_hash: EMPTY_SHA256,
            body: None,
        },
    )
    .await?;
    Ok(())
}

/// Lists object keys under our prefix (first page, up to 1000 objects).
pub async fn list_objects(http: &reqwest::Client, cfg: &S3Config) -> Result<Vec<(String, DateTime<Utc>)>, Fail> {
    let resp = send(
        http,
        Req {
            cfg,
            method: "GET",
            name: "",
            query: "list-type=2&prefix=malkkeum%2F",
            payload_hash: EMPTY_SHA256,
            body: None,
        },
    )
    .await?;
    let xml = resp.text().await.map_err(|_| Fail::Unreachable)?;
    parse_listing(&xml).ok_or(Fail::BadListing)
}

fn tag<'a>(block: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{name}>");
    let start = block.find(&open)? + open.len();
    let end = block[start..].find(&format!("</{name}>"))? + start;
    Some(&block[start..end])
}

/// Reads `<Contents>` blocks of a ListObjectsV2 response.
pub fn parse_listing(xml: &str) -> Option<Vec<(String, DateTime<Utc>)>> {
    let mut out = Vec::new();
    for block in xml.split("<Contents>").skip(1) {
        let key = tag(block, "Key")?.to_string();
        let modified = tag(block, "LastModified")?;
        let at = DateTime::parse_from_rfc3339(modified).ok()?.with_timezone(&Utc);
        out.push((key, at));
    }
    Some(out)
}

/// Object keys older than the retention window. Only our own `.enc` backups are ever selected.
pub fn expired(objects: &[(String, DateTime<Utc>)], now: DateTime<Utc>, retention_days: u32) -> Vec<String> {
    let cutoff = now - Duration::days(i64::from(retention_days));
    objects
        .iter()
        .filter(|(key, at)| key.starts_with(PREFIX_OBJECT) && key.ends_with(SUFFIX) && *at < cutoff)
        .map(|(key, _)| key.clone())
        .collect()
}

/// Connection test for the wizard: writes and removes a tiny object. Proves the keys and bucket work.
/// The instance role cannot delete, so its bucket is proven with a listing instead.
pub async fn check(http: &reqwest::Client, cfg: &S3Config) -> Result<(), Fail> {
    if cfg.instance_role.is_some() {
        return list_objects(http, cfg).await.map(|_| ());
    }
    let body = b"malkkeum connection test".to_vec();
    let hash = hex(Sha256::digest(&body));
    let len = body.len() as u64;
    send(
        http,
        Req {
            cfg,
            method: "PUT",
            name: CONNECTION_TEST_OBJECT,
            query: "",
            payload_hash: &hash,
            body: Some((reqwest::Body::from(body), len)),
        },
    )
    .await?;
    delete_object(http, cfg, CONNECTION_TEST_OBJECT).await
}

/// Stores the last backup status in `settings` (read by the status page and the alert rules).
pub async fn store_status(db: &PgPool, status: &Value) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES ($1, $2)
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
    )
    .bind(ops::BACKUP_STATUS_KEY)
    .bind(status)
    .execute(db)
    .await
    .map(|_| ())
}

/// Encrypts `file`, uploads it as `malkkeum/<name>.enc`, then applies retention. Returns Ok when all of it worked.
async fn offsite(http: &reqwest::Client, cfg: &S3Config, data_key: &[u8; 32], file: &Path) -> Result<(), Fail> {
    let name = format!("{PREFIX_OBJECT}{}{SUFFIX}", file.file_name().and_then(|n| n.to_str()).unwrap_or("backup"));
    let tmp = std::env::temp_dir().join(format!("malkkeum-{}.enc", std::process::id()));
    let key = file_key(data_key);
    let src = std::fs::File::open(file)?;
    let dst = std::fs::File::create(&tmp)?;
    let result: Result<(), Fail> = async {
        let mut sealed = io::BufWriter::new(dst);
        encrypt_stream(&key, io::BufReader::new(src), &mut sealed)?;
        sealed.flush()?;
        put_file(http, cfg, &name, &tmp).await?;
        // The instance role has no delete: its bucket's lifecycle expires old copies.
        if cfg.instance_role.is_some() {
            return Ok(());
        }
        let objects = list_objects(http, cfg).await?;
        for old in expired(&objects, Utc::now(), cfg.retention_days) {
            delete_object(http, cfg, &old).await?;
        }
        Ok(())
    }
    .await;
    let _ = std::fs::remove_file(&tmp);
    result
}

async fn open(config: &Config) -> Result<(PgPool, Integrations), String> {
    let db = PgPoolOptions::new()
        .max_connections(2)
        .connect(&config.database_url)
        .await
        .map_err(|e| e.to_string())?;
    let integ = crate::integrations::load(&db, config).await.map_err(|e| e.to_string())?;
    Ok((db, integ))
}

/// Ops record key for an offsite upload: the photo tar and the database dump are recorded apart.
fn offsite_integration(name: &str) -> &'static str {
    if name.starts_with("uploads-") { ops::PHOTOS_INTEGRATION } else { "backup" }
}

/// `api backup-upload <file>`: the cron job's second step. Local status is recorded first, so a
/// failing offsite target never hides a good local dump.
async fn upload_cmd(config: &Config, file: &str) -> i32 {
    let path = Path::new(file);
    let Ok(meta) = std::fs::metadata(path) else {
        eprintln!("backup file not found: {file}");
        return 1;
    };
    let (db, integ) = match open(config).await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("database: {e}");
            return 1;
        }
    };
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or(file);
    // The photo tar is recorded under its own key: its failure must not be overwritten by the database upload.
    let photos = offsite_integration(name) == ops::PHOTOS_INTEGRATION;
    let prior = match ops::backup_status(&db).await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("status: {e}");
            return 1;
        }
    };
    let now = Utc::now();
    let unset = integ.backup.is_none();
    let mut status = json!({
        "at": now, "file": name, "size_bytes": meta.len(), "offsite": null,
        "offsite_unset_since": ops::offsite_unset_since(prior.as_ref(), unset, now),
    });
    if !photos && let Err(e) = store_status(&db, &status).await {
        eprintln!("status: {e}");
        return 1;
    }
    let Some(cfg) = integ.backup.as_ref() else {
        println!("offsite backup not set up (console: 설정 > 백업)");
        return 0;
    };
    let http = reqwest::Client::new();
    let outcome = offsite(&http, cfg, &config.data_key, path).await;
    let code = outcome.as_ref().err().map(Fail::code);
    if let Err(e) = ops::write_outcome(&db, offsite_integration(name), outcome.as_ref().map(|_| ()).map_err(|e| e.code())).await {
        eprintln!("ops record: {e}");
    }
    if photos {
        return report_upload(name, outcome);
    }
    status["offsite"] = json!({ "ok": code.is_none(), "at": Utc::now(), "error": code });
    if let Err(e) = store_status(&db, &status).await {
        eprintln!("status: {e}");
    }
    report_upload(name, outcome)
}

fn report_upload(name: &str, outcome: Result<(), Fail>) -> i32 {
    match outcome {
        Ok(()) => {
            println!("offsite backup uploaded: {name}");
            0
        }
        Err(e) => {
            eprintln!("offsite backup failed: {name}: {e}");
            1
        }
    }
}

/// `api backup-fetch <object> <out>`: download and decrypt one offsite backup.
async fn fetch_cmd(config: &Config, name: &str, out: &str) -> i32 {
    let Ok((_db, integ)) = open(config).await else {
        eprintln!("database unavailable");
        return 1;
    };
    let Some(cfg) = integ.backup.as_ref() else {
        eprintln!("offsite backup is not set up");
        return 1;
    };
    let object = if name.starts_with(PREFIX_OBJECT) {
        name.to_string()
    } else {
        format!("{PREFIX_OBJECT}{name}")
    };
    let out = PathBuf::from(out);
    let cipher = out.with_extension("enc.part");
    let http = reqwest::Client::new();
    let result: Result<(), Fail> = async {
        get_file(&http, cfg, &object, &cipher).await?;
        let src = std::fs::File::open(&cipher)?;
        let mut dst = io::BufWriter::new(std::fs::File::create(&out)?);
        decrypt_stream(&file_key(&config.data_key), io::BufReader::new(src), &mut dst).map_err(|_| Fail::Corrupt)?;
        dst.flush()?;
        Ok(())
    }
    .await;
    let _ = std::fs::remove_file(&cipher);
    match result {
        Ok(()) => {
            println!("restored file written: {}", out.display());
            0
        }
        Err(e) => {
            let _ = std::fs::remove_file(&out);
            eprintln!("fetch failed: {e}");
            1
        }
    }
}

/// `api backup-seal <in> <out>` / `api backup-open <in> <out>`: the offsite encryption without the upload.
/// The restore drill (scripts/ci/backup-drill.sh) runs a real dump through these to prove it survives
/// the same code path the offsite copy uses. Output appears only once it is complete.
fn convert_cmd(config: &Config, input: &str, output: &str, seal: bool) -> i32 {
    let key = file_key(&config.data_key);
    let result = convert(&key, Path::new(input), Path::new(output), seal);
    match result {
        Ok(()) => {
            println!("{}: {output}", if seal { "sealed" } else { "opened" });
            0
        }
        Err(e) => {
            eprintln!("{} failed: {e}", if seal { "seal" } else { "open" });
            1
        }
    }
}

/// Writes to `<output>.part` and renames it into place, so a failure never leaves a partial file
/// under the final name and an existing output is untouched until the new one is complete.
fn convert(key: &[u8; 32], input: &Path, output: &Path, seal: bool) -> Result<(), Fail> {
    let part = PathBuf::from(format!("{}.part", output.display()));
    let result = convert_to(key, input, &part, seal).and_then(|()| std::fs::rename(&part, output).map_err(Fail::from));
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result
}

fn convert_to(key: &[u8; 32], input: &Path, output: &Path, seal: bool) -> Result<(), Fail> {
    let src = io::BufReader::new(std::fs::File::open(input)?);
    let mut dst = io::BufWriter::new(std::fs::File::create(output)?);
    if seal {
        encrypt_stream(key, src, &mut dst)?;
    } else {
        decrypt_stream(key, src, &mut dst).map_err(|_| Fail::Corrupt)?;
    }
    dst.flush()?;
    Ok(())
}

/// `api backup-list`: object names, newest last.
async fn list_cmd(config: &Config) -> i32 {
    let Ok((_db, integ)) = open(config).await else {
        eprintln!("database unavailable");
        return 1;
    };
    let Some(cfg) = integ.backup.as_ref() else {
        eprintln!("offsite backup is not set up");
        return 1;
    };
    match list_objects(&reqwest::Client::new(), cfg).await {
        Ok(mut objects) => {
            objects.sort_by_key(|(_, at)| *at);
            for (key, at) in objects {
                println!(
                    "{}  {}",
                    key.trim_start_matches(PREFIX_OBJECT),
                    at.with_timezone(&chrono::FixedOffset::east_opt(9 * 3600).expect("KST offset"))
                        .format("%Y-%m-%d %H:%M")
                );
            }
            0
        }
        Err(e) => {
            eprintln!("list failed: {e}");
            1
        }
    }
}

/// Runs one backup subcommand. Returns the process exit code.
pub async fn cli(args: &[String]) -> i32 {
    let config = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("config: {e}");
            return 1;
        }
    };
    match args {
        [cmd, file] if cmd == "backup-upload" => upload_cmd(&config, file).await,
        [cmd, name, out] if cmd == "backup-fetch" => fetch_cmd(&config, name, out).await,
        [cmd, input, output] if cmd == "backup-seal" => convert_cmd(&config, input, output, true),
        [cmd, input, output] if cmd == "backup-open" => convert_cmd(&config, input, output, false),
        [cmd] if cmd == "backup-list" => list_cmd(&config).await,
        _ => {
            eprintln!("usage: api backup-upload <file> | backup-fetch <object> <out> | backup-list | backup-seal <in> <out> | backup-open <in> <out>");
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn enc(key: &[u8; 32], data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        encrypt_stream(key, data, &mut out).expect("encrypt");
        out
    }

    fn dec(key: &[u8; 32], data: &[u8]) -> io::Result<Vec<u8>> {
        let mut out = Vec::new();
        decrypt_stream(key, data, &mut out)?;
        Ok(out)
    }

    #[test]
    fn sigv4_matches_aws_get_vanilla_vector() {
        let c = Sigv4 {
            access: "AKIDEXAMPLE",
            secret: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            region: "us-east-1",
            service: "service",
        };
        let headers = [("host", "example.amazonaws.com"), ("x-amz-date", "20150830T123600Z")];
        let auth = authorization(&c, "GET", "/", "", &headers, EMPTY_SHA256, "20150830T123600Z");
        assert_eq!(
            auth,
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/service/aws4_request, SignedHeaders=host;x-amz-date, Signature=5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31"
        );
    }

    #[test]
    fn sigv4_matches_aws_s3_get_object_example() {
        let c = Sigv4 {
            access: "AKIAIOSFODNN7EXAMPLE",
            secret: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            region: "us-east-1",
            service: "s3",
        };
        let headers = [
            ("host", "examplebucket.s3.amazonaws.com"),
            ("range", "bytes=0-9"),
            ("x-amz-content-sha256", EMPTY_SHA256),
            ("x-amz-date", "20130524T000000Z"),
        ];
        let auth = authorization(&c, "GET", "/test.txt", "", &headers, EMPTY_SHA256, "20130524T000000Z");
        assert!(
            auth.ends_with("Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"),
            "{auth}"
        );
    }

    #[test]
    fn signing_key_matches_aws_documented_derivation() {
        let k = signing_key("wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY", "20120215", "us-east-1", "iam");
        assert_eq!(hex(k), "f4780e2d9f65fa895f9c67b32ce1baf0b0d8a43505a000a1a9e090d414db404d");
    }

    #[test]
    fn encoding_keeps_slashes_and_escapes_the_rest() {
        assert_eq!(encode_path("/bucket/malkkeum/db 1.enc"), "/bucket/malkkeum/db%201.enc");
        assert_eq!(host_of("https://acc.r2.cloudflarestorage.com/"), "acc.r2.cloudflarestorage.com");
    }

    #[test]
    fn encrypted_backup_round_trips_across_chunk_boundaries() {
        let key = file_key(&[9u8; 32]);
        for len in [0, 1, CHUNK - 1, CHUNK, CHUNK + 1, CHUNK * 3 + 17] {
            let data: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();
            let sealed = enc(&key, &data);
            assert_eq!(dec(&key, &sealed).expect("decrypt"), data, "len {len}");
        }
    }

    #[test]
    fn wrong_key_tampering_truncation_and_reordering_are_rejected() {
        let key = [1u8; 32];
        let data = vec![7u8; CHUNK * 2 + 5];
        let sealed = enc(&key, &data);
        assert!(dec(&[2u8; 32], &sealed).is_err(), "wrong key");
        let mut flipped = sealed.clone();
        let mid = flipped.len() / 2;
        flipped[mid] ^= 1;
        assert!(dec(&key, &flipped).is_err(), "tampered byte");
        assert!(dec(&key, &sealed[..sealed.len() - 40]).is_err(), "truncated");
        assert!(dec(&key, b"not a backup at all").is_err(), "wrong format");
        let mut trailing = sealed.clone();
        trailing.push(0);
        assert!(dec(&key, &trailing).is_err(), "trailing data");
    }

    #[test]
    fn retention_selects_only_old_malkkeum_backups() {
        let now = Utc.with_ymd_and_hms(2026, 11, 11, 0, 0, 0).unwrap();
        let old = Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap();
        let new = Utc.with_ymd_and_hms(2026, 11, 10, 0, 0, 0).unwrap();
        let objects = vec![
            ("malkkeum/db-old.sql.gz.enc".to_string(), old),
            ("malkkeum/db-new.sql.gz.enc".to_string(), new),
            ("malkkeum/connection-test.txt".to_string(), old),
            ("other/db-old.sql.gz.enc".to_string(), old),
        ];
        assert_eq!(expired(&objects, now, 30), vec!["malkkeum/db-old.sql.gz.enc".to_string()]);
    }

    #[test]
    fn listing_xml_is_parsed() {
        let xml = "<ListBucketResult><Contents><Key>malkkeum/a.enc</Key><LastModified>2026-11-10T18:30:00.000Z</LastModified></Contents></ListBucketResult>";
        let parsed = parse_listing(xml).expect("listing");
        assert_eq!(parsed[0].0, "malkkeum/a.enc");
        assert_eq!(parse_listing("<ListBucketResult></ListBucketResult>"), Some(Vec::new()));
    }

    #[test]
    fn convert_round_trips_a_file_and_leaves_no_partial_output() {
        let dir = std::env::temp_dir().join(format!("malkkeum-convert-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("test dir");
        let key = file_key(&[4u8; 32]);
        let data: Vec<u8> = (0..CHUNK * 3 + 5).map(|i| (i % 253) as u8).collect();
        let (plain, sealed, opened, bad) = (
            dir.join("plain.sql.gz"),
            dir.join("plain.sql.gz.enc"),
            dir.join("opened.sql.gz"),
            dir.join("bad.enc"),
        );
        std::fs::write(&plain, &data).expect("write plain");
        convert(&key, &plain, &sealed, true).expect("seal");
        assert!(std::fs::read(&sealed).expect("sealed").starts_with(MAGIC));
        convert(&key, &sealed, &opened, false).expect("open");
        assert_eq!(std::fs::read(&opened).expect("opened"), data);
        std::fs::write(&bad, b"not a backup at all").expect("write bad");
        let failed = dir.join("failed.sql.gz");
        assert!(convert(&key, &bad, &failed, false).is_err(), "damaged input is refused");
        assert!(!failed.exists() && !dir.join("failed.sql.gz.part").exists(), "no partial output left behind");
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn file_key_is_derived_not_the_data_key() {
        let k = file_key(&[3u8; 32]);
        assert_ne!(k, [3u8; 32]);
        assert_eq!(k, file_key(&[3u8; 32]));
    }

    const GIB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn photo_archives_over_five_gib_go_up_as_parts_within_the_part_limits() {
        assert!(!needs_multipart(4 * GIB));
        assert!(needs_multipart(5 * GIB + 1), "a single PUT stops at 5 GiB");
        let len = 40 * GIB + 7;
        let size = part_size(len);
        assert!((MIN_PART..=5 * GIB).contains(&size), "part size {size}");
        assert!(len.div_ceil(size) <= MAX_PARTS, "too many parts");
        assert!(part_size(10 * 1024 * GIB) <= 5 * GIB, "huge archives still fit the part size cap");
    }

    #[test]
    fn multipart_upload_xml_is_read_and_written() {
        let init = "<InitiateMultipartUploadResult><Bucket>b</Bucket><Key>k</Key><UploadId>VXBs.a_1-2</UploadId></InitiateMultipartUploadResult>";
        assert_eq!(tag(init, "UploadId"), Some("VXBs.a_1-2"));
        let done = complete_xml(&[(1, "\"aa\"".into()), (2, "\"bb\"".into())]);
        assert_eq!(
            done,
            "<CompleteMultipartUpload><Part><PartNumber>1</PartNumber><ETag>\"aa\"</ETag></Part><Part><PartNumber>2</PartNumber><ETag>\"bb\"</ETag></Part></CompleteMultipartUpload>"
        );
    }

    #[test]
    fn photo_and_database_offsite_outcomes_are_kept_apart() {
        assert_eq!(offsite_integration("uploads-20261116-031500.tar.gz"), "backup-uploads");
        assert_eq!(offsite_integration("db-20261116-031500.sql.gz"), "backup");
    }

    // ---- instance role (AWS template): IMDSv2 credentials with their session token, no deletes ----

    type Heads = std::sync::Arc<std::sync::Mutex<Vec<String>>>;

    fn role_doc(expires: DateTime<Utc>) -> String {
        json!({ "Code": "Success", "AccessKeyId": "ASIAFAKE", "SecretAccessKey": "fake-secret", "Token": "fake-session", "Expiration": expires.to_rfc3339() })
            .to_string()
    }

    /// A fake metadata service and S3 bucket on one local port. Keeps every request head; the role's
    /// credentials expire at `expires`. Without the IMDSv2 token header the credential paths answer 401.
    async fn fake_aws(expires: DateTime<Utc>) -> (String, Heads) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let base = format!("http://{}", listener.local_addr().expect("addr"));
        let heads: Heads = Default::default();
        let log = heads.clone();
        tokio::spawn(async move {
            while let Ok((sock, _)) = listener.accept().await {
                tokio::spawn(answer(sock, log.clone(), expires));
            }
        });
        (base, heads)
    }

    async fn answer(mut sock: tokio::net::TcpStream, log: Heads, expires: DateTime<Utc>) {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        let end = loop {
            let Ok(n) = sock.read(&mut chunk).await else { return };
            if n == 0 {
                return;
            }
            buf.extend_from_slice(&chunk[..n]);
            if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break i + 4;
            }
        };
        let head = String::from_utf8_lossy(&buf[..end]).to_lowercase();
        let length: usize = head
            .lines()
            .find_map(|l| l.strip_prefix("content-length:"))
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0);
        while buf.len() < end + length {
            let Ok(n) = sock.read(&mut chunk).await else { return };
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        log.lock().expect("log").push(head.clone());
        let line = head.lines().next().unwrap_or_default().to_string();
        let token_ok = head.contains("x-aws-ec2-metadata-token: tok-1");
        let (status, body) = match line.split(' ').take(2).collect::<Vec<_>>()[..] {
            ["put", "/latest/api/token"] if head.contains("x-aws-ec2-metadata-token-ttl-seconds:") => (200, "tok-1".to_string()),
            ["get", "/latest/meta-data/iam/security-credentials/"] if token_ok => (200, "dahae-main-instancerole\n".into()),
            ["get", "/latest/meta-data/iam/security-credentials/dahae-main-instancerole"] if token_ok => (200, role_doc(expires)),
            ["get", p] if p.starts_with("/dahae-bkt?") => (200, "<ListBucketResult></ListBucketResult>".into()),
            ["put", p] if p.starts_with("/dahae-bkt/") => (200, String::new()),
            ["get" | "put", p] if p.starts_with("/latest/") => (401, String::new()),
            _ => (404, String::new()),
        };
        let reply = format!("HTTP/1.1 {status} X\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len());
        let _ = sock.write_all(reply.as_bytes()).await;
    }

    fn role_bucket(base: &str) -> S3Config {
        let mut cfg = S3Config::instance_role("dahae-bkt", "ap-northeast-2").expect("valid");
        cfg.endpoint = base.to_string();
        cfg.instance_role = Some(base.to_string());
        cfg
    }

    fn count(heads: &Heads, start: &str) -> usize {
        heads.lock().expect("log").iter().filter(|h| h.starts_with(start)).count()
    }

    #[test]
    fn instance_role_bucket_is_aws_s3_in_its_region_with_no_keys() {
        let cfg = S3Config::instance_role("dahae-main-backupbucket-1a2b", "ap-northeast-2").expect("valid");
        assert_eq!(cfg.endpoint, "https://s3.ap-northeast-2.amazonaws.com");
        assert_eq!(cfg.instance_role.as_deref(), Some(IMDS_BASE));
        assert!(cfg.access_key.is_empty() && cfg.secret_key.is_empty());
        for (bucket, region) in [
            ("Dahae", "ap-northeast-2"),
            ("ab", "ap-northeast-2"),
            ("a/b", "ap-northeast-2"),
            ("dahae-b", ""),
            ("dahae-b", "ap northeast"),
        ] {
            assert!(S3Config::instance_role(bucket, region).is_none(), "{bucket} {region}");
        }
    }

    #[test]
    fn role_credentials_need_success_and_all_four_fields() {
        let at = Utc::now();
        assert_eq!(parse_role_creds(&role_doc(at)).map(|c| c.token), Some("fake-session".into()));
        let mut doc: Value = serde_json::from_str(&role_doc(at)).expect("json");
        doc["Code"] = json!("Failure");
        assert!(parse_role_creds(&doc.to_string()).is_none());
        let mut doc: Value = serde_json::from_str(&role_doc(at)).expect("json");
        doc["Token"] = json!("");
        assert!(parse_role_creds(&doc.to_string()).is_none());
        assert!(parse_role_creds("not json").is_none());
    }

    #[tokio::test]
    async fn role_requests_carry_the_signed_session_token_and_reuse_fresh_credentials() {
        let (base, heads) = fake_aws(Utc::now() + Duration::hours(6)).await;
        let cfg = role_bucket(&base);
        let http = reqwest::Client::new();
        list_objects(&http, &cfg).await.expect("first listing");
        list_objects(&http, &cfg).await.expect("second listing");
        assert_eq!(count(&heads, "put /latest/api/token"), 1, "fresh credentials are reused");
        let listing = heads
            .lock()
            .expect("log")
            .iter()
            .find(|h| h.starts_with("get /dahae-bkt?"))
            .cloned()
            .expect("listing request");
        assert!(listing.contains("x-amz-security-token: fake-session"), "{listing}");
        assert!(listing.contains("credential=asiafake/"), "{listing}");
        assert!(
            listing.contains("signedheaders=host;x-amz-content-sha256;x-amz-date;x-amz-security-token"),
            "{listing}"
        );
    }

    #[tokio::test]
    async fn role_credentials_close_to_expiry_are_fetched_again() {
        let (base, heads) = fake_aws(Utc::now() + Duration::seconds(60)).await;
        let cfg = role_bucket(&base);
        let http = reqwest::Client::new();
        list_objects(&http, &cfg).await.expect("first");
        list_objects(&http, &cfg).await.expect("second");
        assert_eq!(count(&heads, "put /latest/api/token"), 2, "inside the refresh margin");
    }

    #[tokio::test]
    async fn role_backups_upload_and_check_without_ever_deleting() {
        let (base, heads) = fake_aws(Utc::now() + Duration::hours(6)).await;
        let cfg = role_bucket(&base);
        let http = reqwest::Client::new();
        let dir = std::env::temp_dir().join(format!("role-backup-{}", uuid::Uuid::now_v7().simple()));
        std::fs::create_dir_all(&dir).expect("dir");
        let file = dir.join("db-20261123-033000.sql.gz");
        std::fs::write(&file, b"dump").expect("dump");
        offsite(&http, &cfg, &[3u8; 32], &file).await.expect("upload");
        check(&http, &cfg).await.expect("check");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(count(&heads, "put /dahae-bkt/malkkeum/db-20261123-033000.sql.gz.enc"), 1);
        assert_eq!(count(&heads, "delete "), 0, "the role has no delete; lifecycle expires old copies");
        assert_eq!(count(&heads, "get /dahae-bkt?"), 1, "the check lists instead of writing a test object");
    }

    #[tokio::test]
    async fn no_metadata_service_is_reported_as_no_instance_role() {
        let (base, _) = fake_aws(Utc::now() + Duration::hours(6)).await;
        let mut cfg = role_bucket(&base);
        cfg.instance_role = Some(format!("{base}/missing"));
        let err = list_objects(&reqwest::Client::new(), &cfg).await.expect_err("no role");
        assert_eq!(err.code(), "no_instance_role");
    }
}
