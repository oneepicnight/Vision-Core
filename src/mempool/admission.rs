use crate::types::transaction::{canonical_tx_id, validate_tx_stateless, TxValidationError};
use crate::types::Tx;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MempoolAdmissionError {
    StatelessValidation(TxValidationError),
    DuplicateCanonicalTxId,
    StaleNonce {
        current_nonce: u64,
        tx_nonce: u64,
    },
    NonceGap {
        current_nonce: u64,
        tx_nonce: u64,
    },
    DuplicateSenderNonce {
        sender_pubkey: String,
        nonce: u64,
        existing_tip: u64,
        new_tip: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmissionDecision {
    Accept,
    Replace { evict_tx_id: String },
}

/// Canonical mempool admission policy for a snapshot of pending transactions.
pub struct MempoolAdmission<'a> {
    current_nonce: u64,
    pending: &'a [Tx],
}

impl<'a> MempoolAdmission<'a> {
    pub fn new(current_nonce: u64, pending: &'a [Tx]) -> Self {
        Self {
            current_nonce,
            pending,
        }
    }

    pub fn evaluate(&self, tx: &Tx) -> Result<AdmissionDecision, MempoolAdmissionError> {
        validate_tx_stateless(tx).map_err(MempoolAdmissionError::StatelessValidation)?;

        let tx_id = canonical_tx_id(tx);
        if self
            .pending
            .iter()
            .any(|existing| canonical_tx_id(existing) == tx_id)
        {
            return Err(MempoolAdmissionError::DuplicateCanonicalTxId);
        }

        if tx.nonce < self.current_nonce {
            return Err(MempoolAdmissionError::StaleNonce {
                current_nonce: self.current_nonce,
                tx_nonce: tx.nonce,
            });
        }

        if tx.nonce > self.current_nonce.saturating_add(1) {
            return Err(MempoolAdmissionError::NonceGap {
                current_nonce: self.current_nonce,
                tx_nonce: tx.nonce,
            });
        }

        if let Some(existing) = self.pending.iter().find(|existing| {
            existing.sender_pubkey == tx.sender_pubkey && existing.nonce == tx.nonce
        }) {
            if tx.tip > existing.tip {
                return Ok(AdmissionDecision::Replace {
                    evict_tx_id: canonical_tx_id(existing),
                });
            }

            return Err(MempoolAdmissionError::DuplicateSenderNonce {
                sender_pubkey: tx.sender_pubkey.clone(),
                nonce: tx.nonce,
                existing_tip: existing.tip,
                new_tip: tx.tip,
            });
        }

        Ok(AdmissionDecision::Accept)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::transaction::{canonical_unsigned_payload, CashTransferArgs};
    use ed25519_dalek::{Signer, SigningKey};

    fn signing_key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn sign_transfer_tx(mut tx: Tx, seed: u8) -> Tx {
        let signing_key = signing_key(seed);
        tx.sender_pubkey = hex::encode(signing_key.verifying_key().to_bytes());
        tx.sig.clear();
        let sig = signing_key.sign(&canonical_unsigned_payload(&tx));
        tx.sig = hex::encode(sig.to_bytes());
        tx
    }

    fn transfer_args(to: &str, amount: u128) -> Vec<u8> {
        serde_json::to_vec(&CashTransferArgs {
            to: to.to_string(),
            amount,
        })
        .unwrap()
    }

    fn signed_transfer_tx_with_amount(
        seed: u8,
        nonce: u64,
        tip: u64,
        fee_limit: u64,
        amount: u128,
    ) -> Tx {
        sign_transfer_tx(
            Tx {
                nonce,
                sender_pubkey: String::new(),
                module: "cash".to_string(),
                method: "transfer".to_string(),
                args: transfer_args(&"22".repeat(32), amount),
                tip,
                fee_limit,
                sig: String::new(),
            },
            seed,
        )
    }

    fn signed_transfer_tx(seed: u8, nonce: u64, tip: u64, fee_limit: u64) -> Tx {
        signed_transfer_tx_with_amount(seed, nonce, tip, fee_limit, 1)
    }

    fn with_sender_nonce(seed: u8, nonce: u64, tip: u64, fee_limit: u64) -> Tx {
        signed_transfer_tx(seed, nonce, tip, fee_limit)
    }

    #[test]
    fn accepts_valid_tx() {
        let tx = signed_transfer_tx(1, 7, 2, 1_000);
        let admission = MempoolAdmission::new(7, &[]);

        assert_eq!(admission.evaluate(&tx), Ok(AdmissionDecision::Accept));
    }

    #[test]
    fn rejects_invalid_stateless_tx_before_admission() {
        let mut tx = signed_transfer_tx(1, 7, 2, 200);
        tx.fee_limit = 200;
        let admission = MempoolAdmission::new(7, &[]);

        assert_eq!(
            admission.evaluate(&tx),
            Err(MempoolAdmissionError::StatelessValidation(
                TxValidationError::FeeLimitTooLow
            ))
        );
    }

    #[test]
    fn rejects_duplicate_canonical_tx_id() {
        let tx = signed_transfer_tx(1, 7, 2, 1_000);
        let pending = vec![tx.clone()];
        let admission = MempoolAdmission::new(7, &pending);

        assert_eq!(
            admission.evaluate(&tx),
            Err(MempoolAdmissionError::DuplicateCanonicalTxId)
        );
    }

    #[test]
    fn rejects_stale_nonce() {
        let tx = signed_transfer_tx(1, 4, 2, 1_000);
        let admission = MempoolAdmission::new(5, &[]);

        assert_eq!(
            admission.evaluate(&tx),
            Err(MempoolAdmissionError::StaleNonce {
                current_nonce: 5,
                tx_nonce: 4,
            })
        );
    }

    #[test]
    fn rejects_nonce_gap_greater_than_one() {
        let tx = signed_transfer_tx(1, 7, 2, 1_000);
        let admission = MempoolAdmission::new(5, &[]);

        assert_eq!(
            admission.evaluate(&tx),
            Err(MempoolAdmissionError::NonceGap {
                current_nonce: 5,
                tx_nonce: 7,
            })
        );
    }

    #[test]
    fn rejects_duplicate_sender_nonce_without_replacement() {
        let existing = signed_transfer_tx(1, 8, 3, 1_000);
        let tx = signed_transfer_tx_with_amount(1, 8, 3, 1_000, 2);
        let pending = vec![existing];
        let admission = MempoolAdmission::new(8, &pending);

        assert_eq!(
            admission.evaluate(&tx),
            Err(MempoolAdmissionError::DuplicateSenderNonce {
                sender_pubkey: tx.sender_pubkey.clone(),
                nonce: 8,
                existing_tip: 3,
                new_tip: 3,
            })
        );
    }

    #[test]
    fn allows_replacement_with_strictly_higher_tip() {
        let existing = signed_transfer_tx(1, 8, 3, 1_000);
        let tx = signed_transfer_tx(1, 8, 4, 1_000);
        let evict_tx_id = canonical_tx_id(&existing);
        let pending = vec![existing];
        let admission = MempoolAdmission::new(8, &pending);

        assert_eq!(
            admission.evaluate(&tx),
            Ok(AdmissionDecision::Replace { evict_tx_id })
        );
    }

    #[test]
    fn rejects_replacement_with_lower_tip() {
        let existing = signed_transfer_tx(1, 8, 4, 1_000);
        let tx = signed_transfer_tx_with_amount(1, 8, 3, 1_000, 2);
        let pending = vec![existing];
        let admission = MempoolAdmission::new(8, &pending);

        assert_eq!(
            admission.evaluate(&tx),
            Err(MempoolAdmissionError::DuplicateSenderNonce {
                sender_pubkey: tx.sender_pubkey.clone(),
                nonce: 8,
                existing_tip: 4,
                new_tip: 3,
            })
        );
    }

    #[test]
    fn accepts_nonce_current_plus_one() {
        let tx = with_sender_nonce(1, 6, 2, 1_000);
        let admission = MempoolAdmission::new(5, &[]);

        assert_eq!(admission.evaluate(&tx), Ok(AdmissionDecision::Accept));
    }
}
