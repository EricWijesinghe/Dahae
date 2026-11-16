//! Business rules shared by the API server and (through FFI later) the apps.
//! Everything here is pure and deterministic: callers pass "today" and data in.

pub mod booking;
pub mod cancel_policy;
pub mod cash_receipt;
pub mod catalog;
pub mod coupon;
pub mod error;
pub mod feed;
pub mod finance;
pub mod growth;
pub mod household;
pub mod money;
pub mod partner;
pub mod payee_identifier;
pub mod peak;
pub mod points;
pub mod pricing;
pub mod quality;
pub mod quote;
pub mod schedule;
pub mod service_group;
pub mod subscription;
pub mod tax;
pub mod tip;
pub mod training;
pub mod waitlist;

pub use error::DomainError;
pub use money::Won;
