// Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
// SPDX-License-Identifier: LicenseRef-Proprietary

//! Server jobs an owner queues from the console: the kinds the server agent runs, their argument rules,
//! and the job file format `deploy/agent.sh` reads from the inbox. Nothing here touches the disk.

use crate::{config::valid_edition_name, entropy};
use uuid::Uuid;

const DAYS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];

/// Kinds the server agent runs today. Anything else is refused before a job file is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    StatusRefresh,
    BackupNow,
    Update,
    UpdateSkip,
    Channel,
    Restart,
    Schedule,
    DomainCheck,
    DomainSet,
    Heartbeat,
    HostApprovalEnroll,
    Rollback,
    /// The secret keys (DATA_KEY, JWT_SECRET) sealed with the owner's passphrase (`arg`, hex), handed out once through `output`.
    KeysBackup,
    /// Records that the owner keeps the sealed copy off the server. Queued by the server when the owner says so
    /// after the download (`kept`), never by a client.
    KeysConfirm,
    /// Follow another signed release stream (`arg` = an edition the installer offered this domain, `dahae edition`).
    Edition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobSpec {
    pub kind: Kind,
    pub arg: String,
}

impl Kind {
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "status-refresh" => Self::StatusRefresh,
            "backup-now" => Self::BackupNow,
            "update" => Self::Update,
            "update-skip" => Self::UpdateSkip,
            "channel" => Self::Channel,
            "restart" => Self::Restart,
            "schedule" => Self::Schedule,
            "domain-check" => Self::DomainCheck,
            "domain-set" => Self::DomainSet,
            "heartbeat" => Self::Heartbeat,
            "host-approval-enroll" => Self::HostApprovalEnroll,
            "rollback" => Self::Rollback,
            "keys-backup" => Self::KeysBackup,
            "edition" => Self::Edition,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::StatusRefresh => "status-refresh",
            Self::BackupNow => "backup-now",
            Self::Update => "update",
            Self::UpdateSkip => "update-skip",
            Self::Channel => "channel",
            Self::Restart => "restart",
            Self::Schedule => "schedule",
            Self::DomainCheck => "domain-check",
            Self::DomainSet => "domain-set",
            Self::Heartbeat => "heartbeat",
            Self::HostApprovalEnroll => "host-approval-enroll",
            Self::Rollback => "rollback",
            Self::KeysBackup => "keys-backup",
            Self::KeysConfirm => "keys-confirm",
            Self::Edition => "edition",
        }
    }

    /// Plain Korean name for owner texts.
    pub fn label(self) -> &'static str {
        match self {
            Self::StatusRefresh => "상태 새로고침",
            Self::BackupNow => "지금 백업",
            Self::Update => "업데이트",
            Self::UpdateSkip => "업데이트 건너뛰기",
            Self::Channel => "업데이트 채널 변경",
            Self::Restart => "서버 재시작",
            Self::Schedule => "자동 작업 일정 변경",
            Self::DomainCheck => "도메인 확인",
            Self::DomainSet => "도메인 바꾸기",
            Self::Heartbeat => "외부 가동 확인 주소 변경",
            Self::HostApprovalEnroll => "서버 승인 코드 만들기",
            Self::Rollback => "이전 버전으로 되돌리기",
            Self::KeysBackup => "비밀 키 사본 내려받기",
            Self::KeysConfirm => "비밀 키 사본 보관 확인",
            Self::Edition => "업데이트 판(에디션) 바꾸기",
        }
    }

    /// Owner kinds need the owner's TOTP on the request. The read-only status refresh and DNS check need only
    /// an admin session.
    pub fn needs_owner(self) -> bool {
        !matches!(self, Self::StatusRefresh | Self::DomainCheck)
    }

    /// The agent holds these until the server approval code arrives (`agent.sh::is_high_kind`).
    pub fn needs_approval(self) -> bool {
        matches!(self, Self::DomainSet | Self::Rollback | Self::Edition)
    }

    /// The argument is a secret (the key copy's passphrase): it goes into the job file only, never the database.
    pub fn secret_arg(self) -> bool {
        self == Self::KeysBackup
    }
}

