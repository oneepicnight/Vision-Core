use axum::{body::Bytes, extract::State, http::StatusCode, response::{IntoResponse, Response}, Json};
use serde::Serialize;

use crate::api::state::NodeApiState;
use crate::chain::ChainState;
use crate::mempool::{AdmissionDecision, Mempool, MempoolAdmissionError};
use crate::types::transaction::{canonical_tx_id, TxSignatureError, TxValidationError};
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct TransactionSubmissionHttpError {
    pub code: &'static str,
    pub message: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct TransactionAdmissionDecisionHttp {
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evict_tx_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct TransactionSubmissionHttpResponse {
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tx_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_nonce: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<TransactionAdmissionDecisionHttp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<TransactionSubmissionHttpError>,
}

impl TransactionSubmissionHttpResponse {
    fn accepted(tx_id: String, current_nonce: u64, decision: TransactionAdmissionDecisionHttp) -> Self {
        Self {
            status: "accepted",
            tx_id: Some(tx_id),
            current_nonce: Some(current_nonce),
            decision: Some(decision),
            error: None,
        }
    }

    fn rejected(tx_id: String, current_nonce: u64, error: TransactionSubmissionHttpError) -> Self {
        Self {
            status: "rejected",
            tx_id: Some(tx_id),
            current_nonce: Some(current_nonce),
            decision: None,
            error: Some(error),
        }
    }

    fn malformed_request() -> Self {
        Self {
            status: "malformed_request",
            tx_id: None,
            current_nonce: None,
            decision: None,
            error: Some(TransactionSubmissionHttpError {
                code: "malformed_request",
                message: "request body must be a canonical signed Tx JSON object",
            }),
        }
    }
}

pub(crate) async fn submit_transaction_http(
    State(state): State<NodeApiState>,
    body: Bytes,
) -> Response {
    let tx: Tx = match serde_json::from_slice(&body) {
        Ok(tx) => tx,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(TransactionSubmissionHttpResponse::malformed_request()),
            )
                .into_response();
        }
    };

    match state.submit_transaction(tx).await {
        TransactionSubmissionResult::Accepted(accepted) => (
            StatusCode::OK,
            Json(TransactionSubmissionHttpResponse::accepted(
                accepted.tx_id,
                accepted.current_nonce,
                transaction_admission_decision_http(accepted.decision),
            )),
        )
            .into_response(),
        TransactionSubmissionResult::Rejected(rejected) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(TransactionSubmissionHttpResponse::rejected(
                rejected.tx_id,
                rejected.current_nonce,
                transaction_submission_error_http(&rejected.error),
            )),
        )
            .into_response(),
    }
}

fn transaction_admission_decision_http(decision: AdmissionDecision) -> TransactionAdmissionDecisionHttp {
    match decision {
        AdmissionDecision::Accept => TransactionAdmissionDecisionHttp {
            kind: "accept",
            evict_tx_id: None,
        },
        AdmissionDecision::Replace { evict_tx_id } => TransactionAdmissionDecisionHttp {
            kind: "replace",
            evict_tx_id: Some(evict_tx_id),
        },
    }
}

fn transaction_submission_error_http(error: &MempoolAdmissionError) -> TransactionSubmissionHttpError {
    match error {
        MempoolAdmissionError::StatelessValidation(validation_error) => {
            tx_validation_error_http(validation_error)
        }
        MempoolAdmissionError::DuplicateCanonicalTxId => TransactionSubmissionHttpError {
            code: "duplicate_canonical_tx_id",
            message: "a transaction with the same canonical tx_id already exists in the mempool",
        },
        MempoolAdmissionError::StaleNonce { .. } => TransactionSubmissionHttpError {
            code: "stale_nonce",
            message: "transaction nonce is behind the sender's current canonical nonce",
        },
        MempoolAdmissionError::NonceGap { .. } => TransactionSubmissionHttpError {
            code: "nonce_gap",
            message: "transaction nonce skips ahead of the sender's current canonical nonce",
        },
        MempoolAdmissionError::DuplicateSenderNonce { .. } => TransactionSubmissionHttpError {
            code: "duplicate_sender_nonce",
            message: "a pending transaction for this sender and nonce already exists",
        },
    }
}

