#![forbid(unsafe_code)]

pub mod decryption;
pub mod error;
pub mod mapper;
pub mod scanner;
pub mod types;
pub mod viewing_key;

pub use decryption::trial_decrypt_note;
pub use error::ZcashAdapterError;
pub use mapper::{map_note_to_canonical, TxMeta};
pub use scanner::{ResolvedLightwalletd, ZcashScanner};
pub use types::{MemoVisibility, Pool, ZcashNote, ZcashScanCheckpoint};
pub use viewing_key::{parse_and_validate, KeyScope, ValidatedZcashKey};