/// Checks a kind and its argument the way the agent does. Returns a stable error code for the response.
/// An `edition` must also be one the installer offered this server ([`edition_offered`]).
pub fn validate(kind: &str, arg: &str) -> Result<JobSpec, &'static str> {
    let kind = Kind::parse(kind).ok_or("unknown_job_kind")?;
    let ok = match kind {
        Kind::StatusRefresh | Kind::BackupNow | Kind::Update | Kind::UpdateSkip | Kind::Rollback | Kind::KeysConfirm => arg.is_empty(),
        Kind::KeysBackup => valid_passphrase_hex(arg),
        Kind::Channel => matches!(arg, "stable" | "beta"),
        Kind::Restart => matches!(arg, "api" | "web" | "db" | "all"),
        Kind::Schedule => valid_schedule(arg),
        Kind::DomainCheck | Kind::DomainSet => valid_domain(arg),
        Kind::Heartbeat => arg == "off" || valid_heartbeat_url(arg),
        // The server fills in the caller's staff id; a client never chooses whose code it is.
        Kind::HostApprovalEnroll => arg.is_empty(),
        Kind::Edition => valid_edition_name(arg),
    };
    if ok {
        Ok(JobSpec { kind, arg: arg.to_string() })
    } else {
        Err("invalid_job_arg")
    }
}

/// The agent's `edition_offered`: only an edition in `EDITIONS_OFFERED`, which lists the editions whose table row
/// names this domain. Every other server has an empty list, so the switch is never offered there.
pub fn edition_offered(name: &str, offered: &[String]) -> bool {
    offered.iter().any(|e| e == name)
}

/// The agent's `KEYS_PASS_HEX_RE`: 12 to 256 bytes of passphrase as lowercase hex.
fn valid_passphrase_hex(arg: &str) -> bool {
    arg.len().is_multiple_of(2) && (24..=512).contains(&arg.len()) && arg.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// `off`, or a weekday and a 24-hour time such as `mon@04:00`.
fn valid_schedule(arg: &str) -> bool {
    if arg == "off" {
        return true;
    }
    let Some((day, time)) = arg.split_once('@') else {
        return false;
    };
    let Some((hh, mm)) = time.split_once(':') else {
        return false;
    };
    DAYS.contains(&day) && two_digits(hh).is_some_and(|h| h <= 23) && two_digits(mm).is_some_and(|m| m <= 59)
}

/// The agent's `DOMAIN_RE`: lowercase labels of letters, digits and inner hyphens; the last label letters only,
/// or an `xn--` label. At most 253 characters, never `.local`.
pub fn valid_domain(d: &str) -> bool {
    let labels: Vec<&str> = d.split('.').collect();
    let Some((tld, rest)) = labels.split_last() else {
        return false;
    };
    d.len() <= 253 && !rest.is_empty() && rest.iter().all(|l| valid_label(l)) && valid_tld(tld)
}

fn valid_label(l: &str) -> bool {
    let b = l.as_bytes();
    let inner = |c: &u8| c.is_ascii_lowercase() || c.is_ascii_digit();
    (1..=63).contains(&b.len()) && inner(&b[0]) && inner(&b[b.len() - 1]) && b.iter().all(|c| inner(c) || *c == b'-')
}

fn valid_tld(l: &str) -> bool {
    match l.strip_prefix("xn--") {
        Some(p) => (1..=59).contains(&p.len()) && p.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-'),
        None => (2..=63).contains(&l.len()) && l.bytes().all(|c| c.is_ascii_lowercase()) && l != "local",
    }
}

/// The agent's `HB_URL_RE`: `https://host[:port][/path]`, with no `&`, quotes or spaces. The agent also checks
/// that the host resolves to public addresses only.
pub fn valid_heartbeat_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let (authority, path) = rest.find('/').map_or((rest, ""), |i| rest.split_at(i));
    let (host, port) = authority.split_once(':').map_or((authority, None), |(h, p)| (h, Some(p)));
    let host_ok = !host.is_empty() && host.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'.' || c == b'-');
    let port_ok = port.is_none_or(|p| (1..=5).contains(&p.len()) && p.bytes().all(|c| c.is_ascii_digit()));
    // No '=': the agent's job_parse refuses it in any value (agent.sh), so the job would fail after the owner code.
    let path_ok = path.bytes().all(|c| c.is_ascii_alphanumeric() || b"._~%/:@+?-".contains(&c));
    host_ok && port_ok && path_ok
}

