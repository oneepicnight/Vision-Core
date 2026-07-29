#[cfg(test)]
mod tests {
    use crate::genesis::genesis::{genesis_block, validate_genesis_hash};

    #[test]
    fn genesis_hash_validates() {
        validate_genesis_hash().expect("genesis hash must be valid");
    }

    #[test]
    fn genesis_block_pow_hash_matches_constant() {
        let block = genesis_block();
        assert_eq!(
            block.hash(),
            crate::genesis::genesis::GENESIS_HASH,
            "genesis block pow_hash must equal GENESIS_HASH constant"
        );
    }

    #[test]
    fn genesis_block_shape() {
        let block = genesis_block();
        assert_eq!(block.height(), 0);
        assert!(block.txs.is_empty());
        assert_eq!(block.header.difficulty, 1);
        assert_eq!(block.header.number, 0);
    }
}
