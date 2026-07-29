use std::collections::BTreeMap;

const STATE_ROOT_MAGIC: &[u8; 6] = b"VSTATE";
const STATE_ROOT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StateRootError {
    MalformedAccountKey,
    MixedCaseAccountKey,
}

fn is_lower_hex_byte(byte: u8) -> bool {
    matches!(byte, b'0'..=b'9' | b'a'..=b'f')
}

fn validate_account_key(key: &str) -> Result<(), StateRootError> {
    if key.len() != 64 {
        return Err(StateRootError::MalformedAccountKey);
    }

    let mut saw_uppercase = false;
    let mut saw_non_lower_hex = false;
    for byte in key.as_bytes() {
        if matches!(byte, b'A'..=b'F') {
            saw_uppercase = true;
        } else if !is_lower_hex_byte(*byte) {
            saw_non_lower_hex = true;
        }
    }

    if saw_uppercase {
        return Err(StateRootError::MixedCaseAccountKey);
    }
    if saw_non_lower_hex {
        return Err(StateRootError::MalformedAccountKey);
    }

    Ok(())
}

fn decode_account_key(key: &str) -> Result<[u8; 32], StateRootError> {
    validate_account_key(key)?;
    hex::decode(key)
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or(StateRootError::MalformedAccountKey)
}

fn encode_header(out: &mut Vec<u8>) {
    out.extend_from_slice(STATE_ROOT_MAGIC);
    out.extend_from_slice(&STATE_ROOT_VERSION.to_le_bytes());
}

fn append_balances(
    out: &mut Vec<u8>,
    balances: &BTreeMap<String, u128>,
) -> Result<(), StateRootError> {
    let mut entries: Vec<_> = balances
        .iter()
        .filter(|(_, amount)| **amount != 0)
        .collect();
    entries.sort_by(|(left_key, _), (right_key, _)| left_key.cmp(right_key));

    out.extend_from_slice(&(entries.len() as u64).to_le_bytes());
    for (key, amount) in entries {
        let decoded = decode_account_key(key)?;
        out.extend_from_slice(&decoded);
        out.extend_from_slice(&amount.to_le_bytes());
    }

    Ok(())
}

fn append_nonces(out: &mut Vec<u8>, nonces: &BTreeMap<String, u64>) -> Result<(), StateRootError> {
    let mut entries: Vec<_> = nonces.iter().filter(|(_, nonce)| **nonce != 0).collect();
    entries.sort_by(|(left_key, _), (right_key, _)| left_key.cmp(right_key));

    out.extend_from_slice(&(entries.len() as u64).to_le_bytes());
    for (key, nonce) in entries {
        let decoded = decode_account_key(key)?;
        out.extend_from_slice(&decoded);
        out.extend_from_slice(&nonce.to_le_bytes());
    }

    Ok(())
}

pub(crate) fn canonical_state_vector(
    balances: &BTreeMap<String, u128>,
    nonces: &BTreeMap<String, u64>,
) -> Result<Vec<u8>, StateRootError> {
    let mut out = Vec::with_capacity(6 + 4 + 8 + 8 + (balances.len() * 48) + (nonces.len() * 40));
    encode_header(&mut out);
    append_balances(&mut out, balances)?;
    append_nonces(&mut out, nonces)?;
    Ok(out)
}

