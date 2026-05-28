use ark_bls12_381::Fr;

/// Maps CanonicalEvent fields into BLS12-381 scalar field elements.
///
/// Each `WitnessRecord` is one event's circuit-ready representation. Not all
/// fields are relevant to every circuit, but every circuit shares the same
/// encoding so witnesses are interoperable across proof types.
#[derive(Debug, Clone)]
pub struct WitnessRecord {
    /// Amount in smallest unit, encoded as Fr.
    pub amount: Fr,
    /// Event type ordinal: Shield=0, Receive=1, Send=2, Unshield=3, Fee=4.
    pub event_type: Fr,
    /// Txid hashed via Blake2s with domain separation to a single Fr.
    pub txid_hash: Fr,
    /// Block height as Fr.
    pub block_height: Fr,
    /// Unix timestamp in seconds as Fr.
    pub timestamp: Fr,
    /// Externally supplied risk score [0..100] as Fr.
    pub risk_score: Fr,
}

/// Encodes event type variants as ordinal field elements so circuits can
/// filter by type without string comparisons.
pub fn event_type_ordinal(event_type: &hera_types::EventType) -> u64 {
    match event_type {
        hera_types::EventType::Shield => 0,
        hera_types::EventType::Receive => 1,
        hera_types::EventType::Send => 2,
        hera_types::EventType::Unshield => 3,
        hera_types::EventType::Fee => 4,
    }
}
