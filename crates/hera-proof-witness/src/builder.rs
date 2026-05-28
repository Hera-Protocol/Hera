use std::collections::HashMap;

use ark_bls12_381::Fr;
use uuid::Uuid;

use hera_types::CanonicalEvent;

use crate::{
    amount::parse_amount,
    error::WitnessError,
    schema::{event_type_ordinal, WitnessRecord},
    txid_hash::hash_txid,
};

/// Converts a slice of canonical compliance events plus externally supplied
/// risk scores into circuit-ready witness records.
///
/// Risk scores are decoupled from `CanonicalEvent` because risk evaluation
/// is an independent concern that should not pollute the core compliance type.
pub struct WitnessBuilder;

impl WitnessBuilder {
    pub fn build(
        events: &[CanonicalEvent],
        risk_scores: &HashMap<Uuid, u8>,
    ) -> Result<Vec<WitnessRecord>, WitnessError> {
        events
            .iter()
            .map(|event| Self::convert(event, risk_scores))
            .collect()
    }

    fn convert(
        event: &CanonicalEvent,
        risk_scores: &HashMap<Uuid, u8>,
    ) -> Result<WitnessRecord, WitnessError> {
        let amount = parse_amount(&event.amount, event.asset.decimals)?;
        let event_type = Fr::from(event_type_ordinal(&event.event_type));
        let txid_hash = hash_txid(&event.txid)?;
        let block_height = Fr::from(event.block_height);
        let timestamp = Fr::from(event.timestamp.timestamp() as u64);

        let risk = risk_scores
            .get(&event.event_id)
            .copied()
            .ok_or(WitnessError::MissingRiskScore(event.event_id))?;
        let risk_score = Fr::from(u64::from(risk));

        Ok(WitnessRecord {
            amount,
            event_type,
            txid_hash,
            block_height,
            timestamp,
            risk_score,
        })
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use hera_types::*;

    use super::*;

    fn test_event(event_id: Uuid, amount: &str, event_type: EventType) -> CanonicalEvent {
        CanonicalEvent {
            event_id,
            case_id: Uuid::nil(),
            chain: ChainId::Zcash,
            network: Network::Testnet,
            event_type,
            txid: "deadbeef".into(),
            block_height: 100,
            timestamp: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            asset: Asset {
                symbol: "ZEC".into(),
                asset_id: "native".into(),
                decimals: 8,
            },
            amount: amount.into(),
            counterparty: Counterparty {
                visibility: CounterpartyVisibility::Unknown,
                value: None,
            },
            memo: EventMemo {
                present: false,
                hash: None,
            },
            evidence_refs: vec![],
            provenance: EventProvenance {
                source: "test".into(),
                pool: None,
                scan_version: "test".into(),
            },
            notes: vec![],
        }
    }

    #[test]
    fn builds_witness_records_from_events() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let events = vec![
            test_event(id1, "1.25000000", EventType::Receive),
            test_event(id2, "0.50000000", EventType::Send),
        ];
        let mut risk_scores = HashMap::new();
        risk_scores.insert(id1, 10);
        risk_scores.insert(id2, 20);

        let records = WitnessBuilder::build(&events, &risk_scores).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].amount, Fr::from(125_000_000u128));
        assert_eq!(records[0].event_type, Fr::from(1u64)); // Receive
        assert_eq!(records[1].event_type, Fr::from(2u64)); // Send
        assert_eq!(records[0].risk_score, Fr::from(10u64));
    }

    #[test]
    fn fails_on_missing_risk_score() {
        let id = Uuid::new_v4();
        let events = vec![test_event(id, "1.00000000", EventType::Receive)];
        let risk_scores = HashMap::new();

        assert!(WitnessBuilder::build(&events, &risk_scores).is_err());
    }
}