fn tx_validation_error_http(error: &TxValidationError) -> TransactionSubmissionHttpError {
    match error {
        TxValidationError::TxTooLarge => TransactionSubmissionHttpError {
            code: "tx_too_large",
            message: "serialized transaction exceeds 64 KiB",
        },
        TxValidationError::MissingSenderPubkey => TransactionSubmissionHttpError {
            code: "missing_sender_pubkey",
            message: "non-coinbase transactions require sender_pubkey",
        },
        TxValidationError::MissingSignature => TransactionSubmissionHttpError {
            code: "missing_signature",
            message: "non-coinbase transactions require sig",
        },
        TxValidationError::UnsupportedModuleMethod => TransactionSubmissionHttpError {
            code: "unsupported_module_method",
            message: "only supported module/method pairs may be submitted",
        },
        TxValidationError::BadTransferArgs => TransactionSubmissionHttpError {
            code: "bad_transfer_args",
            message: "cash::transfer args must decode as the canonical transfer payload",
        },
        TxValidationError::InvalidTransferDestination => TransactionSubmissionHttpError {
            code: "invalid_transfer_destination",
            message: "transfer destination must be a 64-character lowercase hex account key",
        },
        TxValidationError::TransferAmountZero => TransactionSubmissionHttpError {
            code: "transfer_amount_zero",
            message: "transfer amount must be greater than zero",
        },
        TxValidationError::TransferToSelf => TransactionSubmissionHttpError {
            code: "transfer_to_self",
            message: "transfer destination must differ from the sender",
        },
        TxValidationError::FeeLimitTooLow => TransactionSubmissionHttpError {
            code: "fee_limit_too_low",
            message: "fee_limit must be at least 201",
        },
        TxValidationError::Signature(signature_error) => tx_signature_error_http(signature_error),
    }
}

fn tx_signature_error_http(error: &TxSignatureError) -> TransactionSubmissionHttpError {
    match error {
        TxSignatureError::SenderPubkeyWrongLength => TransactionSubmissionHttpError {
            code: "sender_pubkey_wrong_length",
            message: "sender_pubkey must be 64 lowercase hex characters",
        },
        TxSignatureError::SenderPubkeyNotLowercaseHex => TransactionSubmissionHttpError {
            code: "sender_pubkey_not_lowercase_hex",
            message: "sender_pubkey must be lowercase hex",
        },
        TxSignatureError::SignatureWrongLength => TransactionSubmissionHttpError {
            code: "signature_wrong_length",
            message: "sig must be 128 lowercase hex characters",
        },
        TxSignatureError::SignatureNotLowercaseHex => TransactionSubmissionHttpError {
            code: "signature_not_lowercase_hex",
            message: "sig must be lowercase hex",
        },
        TxSignatureError::MalformedPublicKey => TransactionSubmissionHttpError {
            code: "malformed_public_key",
            message: "sender_pubkey is not a valid Ed25519 public key",
        },
        TxSignatureError::InvalidSignature => TransactionSubmissionHttpError {
            code: "invalid_signature",
            message: "signature verification failed",
        },
    }
}

/// Submit a canonical signed transaction to the local mempool.
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

#[cfg(test)]
mod http_tests {
    use super::*;
    use axum::{body::{self, Bytes}, extract::State, http::StatusCode};
    use ed25519_dalek::{Signer, SigningKey};
    use std::sync::Arc;
    use tokio::sync::Mutex;

    fn temp_state() -> ChainState {
        let db = sled::Config::new().temporary(true).open().unwrap();
        ChainState::empty(db)
    }

    fn signing_key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn transfer_args(to: &str, amount: u128) -> Vec<u8> {
        serde_json::to_vec(&crate::types::transaction::CashTransferArgs {
            to: to.to_string(),
            amount,
        })
        .unwrap()
    }

