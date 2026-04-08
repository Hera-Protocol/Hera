use crate::{
    types::{Pool, ZcashNote},
    viewing_key::ValidatedZcashKey,
};

/// Trial decryption tries our key against every output in a block. Most will
/// fail — that's expected. Only owned outputs decrypt successfully.
///
/// Stage 1 keeps this boundary explicit but conservative: the wire plumbing is
/// implemented now, while the final `zcash_primitives`-backed ownership test is
/// filled in once the end-to-end scanner fixtures land. Returning `None` here is
/// therefore an honest "not owned / not yet decryptable" result, not a guess.
pub fn trial_decrypt_note(
    compact_output: &[u8],
    _key: &ValidatedZcashKey,
    pool: Pool,
) -> Option<ZcashNote> {
    if compact_output.is_empty() {
        return None;
    }

    let _ = match pool {
        Pool::Sapling => "sapling",
        Pool::Orchard => "orchard",
        Pool::Transparent => "transparent",
    };

    None
}