pub(crate) fn compute_state_root(
    balances: &BTreeMap<String, u128>,
    nonces: &BTreeMap<String, u64>,
) -> Result<String, StateRootError> {
    let vector = canonical_state_vector(balances, nonces)?;
    Ok(hex::encode(blake3::hash(&vector).as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_maps() -> (BTreeMap<String, u128>, BTreeMap<String, u64>) {
        (BTreeMap::new(), BTreeMap::new())
    }

    const EMPTY_VECTOR_HEX: &str = "5653544154450100000000000000000000000000000000000000";
    const EMPTY_ROOT: &str = "defb0e37d7153dc801bd060815d4aaad84b39d9fe09ac54884f0ef16c318a58e";
    const POST_VECTOR_HEX: &str = concat!(
        "565354415445010000000200000000000000",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "39000000000000000000000000000000",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "28000000000000000000000000000000",
        "0100000000000000",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "0100000000000000",
    );
    const POST_ROOT: &str = "213da678f84b492823e6398db193c03ca466908a4135358c820eb86eac8d0448";

    #[test]
    fn empty_state_vector_matches_decision_record() {
        let (balances, nonces) = empty_maps();
        let vector = canonical_state_vector(&balances, &nonces).unwrap();
        assert_eq!(hex::encode(vector), EMPTY_VECTOR_HEX);
    }

    #[test]
    fn empty_state_root_matches_decision_record() {
        let (balances, nonces) = empty_maps();
        assert_eq!(compute_state_root(&balances, &nonces).unwrap(), EMPTY_ROOT);
    }

    #[test]
    fn post_block_state_vector_matches_decision_record() {
        let balances = BTreeMap::from([
            (
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
                57,
            ),
            (
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
                40,
            ),
        ]);
        let nonces = BTreeMap::from([(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            1,
        )]);
        let vector = canonical_state_vector(&balances, &nonces).unwrap();
        assert_eq!(hex::encode(vector), POST_VECTOR_HEX);
    }

    #[test]
    fn post_block_state_root_matches_decision_record() {
        let balances = BTreeMap::from([
            (
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
                57,
            ),
            (
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
                40,
            ),
        ]);
        let nonces = BTreeMap::from([(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            1,
        )]);
        assert_eq!(compute_state_root(&balances, &nonces).unwrap(), POST_ROOT);
    }

    #[test]
    fn insertion_order_does_not_change_vector_or_root() {
        let mut balances_a = BTreeMap::new();
        balances_a.insert(
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
            40,
        );
        balances_a.insert(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            57,
        );

        let mut balances_b = BTreeMap::new();
        balances_b.insert(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            57,
        );
        balances_b.insert(
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
            40,
        );

        let mut nonces_a = BTreeMap::new();
        nonces_a.insert(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            1,
        );

        let mut nonces_b = BTreeMap::new();
        nonces_b.insert(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            1,
        );

        assert_eq!(
            canonical_state_vector(&balances_a, &nonces_a).unwrap(),
            canonical_state_vector(&balances_b, &nonces_b).unwrap(),
        );
        assert_eq!(
            compute_state_root(&balances_a, &nonces_a).unwrap(),
            compute_state_root(&balances_b, &nonces_b).unwrap(),
        );
    }

    #[test]
    fn zero_entries_are_elided() {
        let mut balances = BTreeMap::new();
        balances.insert(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            0,
        );
        balances.insert(
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
            40,
        );

        let mut nonces = BTreeMap::new();
        nonces.insert(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            0,
        );
        nonces.insert(
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
            1,
        );

        let vector = canonical_state_vector(&balances, &nonces).unwrap();
        let mut expected = Vec::new();
        expected.extend_from_slice(STATE_ROOT_MAGIC);
        expected.extend_from_slice(&STATE_ROOT_VERSION.to_le_bytes());
        expected.extend_from_slice(&1u64.to_le_bytes());
        expected.extend_from_slice(
            &hex::decode("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
                .unwrap(),
        );
        expected.extend_from_slice(&40u128.to_le_bytes());
        expected.extend_from_slice(&1u64.to_le_bytes());
        expected.extend_from_slice(
            &hex::decode("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
                .unwrap(),
        );
        expected.extend_from_slice(&1u64.to_le_bytes());

        assert_eq!(vector, expected);
    }

    #[test]
    fn malformed_account_key_is_rejected() {
        let mut balances = BTreeMap::new();
        balances.insert("not-hex".to_string(), 1);
        let (_, nonces) = empty_maps();
        assert_eq!(
            canonical_state_vector(&balances, &nonces),
            Err(StateRootError::MalformedAccountKey),
        );
    }

    #[test]
    fn mixed_case_account_key_is_rejected() {
        let mut balances = BTreeMap::new();
        balances.insert(
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string(),
            1,
        );
        let (_, nonces) = empty_maps();
        assert_eq!(
            canonical_state_vector(&balances, &nonces),
            Err(StateRootError::MixedCaseAccountKey),
        );
    }

    #[test]
    fn balance_changes_affect_root() {
        let mut balances = BTreeMap::new();
        balances.insert(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            57,
        );
        let nonces = BTreeMap::new();

        let root1 = compute_state_root(&balances, &nonces).unwrap();
        balances.insert(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            58,
        );
        let root2 = compute_state_root(&balances, &nonces).unwrap();
        assert_ne!(root1, root2);
    }

    #[test]
    fn nonce_changes_affect_root() {
        let balances = BTreeMap::from([(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            57,
        )]);
        let mut nonces = BTreeMap::new();
        nonces.insert(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            1,
        );
        let root1 = compute_state_root(&balances, &nonces).unwrap();
        nonces.insert(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            2,
        );
        let root2 = compute_state_root(&balances, &nonces).unwrap();
        assert_ne!(root1, root2);
    }
}