    fn sign_tx(mut tx: Tx, seed: u8) -> Tx {
        let signing_key = signing_key(seed);
        tx.sender_pubkey = hex::encode(signing_key.verifying_key().to_bytes());
        tx.sig.clear();
        let sig = signing_key.sign(&crate::types::transaction::canonical_unsigned_payload(&tx));
        tx.sig = hex::encode(sig.to_bytes());
        tx
    }

    fn signed_transfer_tx(seed: u8, nonce: u64, tip: u64) -> Tx {
        sign_tx(
            Tx {
                nonce,
                sender_pubkey: String::new(),
                module: "cash".to_string(),
                method: "transfer".to_string(),
                args: transfer_args(&"22".repeat(32), 1),
                tip,
                fee_limit: crate::types::transaction::MIN_CASH_TRANSFER_FEE_LIMIT,
                sig: String::new(),
            },
            seed,
        )
    }

    async fn submit_json(state: NodeApiState, body: &str) -> (StatusCode, String) {
        let response = submit_transaction_http(State(state), Bytes::from(body.to_owned())).await;
        let status = response.status();
        let bytes = body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, String::from_utf8(bytes.to_vec()).unwrap())
    }

    #[tokio::test]
    async fn valid_signed_tx_is_accepted_through_http() {
        let chain = Arc::new(Mutex::new(temp_state()));
        let mempool = Arc::new(Mempool::new());
        let state = NodeApiState::new(chain.clone(), mempool.clone());
        let tx = signed_transfer_tx(1, 0, 2);
        let tx_json = serde_json::to_string(&tx).unwrap();
        let tx_id = canonical_tx_id(&tx);

        let (status, body) = submit_json(state, &tx_json).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            format!(
                "{{\"status\":\"accepted\",\"tx_id\":\"{}\",\"current_nonce\":0,\"decision\":{{\"kind\":\"accept\"}}}}",
                tx_id
            )
        );
        assert!(mempool.has(&tx_id));
    }

    #[tokio::test]
    async fn invalid_signature_is_rejected_through_http() {
        let chain = Arc::new(Mutex::new(temp_state()));
        let mempool = Arc::new(Mempool::new());
        let state = NodeApiState::new(chain, mempool);
        let mut tx = signed_transfer_tx(1, 0, 2);
        tx.sig = "00".repeat(64);
        let tx_json = serde_json::to_string(&tx).unwrap();

        let (status, body) = submit_json(state, &tx_json).await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(body.contains("\"status\":\"rejected\""));
        assert!(body.contains("\"code\":\"invalid_signature\""));
    }

    #[tokio::test]
    async fn malformed_request_returns_structured_error() {
        let chain = Arc::new(Mutex::new(temp_state()));
        let mempool = Arc::new(Mempool::new());
        let state = NodeApiState::new(chain, mempool);

        let (status, body) = submit_json(state, "{").await;

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            body,
            "{\"status\":\"malformed_request\",\"error\":{\"code\":\"malformed_request\",\"message\":\"request body must be a canonical signed Tx JSON object\"}}"
        );
    }

    #[tokio::test]
    async fn stale_nonce_is_rejected_through_http() {
        let chain = Arc::new(Mutex::new(temp_state()));
        let mempool = Arc::new(Mempool::new());
        let tx = signed_transfer_tx(1, 4, 2);
        let tx_json = serde_json::to_string(&tx).unwrap();
        {
            let mut chain_guard = chain.lock().await;
            chain_guard.nonces.insert(tx.sender_pubkey.clone(), 5);
        }
        let state = NodeApiState::new(chain, mempool);

        let (status, body) = submit_json(state, &tx_json).await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(body.contains("\"code\":\"stale_nonce\""));
    }

    #[tokio::test]
    async fn duplicate_tx_is_rejected_through_http() {
        let chain = Arc::new(Mutex::new(temp_state()));
        let mempool = Arc::new(Mempool::new());
        let state = NodeApiState::new(chain, mempool);
        let tx = signed_transfer_tx(1, 0, 2);
        let tx_json = serde_json::to_string(&tx).unwrap();
        let tx_id = canonical_tx_id(&tx);

        let first = submit_json(state.clone(), &tx_json).await;
        assert_eq!(first.0, StatusCode::OK);

        let second = submit_json(state, &tx_json).await;
        assert_eq!(second.0, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(second.1.contains("\"code\":\"duplicate_canonical_tx_id\""));
        assert!(second.1.contains(&tx_id));
    }

    #[tokio::test]
    async fn response_schema_is_stable_for_accepted_and_rejected_results() {
        let chain = Arc::new(Mutex::new(temp_state()));
        let mempool = Arc::new(Mempool::new());
        let state = NodeApiState::new(chain, mempool);
        let tx = signed_transfer_tx(1, 0, 2);
        let tx_json = serde_json::to_string(&tx).unwrap();
        let tx_id = canonical_tx_id(&tx);

        let (status, accepted_body) = submit_json(state.clone(), &tx_json).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            accepted_body,
            format!(
                "{{\"status\":\"accepted\",\"tx_id\":\"{}\",\"current_nonce\":0,\"decision\":{{\"kind\":\"accept\"}}}}",
                tx_id
            )
        );

        let (status, rejected_body) = submit_json(state, &tx_json).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            rejected_body,
            format!(
                "{{\"status\":\"rejected\",\"tx_id\":\"{}\",\"current_nonce\":0,\"error\":{{\"code\":\"duplicate_canonical_tx_id\",\"message\":\"a transaction with the same canonical tx_id already exists in the mempool\"}}}}",
                tx_id
            )
        );
    }
}


