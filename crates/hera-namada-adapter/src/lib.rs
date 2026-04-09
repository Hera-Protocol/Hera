#![forbid(unsafe_code)]

pub mod asset_resolver;
pub mod error;
pub mod mapper;
pub mod masp_sync;
pub mod note_detection;
pub mod types;
pub mod viewing_key;

pub use asset_resolver::AssetResolver;
pub use error::NamadaAdapterError;
pub use mapper::map_note_to_canonical;
pub use masp_sync::{IndexerCursor, MaspIndexerClient, PublicIndexerState, ShieldedContext};
pub use note_detection::detect_owned_notes;
pub use types::{FeeRecord, MaspNote, NamadaScanCheckpoint, TransferDirection};
pub use viewing_key::{parse_and_validate, ValidatedNamadaKey};
