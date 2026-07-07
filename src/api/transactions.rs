use crate::chain::ChainState;
use crate::mempool::{AdmissionDecision, Mempool, MempoolAdmissionError};
use crate::types::transaction::canonical_tx_id;
use crate::types::Tx;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TransactionSubmissionAccepted {
    pub tx_id: String,
    pub current_nonce: u64,
    pub decision: AdmissionDecision,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TransactionSubmissionRejected {
    pub tx_id: String,
    pub current_nonce: u64,
    pub error: MempoolAdmissionError,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TransactionSubmissionResult {
    Accepted(TransactionSubmissionAccepted),
    Rejected(TransactionSubmissionRejected),
}

/// Submit a canonical signed transaction to the local mempool.
///
/// This is the internal API boundary for transaction submission. It reads the
/// sender's current canonical nonce from chain state, delegates all admission
/// checks to the canonical mempool policy, and never mutates balances or nonces.
pub(crate) fn submit_transaction(
    chain: &ChainState,
    mempool: &Mempool,
    tx: Tx,
) -> TransactionSubmissionResult {
    let tx_id = canonical_tx_id(&tx);
    let current_nonce = chain.nonce_of(&tx.sender_pubkey);

    match mempool.admit(tx, current_nonce) {
        Ok(decision) => TransactionSubmissionResult::Accepted(TransactionSubmissionAccepted {
            tx_id,
            current_nonce,
            decision,
        }),
        Err(error) => TransactionSubmissionResult::Rejected(TransactionSubmissionRejected {
            tx_id,
            current_nonce,
            error,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mempool::MempoolAdmissionError;
    use crate::types::transaction::{
        canonical_unsigned_payload, CashTransferArgs, TxSignatureError, TxValidationError,
        MIN_CASH_TRANSFER_FEE_LIMIT,
    };
    use ed25519_dalek::{Signer, SigningKey};

    fn temp_state() -> ChainState {
        let db = sled::Config::new().temporary(true).open().unwrap();
        ChainState::empty(db)
    }

    fn signing_key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn transfer_args(to: &str, amount: u128) -> Vec<u8> {
        serde_json::to_vec(&CashTransferArgs {
            to: to.to_string(),
            amount,
        })
        .unwrap()
    }

    fn sign_tx(mut tx: Tx, seed: u8) -> Tx {
        let signing_key = signing_key(seed);
        tx.sender_pubkey = hex::encode(signing_key.verifying_key().to_bytes());
        tx.sig.clear();
        let sig = signing_key.sign(&canonical_unsigned_payload(&tx));
        tx.sig = hex::encode(sig.to_bytes());
        tx
    }

    fn signed_transfer_tx_with(
        seed: u8,
        nonce: u64,
        to: &str,
        amount: u128,
        tip: u64,
        fee_limit: u64,
    ) -> Tx {
        sign_tx(
            Tx {
                nonce,
                sender_pubkey: String::new(),
                module: "cash".to_string(),
                method: "transfer".to_string(),
                args: transfer_args(to, amount),
                tip,
                fee_limit,
                sig: String::new(),
            },
            seed,
        )
    }

    fn signed_transfer_tx(seed: u8, nonce: u64, tip: u64, amount: u128) -> Tx {
        signed_transfer_tx_with(
            seed,
            nonce,
            &"22".repeat(32),
            amount,
            tip,
            MIN_CASH_TRANSFER_FEE_LIMIT,
        )
    }

    fn assert_rejected_with(result: TransactionSubmissionResult, expected: MempoolAdmissionError) {
        match result {
            TransactionSubmissionResult::Rejected(rejected) => {
                assert_eq!(rejected.error, expected);
            }
            other => panic!("expected rejected submission, got {:?}", other),
        }
    }

    #[test]
    fn valid_signed_cash_transfer_is_accepted_into_mempool() {
        let chain = temp_state();
        let mempool = Mempool::new();
        let tx = signed_transfer_tx(1, 0, 2, 1);
        let tx_id = canonical_tx_id(&tx);

        let result = submit_transaction(&chain, &mempool, tx.clone());

        assert_eq!(
            result,
            TransactionSubmissionResult::Accepted(TransactionSubmissionAccepted {
                tx_id: tx_id.clone(),
                current_nonce: 0,
                decision: AdmissionDecision::Accept,
            })
        );
        assert!(mempool.has(&tx_id));
        assert_eq!(mempool.get(&tx_id), Some(tx));
    }

    #[test]
    fn invalid_signature_is_rejected() {
        let chain = temp_state();
        let mempool = Mempool::new();
        let mut tx = signed_transfer_tx(1, 0, 2, 1);
        tx.sig = "00".repeat(64);

        let result = submit_transaction(&chain, &mempool, tx);

        assert!(matches!(
            result,
            TransactionSubmissionResult::Rejected(TransactionSubmissionRejected {
                error: MempoolAdmissionError::StatelessValidation(TxValidationError::Signature(
                    TxSignatureError::InvalidSignature
                )),
                current_nonce: 0,
                ..
            })
        ));
        assert!(mempool.is_empty());
    }

    #[test]
    fn malformed_sender_or_signature_is_rejected() {
        let chain = temp_state();
        let mempool = Mempool::new();
        let mut bad_sender = signed_transfer_tx(1, 0, 2, 1);
        bad_sender.sender_pubkey = "aa".repeat(31);

        let result = submit_transaction(&chain, &mempool, bad_sender);
        assert!(matches!(
            result,
            TransactionSubmissionResult::Rejected(TransactionSubmissionRejected {
                error: MempoolAdmissionError::StatelessValidation(TxValidationError::Signature(
                    TxSignatureError::SenderPubkeyWrongLength
                )),
                ..
            })
        ));
        assert!(mempool.is_empty());

        let mut bad_sig = signed_transfer_tx(1, 0, 2, 1);
        bad_sig.sig = "11".repeat(63);

        let result = submit_transaction(&chain, &mempool, bad_sig);
        assert!(matches!(
            result,
            TransactionSubmissionResult::Rejected(TransactionSubmissionRejected {
                error: MempoolAdmissionError::StatelessValidation(TxValidationError::Signature(
                    TxSignatureError::SignatureWrongLength
                )),
                ..
            })
        ));
        assert!(mempool.is_empty());
    }

    #[test]
    fn stale_nonce_is_rejected_using_chain_nonce() {
        let mut chain = temp_state();
        let mempool = Mempool::new();
        let tx = signed_transfer_tx(1, 4, 2, 1);
        chain.nonces.insert(tx.sender_pubkey.clone(), 5);

        let result = submit_transaction(&chain, &mempool, tx);

        assert_rejected_with(
            result,
            MempoolAdmissionError::StaleNonce {
                current_nonce: 5,
                tx_nonce: 4,
            },
        );
        assert!(mempool.is_empty());
    }

    #[test]
    fn nonce_gap_is_rejected_using_chain_nonce() {
        let mut chain = temp_state();
        let mempool = Mempool::new();
        let tx = signed_transfer_tx(1, 7, 2, 1);
        chain.nonces.insert(tx.sender_pubkey.clone(), 5);

        let result = submit_transaction(&chain, &mempool, tx);

        assert_rejected_with(
            result,
            MempoolAdmissionError::NonceGap {
                current_nonce: 5,
                tx_nonce: 7,
            },
        );
        assert!(mempool.is_empty());
    }

    #[test]
    fn duplicate_canonical_tx_id_is_rejected() {
        let chain = temp_state();
        let mempool = Mempool::new();
        let tx = signed_transfer_tx(1, 0, 2, 1);

        assert!(matches!(
            submit_transaction(&chain, &mempool, tx.clone()),
            TransactionSubmissionResult::Accepted(_)
        ));
        let result = submit_transaction(&chain, &mempool, tx);

        assert_rejected_with(result, MempoolAdmissionError::DuplicateCanonicalTxId);
        assert_eq!(mempool.len(), 1);
    }

    #[test]
    fn duplicate_sender_nonce_is_rejected_without_replacement() {
        let chain = temp_state();
        let mempool = Mempool::new();
        let existing = signed_transfer_tx(1, 0, 3, 1);
        let replacement = signed_transfer_tx(1, 0, 3, 2);

        assert!(matches!(
            submit_transaction(&chain, &mempool, existing),
            TransactionSubmissionResult::Accepted(_)
        ));
        let result = submit_transaction(&chain, &mempool, replacement.clone());

        assert_rejected_with(
            result,
            MempoolAdmissionError::DuplicateSenderNonce {
                sender_pubkey: replacement.sender_pubkey,
                nonce: 0,
                existing_tip: 3,
                new_tip: 3,
            },
        );
        assert_eq!(mempool.len(), 1);
    }

    #[test]
    fn higher_tip_replacement_is_accepted() {
        let chain = temp_state();
        let mempool = Mempool::new();
        let existing = signed_transfer_tx(1, 0, 3, 1);
        let replacement = signed_transfer_tx(1, 0, 4, 1);
        let old_id = canonical_tx_id(&existing);
        let new_id = canonical_tx_id(&replacement);

        assert!(matches!(
            submit_transaction(&chain, &mempool, existing),
            TransactionSubmissionResult::Accepted(_)
        ));
        let result = submit_transaction(&chain, &mempool, replacement.clone());

        assert_eq!(
            result,
            TransactionSubmissionResult::Accepted(TransactionSubmissionAccepted {
                tx_id: new_id.clone(),
                current_nonce: 0,
                decision: AdmissionDecision::Replace {
                    evict_tx_id: old_id.clone(),
                },
            })
        );
        assert_eq!(mempool.len(), 1);
        assert!(!mempool.has(&old_id));
        assert_eq!(mempool.get(&new_id), Some(replacement));
    }

    #[test]
    fn equal_or_lower_tip_replacement_is_rejected() {
        let chain = temp_state();
        let equal_tip_pool = Mempool::new();
        let existing = signed_transfer_tx(1, 0, 3, 1);
        let equal_tip = signed_transfer_tx(1, 0, 3, 2);

        assert!(matches!(
            submit_transaction(&chain, &equal_tip_pool, existing),
            TransactionSubmissionResult::Accepted(_)
        ));
        assert_rejected_with(
            submit_transaction(&chain, &equal_tip_pool, equal_tip.clone()),
            MempoolAdmissionError::DuplicateSenderNonce {
                sender_pubkey: equal_tip.sender_pubkey,
                nonce: 0,
                existing_tip: 3,
                new_tip: 3,
            },
        );
        assert_eq!(equal_tip_pool.len(), 1);

        let lower_tip_pool = Mempool::new();
        let existing = signed_transfer_tx(1, 0, 3, 1);
        let lower_tip = signed_transfer_tx(1, 0, 2, 2);
        assert!(matches!(
            submit_transaction(&chain, &lower_tip_pool, existing),
            TransactionSubmissionResult::Accepted(_)
        ));
        assert_rejected_with(
            submit_transaction(&chain, &lower_tip_pool, lower_tip.clone()),
            MempoolAdmissionError::DuplicateSenderNonce {
                sender_pubkey: lower_tip.sender_pubkey,
                nonce: 0,
                existing_tip: 3,
                new_tip: 2,
            },
        );
        assert_eq!(lower_tip_pool.len(), 1);
    }

    #[test]
    fn fee_limit_below_201_is_rejected() {
        let chain = temp_state();
        let mempool = Mempool::new();
        let tx = signed_transfer_tx_with(1, 0, &"22".repeat(32), 1, 2, 200);

        let result = submit_transaction(&chain, &mempool, tx);

        assert_rejected_with(
            result,
            MempoolAdmissionError::StatelessValidation(TxValidationError::FeeLimitTooLow),
        );
        assert!(mempool.is_empty());
    }

    #[test]
    fn rejected_submission_does_not_mutate_balances_or_nonces() {
        let mut chain = temp_state();
        let mempool = Mempool::new();
        let tx = signed_transfer_tx(1, 4, 2, 1);
        chain.balances.insert(tx.sender_pubkey.clone(), 1_000);
        chain.nonces.insert(tx.sender_pubkey.clone(), 5);
        let before_balances = chain.balances.clone();
        let before_nonces = chain.nonces.clone();

        let result = submit_transaction(&chain, &mempool, tx);

        assert!(matches!(
            result,
            TransactionSubmissionResult::Rejected(TransactionSubmissionRejected {
                error: MempoolAdmissionError::StaleNonce { .. },
                ..
            })
        ));
        assert_eq!(chain.balances, before_balances);
        assert_eq!(chain.nonces, before_nonces);
        assert!(mempool.is_empty());
    }
}