fn two_digits(s: &str) -> Option<u8> {
    if s.len() == 2 && s.bytes().all(|b| b.is_ascii_digit()) {
        s.parse().ok()
    } else {
        None
    }
}

/// A fresh 128-bit public progress token, as 32 lowercase hex digits (the agent's token format).
pub fn new_progress_token() -> String {
    let mut bytes = [0u8; 16];
    entropy::fill(&mut bytes);
    hex::encode(bytes)
}

/// The job file the agent reads: `id`, `kind`, optional `arg`, `token`, one `key=value` per line.
pub fn job_file(id: Uuid, spec: &JobSpec, token: &str) -> String {
    let arg = if spec.arg.is_empty() { String::new() } else { format!("arg={}\n", spec.arg) };
    format!("id={}\nkind={}\n{arg}token={token}\n", id.hyphenated(), spec.kind.name())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_the_kinds_and_args_the_agent_runs() {
        assert!(validate("backup-now", "").is_ok());
        assert!(validate("channel", "beta").is_ok());
        assert!(validate("restart", "all").is_ok());
        assert!(validate("schedule", "sun@23:59").is_ok());
        assert!(validate("schedule", "off").is_ok());
        assert_eq!(validate("restart-service", ""), Err("unknown_job_kind"));
        assert_eq!(validate("channel", "nightly"), Err("invalid_job_arg"));
        assert_eq!(validate("update-skip", "x"), Err("invalid_job_arg"));
    }

    #[test]
    fn schedule_rejects_bad_days_and_times() {
        for bad in ["mon@24:00", "mon@12:60", "mon@1:00", "mon@+1:00", "funday@04:00", "mon@04-00", "mon04:00"] {
            assert_eq!(validate("schedule", bad), Err("invalid_job_arg"), "{bad}");
        }
    }

    #[test]
    fn domain_kinds_take_a_domain_like_the_agent() {
        for good in ["myshop.co.kr", "a-b.example.com", "shop.xn--3e0b707e", "x1.io"] {
            assert!(validate("domain-check", good).is_ok(), "{good}");
            assert!(validate("domain-set", good).is_ok(), "{good}");
        }
        let long = format!("{}.co.kr", "a".repeat(64));
        for bad in [
            "",
            "localhost",
            "1.2.3.4",
            "a..b",
            "x.local",
            "MyShop.co.kr",
            "-a.com",
            "a-.com",
            "a.com;id",
            "a.c",
            &long,
        ] {
            assert_eq!(validate("domain-set", bad), Err("invalid_job_arg"), "{bad}");
        }
    }

    #[test]
    fn heartbeat_takes_off_or_an_https_url() {
        assert!(validate("heartbeat", "off").is_ok());
        assert!(validate("heartbeat", "https://hc-ping.com/1f2e-aaaa").is_ok());
        assert!(validate("heartbeat", "https://x.io:8443/a?b").is_ok());
        for bad in [
            "",
            "https://x.io:8443/a?b=1",
            "http://hc-ping.com/x",
            "https://x.io/a?b=1&c=2",
            "https://x.io/$(id)",
            "https://",
            "https://x.io:123456/",
            "https://x .io",
        ] {
            assert_eq!(validate("heartbeat", bad), Err("invalid_job_arg"), "{bad}");
        }
    }

    #[test]
    fn enrol_never_takes_an_id_from_the_client_and_update_never_waits_for_approval() {
        assert!(validate("host-approval-enroll", "").is_ok());
        assert_eq!(validate("host-approval-enroll", "0190a6f2-1111-7000-8000-000000000001"), Err("invalid_job_arg"));
        assert!(!Kind::DomainCheck.needs_owner());
        assert!(Kind::DomainSet.needs_owner() && Kind::Heartbeat.needs_owner() && Kind::HostApprovalEnroll.needs_owner());
        assert!(Kind::DomainSet.needs_approval());
        assert!(!Kind::Update.needs_approval() && !Kind::DomainCheck.needs_approval());
    }

    #[test]
    fn rollback_waits_for_approval_and_the_key_copy_takes_a_hex_passphrase_kept_out_of_the_database() {
        assert!(validate("rollback", "").is_ok());
        assert_eq!(validate("rollback", "v1.5.0"), Err("invalid_job_arg"));
        assert!(Kind::Rollback.needs_owner() && Kind::Rollback.needs_approval());
        let pass = hex::encode("correct horse battery");
        assert!(validate("keys-backup", &pass).is_ok());
        assert!(Kind::KeysBackup.needs_owner() && Kind::KeysBackup.secret_arg() && !Kind::KeysBackup.needs_approval());
        for bad in [
            String::new(),
            hex::encode("elevenchars"),
            pass.to_uppercase(),
            format!("{pass}a"),
            "zz".repeat(12),
            "ab".repeat(257),
        ] {
            assert_eq!(validate("keys-backup", &bad), Err("invalid_job_arg"), "{bad}");
        }
        assert_eq!(validate("keys-confirm", ""), Err("unknown_job_kind"), "only the server queues the confirmation");
        assert_eq!(Kind::KeysConfirm.name(), "keys-confirm");
    }

    #[test]
    fn edition_is_an_owner_job_held_for_approval_and_takes_only_an_offered_name() {
        assert_eq!(
            validate("edition", "uzipsa"),
            Ok(JobSpec {
                kind: Kind::Edition,
                arg: "uzipsa".into()
            })
        );
        assert!(Kind::Edition.needs_owner() && Kind::Edition.needs_approval() && !Kind::Edition.secret_arg());
        assert_eq!(Kind::parse(Kind::Edition.name()), Some(Kind::Edition));
        for bad in ["", "U", "Uzipsa", "uzipsa;id", "a,b", "../x", &"a".repeat(32)] {
            assert_eq!(validate("edition", bad), Err("invalid_job_arg"), "{bad}");
        }
        let offered = vec!["uzipsa".to_string()];
        assert!(edition_offered("uzipsa", &offered));
        assert!(!edition_offered("dahae", &offered), "not listed");
        assert!(!edition_offered("uzipsa", &[]), "nothing offered on other servers");
    }

    #[test]
    fn job_file_has_the_agent_format() {
        let id = Uuid::now_v7();
        let spec = JobSpec {
            kind: Kind::Channel,
            arg: "stable".into(),
        };
        let body = job_file(id, &spec, "0123456789abcdef0123456789abcdef");
        assert_eq!(
            body,
            format!("id={}\nkind=channel\narg=stable\ntoken=0123456789abcdef0123456789abcdef\n", id.hyphenated())
        );
        let bare = job_file(
            id,
            &JobSpec {
                kind: Kind::BackupNow,
                arg: String::new(),
            },
            "0123456789abcdef0123456789abcdef",
        );
        assert!(!bare.contains("arg="));
    }

    #[test]
    fn progress_token_is_32_lowercase_hex() {
        let t = new_progress_token();
        assert_eq!(t.len(), 32);
        assert!(t.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
    }
}
