use hera_types::ChainId;

/// Names the reproducible evidence handles that can be derived from a scan run
/// and re-checked later during audit or report verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceRef {
    /// Anchors an event to a Zcash compact block so the raw lightwalletd source
    /// can be retrieved again by height and block hash.
    CompactBlock { height: u32, hash: String },
    /// Anchors an event to a decryption witness so owned-note claims can be tied
    /// back to a stable commitment identifier.
    DecryptionWitness { note_commitment: String },
    /// Anchors an event to an indexer record when the chain integration relies on
    /// a pre-filtered sync service instead of raw block scanning.
    IndexerEntry { url: String, txid: String },
}

/// Serializes evidence references into stable string forms because evidence
/// references must be reproducible — given the same scan, we should produce the
/// same refs. This is what makes reports auditable.
pub fn build_evidence_refs(chain: ChainId, refs: &[EvidenceRef]) -> Vec<String> {
    refs.iter()
        .map(|reference| match (&chain, reference) {
            (ChainId::Zcash, EvidenceRef::CompactBlock { height, hash }) => {
                format!("compactblock:{height}:{hash}")
            }
            (_, EvidenceRef::DecryptionWitness { note_commitment }) => {
                format!("decryption_witness:{note_commitment}")
            }
            (_, EvidenceRef::IndexerEntry { url, txid }) => format!("indexer:{url}:{txid}"),
            (ChainId::Namada, EvidenceRef::CompactBlock { height, hash }) => {
                format!("compactblock:{height}:{hash}")
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use hera_types::ChainId;

    use super::{build_evidence_refs, EvidenceRef};

    #[test]
    fn builds_stable_evidence_refs_for_mixed_sources() {
        let refs = build_evidence_refs(
            ChainId::Namada,
            &[
                EvidenceRef::IndexerEntry {
                    url: "tx/abc".into(),
                    txid: "abc".into(),
                },
                EvidenceRef::DecryptionWitness {
                    note_commitment: "commitment-1".into(),
                },
            ],
        );

        assert_eq!(
            refs,
            vec![
                "indexer:tx/abc:abc".to_string(),
                "decryption_witness:commitment-1".to_string()
            ]
        );
    }
}