#[cfg(test)]
mod http_state_tests {
    use super::*;
    use axum::{body::Bytes, extract::State};
    use ed25519_dalek::{Signer, SigningKey};
    use std::sync::Arc;
    use tokio::sync::Mutex;

    fn temp_state() -> ChainState {
        let db = sled::Config::new().temporary(true).open().unwrap();
        ChainState::empty(db)
    }

    fn signing_key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn transfer_args(to: &str, amount: u128) -> Vec<u8> {
        serde_json::to_vec(&crate::types::transaction::CashTransferArgs {
            to: to.to_string(),
            amount,
        })
        .unwrap()
    }

    fn signed_transfer_tx(seed: u8, nonce: u64, tip: u64) -> Tx {
        let signing_key = signing_key(seed);
        let mut tx = Tx {
            nonce,
            sender_pubkey: hex::encode(signing_key.verifying_key().to_bytes()),
            module: "cash".to_string(),
            method: "transfer".to_string(),
            args: transfer_args(&"22".repeat(32), 1),
            tip,
            fee_limit: crate::types::transaction::MIN_CASH_TRANSFER_FEE_LIMIT,
            sig: String::new(),
        };
        let sig = signing_key.sign(&crate::types::transaction::canonical_unsigned_payload(&tx));
        tx.sig = hex::encode(sig.to_bytes());
        tx
    }

    #[tokio::test]
    async fn rejected_submission_does_not_mutate_balances_or_nonces_through_http() {
        let chain = Arc::new(Mutex::new(temp_state()));
        let mempool = Arc::new(Mempool::new());
        let state = NodeApiState::new(chain.clone(), mempool);
        let mut tx = signed_transfer_tx(1, 0, 2);
        tx.sig = "00".repeat(64);
        let body = serde_json::to_string(&tx).unwrap();

        let before = {
            let chain_guard = chain.lock().await;
            (chain_guard.balances.clone(), chain_guard.nonces.clone())
        };

        let response = submit_transaction_http(State(state), Bytes::from(body)).await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

        let after = {
            let chain_guard = chain.lock().await;
            (chain_guard.balances.clone(), chain_guard.nonces.clone())
        };

        assert_eq!(after, before);
    }
}


