use crate::types::BlockHeader;

const VPOW_MAGIC: &[u8; 4] = b"VPOW";
const VPOW_VERSION: u32 = 1;

fn decode_hash_32(label: &str, s: &str) -> Result<[u8; 32], String> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    let bytes = hex::decode(s).map_err(|e| format!("{label}: invalid hex: {e}"))?;
    if bytes.len() != 32 {
        return Err(format!("{label}: expected 32 bytes, got {}", bytes.len()));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn write_le_u32(buf: &mut Vec<u8>, value: u32) {
    buf.extend_from_slice(&value.to_le_bytes());
}

fn write_le_u64(buf: &mut Vec<u8>, value: u64) {
    buf.extend_from_slice(&value.to_le_bytes());
}

fn write_be_u64(buf: &mut Vec<u8>, value: u64) {
    buf.extend_from_slice(&value.to_be_bytes());
}

/// Build the historical Vision PoW preimage used by `vision-node`.
///
/// This matches `vision-node::consensus_pow::encoding::pow_message_bytes`.
/// It is intentionally isolated from the current validation path.
pub fn historical_vpow_message_bytes(header: &BlockHeader) -> Result<Vec<u8>, String> {
    let parent_hash = decode_hash_32("parent_hash", &header.parent_hash)?;
    let tx_root = decode_hash_32("tx_root", &header.tx_root)?;
    let miner_bytes = header.miner.as_bytes();

    let mut out = Vec::with_capacity(108 + miner_bytes.len());
    out.extend_from_slice(VPOW_MAGIC);
    write_le_u32(&mut out, VPOW_VERSION);
    out.extend_from_slice(&parent_hash);
    write_le_u64(&mut out, header.number);
    write_le_u64(&mut out, header.timestamp);
    write_le_u64(&mut out, header.difficulty);
    write_be_u64(&mut out, header.nonce);
    out.extend_from_slice(&tx_root);
    write_le_u32(&mut out, miner_bytes.len() as u32);
    out.extend_from_slice(miner_bytes);

    Ok(out)
}

/// Build the historical preimage used by the active miner/validator split.
///
/// The historical pipeline zeroed the header nonce before encoding and passed
/// the tested nonce separately into the VisionX hash function.
pub fn historical_vpow_message_bytes_with_nonce_zero(
    header: &BlockHeader,
) -> Result<Vec<u8>, String> {
    let mut cloned = header.clone();
    cloned.nonce = 0;
    historical_vpow_message_bytes(&cloned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_header() -> BlockHeader {
        BlockHeader {
            parent_hash: "0x".to_string() + &"11".repeat(32),
            number: 12_345,
            timestamp: 1_700_000_000,
            difficulty: 1_000,
            nonce: 42,
            pow_hash: String::new(),
            state_root: "0x".to_string() + &"22".repeat(32),
            tx_root: "0x".to_string() + &"33".repeat(32),
            miner: "pow_miner".to_string(),
        }
    }

    #[test]
    fn exact_byte_layout_matches_documented_vector() {
        let mut header = sample_header();
        header.nonce = 0;

        let bytes = historical_vpow_message_bytes(&header).expect("encoding should succeed");
        assert_eq!(bytes.len(), 117);
        assert_eq!(
            hex::encode(bytes),
            "56504f57010000001111111111111111111111111111111111111111111111111111111111111111393000000000000000f1536500000000e8030000000000000000000000000000333333333333333333333333333333333333333333333333333333333333333309000000706f775f6d696e6572"
        );
    }

    #[test]
    fn deterministic_output_is_stable() {
        let header = sample_header();
        let a = historical_vpow_message_bytes(&header).expect("encoding should succeed");
        let b = historical_vpow_message_bytes(&header).expect("encoding should succeed");
        assert_eq!(a, b);
    }

    #[test]
    fn field_ordering_matches_vision_node_layout() {
        let header = BlockHeader {
            parent_hash: "0x".to_string() + &"aa".repeat(32),
            number: 0x0102_0304_0506_0708,
            timestamp: 0x1112_1314_1516_1718,
            difficulty: 0x2122_2324_2526_2728,
            nonce: 0x3132_3334_3536_3738,
            pow_hash: String::new(),
            state_root: "0x".to_string() + &"44".repeat(32),
            tx_root: "0x".to_string() + &"55".repeat(32),
            miner: "miner".to_string(),
        };

        let bytes = historical_vpow_message_bytes(&header).expect("encoding should succeed");
        assert_eq!(&bytes[0..4], b"VPOW");
        assert_eq!(&bytes[4..8], &1u32.to_le_bytes());
        assert_eq!(&bytes[8..40], &[0xaa; 32]);
        assert_eq!(&bytes[40..48], &0x0102_0304_0506_0708u64.to_le_bytes());
        assert_eq!(&bytes[48..56], &0x1112_1314_1516_1718u64.to_le_bytes());
        assert_eq!(&bytes[56..64], &0x2122_2324_2526_2728u64.to_le_bytes());
        assert_eq!(&bytes[64..72], &0x3132_3334_3536_3738u64.to_be_bytes());
        assert_eq!(&bytes[72..104], &[0x55; 32]);
        assert_eq!(&bytes[104..108], &5u32.to_le_bytes());
        assert_eq!(&bytes[108..], b"miner");
    }

    #[test]
    fn nonce_is_handled_separately_for_mining_path() {
        let mut header = sample_header();
        header.nonce = 42;

        let mut zero_header = sample_header();
        zero_header.nonce = 0;

        let raw = historical_vpow_message_bytes(&header).expect("encoding should succeed");
        let zeroed = historical_vpow_message_bytes_with_nonce_zero(&header)
            .expect("zero-nonce encoding should succeed");
        let expected_zero = historical_vpow_message_bytes(&zero_header)
            .expect("zero-header encoding should succeed");

        assert_ne!(raw, zeroed);
        assert_eq!(zeroed, expected_zero);
    }

    #[test]
    fn compatibility_with_documented_historical_behavior() {
        let header = sample_header();
        let bytes = historical_vpow_message_bytes_with_nonce_zero(&header)
            .expect("encoding should succeed");
        assert_eq!(bytes.len(), 117);
        assert_eq!(
            hex::encode(bytes),
            "56504f57010000001111111111111111111111111111111111111111111111111111111111111111393000000000000000f1536500000000e8030000000000000000000000000000333333333333333333333333333333333333333333333333333333333333333309000000706f775f6d696e6572"
        );
    }
}
