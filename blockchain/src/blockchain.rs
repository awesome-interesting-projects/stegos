//! Blockchain definition.

//
// Copyright (c) 2018 Stegos AG
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

use crate::awards::{Awards, ValidatorAwardState};
use crate::block::*;
use crate::config::*;
use crate::election::mix;
use crate::election::ElectionInfo;
use crate::election::{self, ElectionResult};
use crate::error::*;
use crate::escrow::*;
use crate::metrics;
use crate::mvcc::MultiVersionedMap;
use crate::output::*;
use crate::timestamp::Timestamp;
use crate::transaction::{CoinbaseTransaction, ServiceAwardTransaction, Transaction};
use crate::view_changes::ViewChangeProof;
use bitvector::BitVector;
use byteorder::{BigEndian, ByteOrder};
use log::*;
use rocksdb;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use stegos_crypto::bulletproofs::fee_a;
use stegos_crypto::hash::*;
use stegos_crypto::pbc::VRF;
use stegos_crypto::scc::{Fr, Pt, PublicKey, SecretKey};
use stegos_crypto::vdf::VDF;
use stegos_crypto::{pbc, scc};
use stegos_serialization::traits::ProtoConvert;

pub type ViewCounter = u32;
pub type ValidatorId = u32;

/// Information of current chain, that is used as proof of viewchange.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChainInfo {
    pub epoch: u64,
    pub offset: u32,
    pub view_change: ViewCounter,
    pub last_block: Hash,
}

impl ChainInfo {
    /// Create ChainInfo from micro block.
    /// ## Panics
    /// if view_change is equal to 0
    pub fn from_micro_block(micro_block: &MicroBlock) -> Self {
        assert_ne!(micro_block.header.view_change, 0);
        ChainInfo {
            epoch: micro_block.header.epoch,
            offset: micro_block.header.offset,
            view_change: micro_block.header.view_change - 1,
            last_block: micro_block.header.previous,
        }
    }

    /// Create ChainInfo from blockchain.
    pub fn from_blockchain(blockchain: &Blockchain) -> Self {
        ChainInfo {
            epoch: blockchain.epoch(),
            offset: blockchain.offset(),
            view_change: blockchain.view_change(),
            last_block: blockchain.last_block_hash(),
        }
    }
}

impl Hashable for ChainInfo {
    fn hash(&self, hasher: &mut Hasher) {
        self.epoch.hash(hasher);
        self.offset.hash(hasher);
        self.view_change.hash(hasher);
        self.last_block.hash(hasher);
    }
}

/// A helper to find UTXO in this blockchain.
#[derive(Debug, Clone)]
enum OutputKey {
    MacroBlock {
        /// Block Epoch.
        epoch: u64,
        /// Output number.
        output_id: u32,
    },
    MicroBlock {
        /// Block Epoch.
        epoch: u64,
        /// Block Height.
        offset: u32,
        /// Transaction number.
        tx_id: u32,
        /// Output number.
        txout_id: u32,
    },
}

/// A helper to store the global monetary balance in MultiVersionedMap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Balance {
    /// The total sum of money created.
    pub created: Pt,
    /// The total sum of money burned.
    pub burned: Pt,
    /// The total sum of gamma adjustments.
    pub gamma: Fr,
    /// The total sum of block rewards.
    pub block_reward: i64,
}

/// A special offset used to tore Macro Blocks on the disk.
const MACRO_BLOCK_OFFSET: u32 = 4294967295u32;

#[derive(Debug, Default, Clone, Copy, PartialOrd, Ord, PartialEq, Eq)]
pub(crate) struct LSN(pub(crate) u64, pub(crate) u32); // use `struct` to disable explicit casts.
const INITIAL_LSN: LSN = LSN(0, 0);

type BlockByHashMap = MultiVersionedMap<Hash, LSN, LSN>;
type OutputByHashMap = MultiVersionedMap<Hash, OutputKey, LSN>;
type BalanceMap = MultiVersionedMap<(), Balance, LSN>;

type ElectionResultList = MultiVersionedMap<(), ElectionResult, LSN>;
type ValidatorsActivity = MultiVersionedMap<pbc::PublicKey, ValidatorAwardState, LSN>;

#[derive(Eq, PartialEq, Debug, Clone)]
pub struct OutputRecovery {
    pub output: Output,
    pub epoch: u64,
    pub is_final: bool,
    pub timestamp: Timestamp,
}

pub type AccountRecoveryState = Vec<OutputRecovery>;

/// The blockchain database.
pub struct Blockchain {
    //
    // Configuration.
    //
    cfg: ChainConfig,

    //
    // Storage.
    //
    /// Persistent storage for blocks.
    database: rocksdb::DB,
    /// In-memory index to lookup blocks by its hash.
    block_by_hash: BlockByHashMap,
    /// In-memory index to lookup UTXO by its hash.
    output_by_hash: OutputByHashMap,
    /// Global monetary balance.
    balance: BalanceMap,
    /// In-memory storage of stakes.
    escrow: Escrow,
    /// VDF state.
    pub(crate) vdf: VDF,
    /// VDF difficulty,
    difficulty: u64,

    //
    // Epoch Information.
    //
    /// Monotonically increasing 1-indexed identifier of the current epoch.
    /// Equals to the number of macro blocks in the blockchain.
    /// 1-based indexed - the genetic macro block starts epoch #1.
    epoch: u64,
    /// Current block number within the epoch.
    offset: u32,
    /// Last election result.
    election_result: ElectionResultList,
    /// Last saved view change, if view change was happen at current offset,
    /// and we was not leader.
    view_change_proof: Option<ViewChangeProof>,
    /// A timestamp from the last macro block.
    last_macro_block_timestamp: Timestamp,
    /// Copy of the last macro block hash.
    last_macro_block_hash: Hash,
    /// Copy of the last macro block random.
    last_macro_block_random: Hash,
    /// A timestamp from the last block.
    last_block_timestamp: Timestamp,
    /// Copy of the last block hash.
    last_block_hash: Hash,

    //
    // Service awards.
    //
    awards: Awards,
    epoch_activity: ValidatorsActivity,
}

impl Blockchain {
    //----------------------------------------------------------------------------------------------
    // Constructors.
    //----------------------------------------------------------------------------------------------

    pub fn new(
        cfg: ChainConfig,
        chain_dir: &Path,
        force_check: bool,
        genesis: MacroBlock,
        timestamp: Timestamp,
    ) -> Result<Blockchain, BlockchainError> {
        //
        // Storage.
        //
        let database = rocksdb::DB::open_default(chain_dir)?;
        let block_by_hash: BlockByHashMap = BlockByHashMap::new();
        let output_by_hash: OutputByHashMap = OutputByHashMap::new();
        let mut balance: BalanceMap = BalanceMap::new();
        let initial_balance = Balance {
            created: Pt::identity(),
            burned: Pt::identity(),
            gamma: Fr::zero(),
            block_reward: 0,
        };
        balance.insert(INITIAL_LSN, (), initial_balance);
        let escrow = Escrow::new();
        let difficulty = genesis.header.difficulty;
        let vdf = VDF::new();

        //
        // Epoch Information.
        //
        let epoch: u64 = 0;
        let offset: u32 = 0;
        let view_change_proof = None;
        let election_result = ElectionResultList::new();
        let last_macro_block_timestamp = Timestamp::UNIX_EPOCH;
        let last_macro_block_hash = Hash::digest("genesis");
        let last_macro_block_random = Hash::digest("genesis");
        let last_block_timestamp = Timestamp::UNIX_EPOCH;
        let last_block_hash = Hash::digest("genesis");

        //
        // Service awards.
        //
        let awards = Awards::new(cfg.awards_difficulty);
        let epoch_activity = MultiVersionedMap::new();

        let mut blockchain = Blockchain {
            cfg,
            database,
            block_by_hash,
            output_by_hash,
            balance,
            escrow,
            vdf,
            difficulty,
            epoch,
            offset,
            election_result,
            view_change_proof,
            last_macro_block_timestamp,
            last_macro_block_hash,
            last_macro_block_random,
            last_block_timestamp,
            last_block_hash,
            awards,
            epoch_activity,
        };

        blockchain.recover(genesis, timestamp, force_check)?;
        Ok(blockchain)
    }

    //----------------------------------------------------------------------------------------------
    // Recovery.
    //----------------------------------------------------------------------------------------------

    fn recover(
        &mut self,
        genesis: MacroBlock,
        timestamp: Timestamp,
        force_check: bool,
    ) -> Result<(), BlockchainError> {
        let genesis_hash = Hash::digest(&genesis);

        let mut blocks = self.blocks();
        let block = blocks.next();
        let block = if let Some(block) = block {
            block
        } else {
            debug!("Creating a new blockchain...");
            self.push_macro_block(genesis, timestamp)?;
            info!(
                "Initialized a new blockchain: epoch={}, offset={}, last_block={}",
                self.epoch, self.offset, self.last_block_hash
            );
            return Ok(());
        };

        info!("Recovering blockchain from the disk...");

        // Recover genesis.
        self.recover_block(block, timestamp, force_check)?;

        // Check genesis.
        if genesis_hash != self.last_block_hash() {
            return Err(
                BlockchainError::IncompatibleGenesis(genesis_hash, self.last_block_hash()).into(),
            );
        }

        // Recover remaining blocks.
        for block in blocks {
            self.recover_block(block, timestamp, force_check)?;
        }

        info!(
            "Recovered blockchain from the disk: epoch={}, offset={}, last_block={}",
            self.epoch, self.offset, self.last_block_hash
        );

        Ok(())
    }

    fn recover_block(
        &mut self,
        block: Block,
        timestamp: Timestamp,
        force_check: bool,
    ) -> Result<(), BlockchainError> {
        // Skip validate_macro_block()/validate_micro_block().
        match block {
            Block::MicroBlock(block) => {
                debug!(
                    "Recovering a micro block from the disk: epoch={}, offset={}, block={}",
                    block.header.epoch,
                    block.header.offset,
                    Hash::digest(&block)
                );
                if force_check {
                    self.validate_micro_block(&block, timestamp, true)?;
                }
                let lsn = LSN(block.header.epoch, block.header.offset);
                let _ = self.register_micro_block(lsn, block);
            }
            Block::MacroBlock(block) => {
                let block_hash = Hash::digest(&block);
                debug!(
                    "Recovering a macro block from the disk: epoch={}, block={}",
                    block.header.epoch, block_hash
                );
                let mut inputs: Vec<Output> = Vec::with_capacity(block.inputs.len());
                for input_hash in &block.inputs {
                    let input = self.output_by_hash(input_hash)?.expect("Missing output");
                    inputs.push(input);
                }
                if force_check {
                    self.validate_macro_block(&block, &inputs, timestamp)?;
                }
                let lsn = LSN(block.header.epoch, MACRO_BLOCK_OFFSET);
                let _ = self.register_macro_block(lsn, block, inputs);
            }
        }
        Ok(())
    }

    ///
    /// Recovery account state from the blockchain.
    /// TODO: this method is a temporary solution until persistence is implemented in wallet.
    /// https://github.com/stegos/stegos/issues/812
    ///
    pub fn recover_accounts(
        &self,
        accounts: &[(&SecretKey, &PublicKey)],
    ) -> Result<Vec<AccountRecoveryState>, BlockchainError> {
        let mut accounts_state: Vec<AccountRecoveryState> =
            vec![Default::default(); accounts.len()];
        let mut epoch: u64 = 0;

        let mut update_account_state =
            |output: &Output, epoch: u64, is_final: bool, timestamp: Timestamp| {
                let output_hash = Hash::digest(&output);
                if !self.contains_output(&output_hash) {
                    return; // Spent.
                }
                for (account_id, (skey, pkey)) in accounts.iter().enumerate() {
                    if output.is_my_utxo(*skey, *pkey) {
                        accounts_state[account_id].push(OutputRecovery {
                            output: output.clone(),
                            epoch,
                            is_final,
                            timestamp,
                        });
                    }
                }
            };

        for block in self.blocks_starting(epoch, 0) {
            match block {
                Block::MacroBlock(block) => {
                    for output in &block.outputs {
                        update_account_state(output, epoch, true, block.header.timestamp);
                    }
                    epoch += 1;
                }
                Block::MicroBlock(block) => {
                    for tx in block.transactions {
                        for output in tx.txouts() {
                            update_account_state(output, epoch, false, block.header.timestamp);
                        }
                    }
                }
            }
        }
        assert_eq!(epoch, self.epoch);
        Ok(accounts_state)
    }

    //
    // Info
    //
    pub fn election_info(&self) -> ElectionInfo {
        ElectionInfo {
            epoch: self.epoch,
            offset: self.offset,
            view_change: self.view_change(),
            slots_count: self.cfg.max_slot_count as i64,
            current_leader: self.select_leader(self.view_change()),
            next_leader: self.select_leader(self.view_change() + 1),
        }
    }
    //----------------------------------------------------------------------------------------------
    // Database API.
    //----------------------------------------------------------------------------------------------

    /// Return the current blockchain epoch.
    #[inline(always)]
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Returns the number of blocks in the current epoch.
    #[inline(always)]
    pub fn offset(&self) -> u32 {
        self.offset
    }

    /// Returns an iterator over UTXO hashes.
    pub fn unspent(&self) -> impl Iterator<Item = &Hash> {
        self.output_by_hash.keys()
    }

    /// Returns true if blockchain contains unspent output.
    pub fn contains_output(&self, output_hash: &Hash) -> bool {
        self.output_by_hash.get(output_hash).is_some()
    }

    /// Resolve UTXO by hash.
    pub fn output_by_hash(&self, output_hash: &Hash) -> Result<Option<Output>, StorageError> {
        match self.output_by_hash.get(output_hash) {
            Some(OutputKey::MacroBlock { epoch, output_id }) => {
                let block = &self.macro_block(*epoch)?;
                if let Some(output) = block.outputs.get(*output_id as usize) {
                    Ok(Some(output.clone()))
                } else {
                    Ok(None) // Pruned.
                }
            }
            Some(OutputKey::MicroBlock {
                epoch,
                offset,
                tx_id,
                txout_id,
            }) => {
                let block = self.micro_block(*epoch, *offset)?;
                let tx = block
                    .transactions
                    .get(*tx_id as usize)
                    .expect("Corrupted outputs_by_hash (Micro-2)");
                let output = tx
                    .txouts()
                    .get(*txout_id as usize)
                    .expect("Corrupted outputs_by_hash (Micro-3)");
                Ok(Some(output.clone()))
            }
            None => Ok(None),
        }
    }

    /// Checks whether a block exists or not.
    pub fn contains_block(&self, block_hash: &Hash) -> bool {
        if let Some(_lsn) = self.block_by_hash.get(block_hash) {
            return true;
        }
        return false;
    }

    /// Get a block by position.
    fn block(&self, lsn: LSN) -> Result<Block, StorageError> {
        match self.database.get(&Self::block_key(lsn))? {
            Some(buffer) => Ok(Block::from_buffer(&buffer).expect("couldn't deserialize block.")),
            None => panic!("Block must exists"),
        }
    }

    /// Get a micro block by offset.
    pub fn micro_block(&self, epoch: u64, offset: u32) -> Result<MicroBlock, StorageError> {
        Ok(self.block(LSN(epoch, offset))?.unwrap_micro())
    }

    /// Get a block by offset.
    pub fn macro_block(&self, epoch: u64) -> Result<MacroBlock, StorageError> {
        Ok(self.block(LSN(epoch, MACRO_BLOCK_OFFSET))?.unwrap_macro())
    }

    /// Return iterator over saved blocks.
    pub fn blocks(&self) -> impl Iterator<Item = Block> {
        self.database
            .full_iterator(rocksdb::IteratorMode::Start)
            .map(|(_, v)| Block::from_buffer(&*v).expect("couldn't deserialize block."))
    }

    /// Return iterator over saved blocks.
    pub fn blocks_starting(&self, epoch: u64, offset: u32) -> impl Iterator<Item = Block> {
        let key = Self::block_key(LSN(epoch, offset));
        let mode = rocksdb::IteratorMode::From(&key, rocksdb::Direction::Forward);
        self.database
            .full_iterator(mode)
            .map(|(_, v)| Block::from_buffer(&*v).expect("couldn't deserialize block."))
    }

    pub fn election_result(&self) -> &ElectionResult {
        self.election_result.get(&()).unwrap()
    }

    /// Return leader public key for specific view_change number.
    pub fn select_leader(&self, view_change: ViewCounter) -> pbc::PublicKey {
        self.election_result().select_leader(view_change)
    }

    /// Returns public key of the active leader.
    pub fn leader(&self) -> pbc::PublicKey {
        self.select_leader(self.view_change())
    }

    /// Returns the current epoch facilitator.
    #[inline]
    pub fn facilitator(&self) -> &pbc::PublicKey {
        &self.election_result().facilitator
    }

    /// Returns the current epoch validators with their stakes.
    #[inline]
    pub fn validators(&self) -> &Vec<(pbc::PublicKey, i64)> {
        &self.election_result().validators
    }

    /// Returns the validators list, at begining of the epoch
    #[inline]
    pub fn validators_at_epoch_start(&self) -> Vec<(pbc::PublicKey, i64)> {
        let epoch_info = self.election_result_by_offset(0).unwrap();
        epoch_info.validators
    }

    /// Returns true if peer is validator in current epoch.
    #[inline]
    pub fn is_validator(&self, peer: &pbc::PublicKey) -> bool {
        self.election_result().is_validator(peer)
    }

    /// Return the timestamp from the last macro block.
    #[inline]
    pub fn last_macro_block_timestamp(&self) -> Timestamp {
        self.last_macro_block_timestamp
    }

    /// Return the last random value.
    #[inline]
    pub fn last_macro_block_random(&self) -> Hash {
        self.last_macro_block_random
    }

    /// Return the last macro block hash.
    #[inline(always)]
    pub fn last_macro_block_hash(&self) -> Hash {
        self.last_macro_block_hash
    }

    /// Return the timestamp from the last block.
    #[inline]
    pub fn last_block_timestamp(&self) -> Timestamp {
        self.last_block_timestamp
    }

    /// Return the last block hash.
    #[inline(always)]
    pub fn last_block_hash(&self) -> Hash {
        self.last_block_hash
    }

    /// Return the last random value.
    #[inline]
    pub fn last_random(&self) -> Hash {
        self.election_result().random.rand
    }

    /// Return current difficulty.
    #[inline]
    pub fn difficulty(&self) -> u64 {
        self.difficulty
    }

    /// Return Verifiable Delay Function.
    pub fn vdf_solver(&self) -> impl Fn() -> Vec<u8> {
        let challenge = self.last_random().to_bytes();
        let vdf = self.vdf.clone();
        let difficulty = self.difficulty;
        move || vdf.solve(&challenge, difficulty)
    }

    /// A shortcut for self.escrow.validate_stakes().
    #[inline]
    pub fn validate_stakes<'a, OutputIter>(
        &self,
        inputs: OutputIter,
        outputs: OutputIter,
    ) -> Result<(), BlockchainError>
    where
        OutputIter: Iterator<Item = (&'a Output)>,
    {
        self.escrow.validate_stakes(inputs, outputs, self.epoch)
    }

    ///
    /// Iterate over stakes of specified validator.
    ///
    #[inline]
    pub fn iter_validator_stakes(
        &self,
        validator_pkey: &pbc::PublicKey,
    ) -> impl Iterator<Item = (&Hash, i64, &scc::PublicKey, u64)> {
        self.escrow.iter_validator_stakes(validator_pkey)
    }

    ///
    /// Return an account key by network key.
    ///
    #[inline]
    pub fn account_by_network_key(
        &self,
        validator_pkey: &pbc::PublicKey,
    ) -> Option<scc::PublicKey> {
        self.escrow.account_by_network_key(validator_pkey)
    }

    /// Return information about escrow.
    #[inline]
    pub fn escrow_info(&self) -> EscrowInfo {
        self.escrow.info(self.epoch)
    }

    /// Returns balance.
    #[inline]
    pub(crate) fn balance(&self) -> &Balance {
        &self.balance.get(&()).unwrap()
    }

    /// Returns number of leader changes since last epoch creation.
    #[inline]
    pub fn view_change(&self) -> u32 {
        self.election_result().view_change
    }

    /// Returns proof of last view change, if it happen on current offset.
    pub fn view_change_proof(&self) -> &Option<ViewChangeProof> {
        &self.view_change_proof
    }

    /// Returns current service awards state.
    #[doc(hidden)] // TODO used for test
    pub fn service_awards(&self) -> &Awards {
        &self.awards
    }

    /// Returns current service awards state.
    pub fn epoch_activity(&self) -> &BTreeMap<pbc::PublicKey, ValidatorAwardState> {
        self.epoch_activity.inner()
    }

    /// Try producing service awards.
    /// Returns current activity map,
    /// Also returns account PublicKey of the winner of service award,
    /// and amount of winning, if winner was found.
    pub fn awards_from_active_epoch(&self, random: &VRF) -> (BitVector, Option<(PublicKey, i64)>) {
        let mut service_awards = self.service_awards().clone();

        let epoch_activity = self.epoch_activity().clone();
        // use only validators that exist at end of epoch (self.validators() - return current validator list)
        // This will filter out slashed cheaters.
        // Also remove cheater validators.
        let epoch_activity: HashMap<_, _> = self
            .validators()
            .iter()
            .map(|(k, _)| {
                (
                    k,
                    epoch_activity
                        .get(k)
                        .cloned()
                        .unwrap_or(ValidatorAwardState::Active),
                )
            })
            .collect();

        let epoch_validators = self.validators_at_epoch_start();

        let mut activity_map = BitVector::ones(epoch_validators.len());

        for (id, (validator, _)) in epoch_validators.iter().enumerate() {
            match epoch_activity.get(validator) {
                // if validator failed, or cheater, remove it from bitmap.
                Some(ValidatorAwardState::FailedAt(..)) | None => {
                    activity_map.remove(id);
                }
                _ => {}
            }
        }

        let validators_activity = epoch_activity.iter().map(|(k, v)| {
            (
                self.escrow
                    .account_by_network_key(k)
                    .expect("validator has account key"),
                *v,
            )
        });

        service_awards.finalize_epoch(self.cfg().service_award_per_epoch, validators_activity);
        (activity_map, service_awards.check_winners(random.rand))
    }

    /// Returns epoch_activity recovered from MacroBlock activity_map.
    /// This activity_map should be validated by consensus.
    pub(crate) fn epoch_activity_from_macro_block(
        &self,
        activity_map: &BitVector,
    ) -> Result<BTreeMap<PublicKey, ValidatorAwardState>, BlockchainError> {
        let mut validators_activity = BTreeMap::new();
        let validators = self.validators_at_epoch_start();
        if activity_map.len() > validators.len() {
            return Err(BlockError::TooBigActivitymap(activity_map.len(), validators.len()).into());
        };
        for (validator_id, (validator, _)) in validators.iter().enumerate() {
            let activity = activity_map.contains(validator_id);
            let validator_account = self
                .escrow
                .account_by_network_key(validator)
                .expect("Validator with account key");
            let activity = if activity {
                ValidatorAwardState::Active
            } else {
                ValidatorAwardState::FailedAt(self.epoch, self.offset())
            };

            // multiple validators can have single wallet.
            // So try to override only Active state.
            if let Some(ValidatorAwardState::FailedAt(..)) =
                validators_activity.get(&validator_account)
            {
                continue;
            }

            validators_activity.insert(validator_account, activity);
        }
        Ok(validators_activity)
    }

    /// Returns current blockchain config.
    pub fn cfg(&self) -> &ChainConfig {
        &self.cfg
    }

    /// Returns number of total slots in current epoch.
    /// Internally always return cfg.max_slot_count
    pub fn total_slots(&self) -> i64 {
        self.cfg.max_slot_count
    }
    /// Sets new blockchain view_change.
    /// ## Panics
    /// if new_view_change not greater than current.
    #[inline]
    pub fn set_view_change(&mut self, new_view_change: u32, proof: ViewChangeProof) {
        assert!(self.view_change() < new_view_change);
        let lsn = LSN(self.epoch, self.offset);
        let mut election_result = self.election_result().clone();
        election_result.view_change = new_view_change;
        self.election_result.insert(lsn, (), election_result);
        self.view_change_proof = Some(proof);
    }

    /// Resets current view change counter.
    pub fn reset_view_change(&mut self) {
        let lsn = LSN(self.epoch, self.offset);
        let mut election_result = self.election_result().clone();
        election_result.view_change = 0;
        self.election_result.insert(lsn, (), election_result);
        self.view_change_proof = None;
    }

    /// Return election result, for specific moment of history, in past.
    pub fn election_result_by_offset(
        &self,
        offset: u32,
    ) -> Result<ElectionResult, BlockchainError> {
        assert!(self.epoch > 0);
        assert!(offset <= self.offset, "Election info from future offset.");
        trace!(
            "election_result_by_offset offset = {}, our_offset = {}",
            offset,
            self.offset
        );

        //TODO: Avoid unnecessary clones
        let mut election = self.election_result.clone();
        let lsn = if offset == 0 {
            LSN(self.epoch - 1, MACRO_BLOCK_OFFSET)
        } else {
            LSN(self.epoch, offset - 1)
        };

        election.rollback_to_lsn(lsn);
        assert!(election.current_lsn() <= lsn);
        let election = election.get(&()).unwrap().clone();
        trace!(
            "by_offset Validators_len = {}, rand = {}",
            election.validators.len(),
            election.random.rand
        );
        trace!(
            "current Validators_len = {}, rand = {}",
            self.election_result().validators.len(),
            self.election_result().random.rand
        );
        Ok(election)
    }

    //----------------------------------------------------------------------------------------------
    // Macro Blocks
    //----------------------------------------------------------------------------------------------

    /// Create a key for block.
    fn block_key(lsn: LSN) -> [u8; 12] {
        let mut bytes = [0u8; 12];
        BigEndian::write_u64(&mut bytes[0..8], lsn.0);
        BigEndian::write_u32(&mut bytes[8..12], lsn.1);
        bytes
    }

    /// Write block to the disk.
    fn write_block(&self, lsn: LSN, block: Block) -> Result<(), StorageError> {
        let data = block.into_buffer().expect("couldn't serialize block.");
        let mut batch = rocksdb::WriteBatch::default();
        // writebatch put fails if size exceeded u32::max, which is not our case.
        batch.put(&Self::block_key(lsn), &data)?;
        self.database.write(batch)?;
        Ok(())
    }

    ///
    /// Return true if current epoch contains all micro blocks.
    ///
    pub fn is_epoch_full(&self) -> bool {
        self.offset >= self.cfg.micro_blocks_in_epoch
    }

    /// Create a new macro block for current epoch.
    ///
    pub fn create_macro_block(
        &self,
        view_change: u32,
        beneficiary_pkey: &scc::PublicKey,
        network_skey: &pbc::SecretKey,
        network_pkey: pbc::PublicKey,
        timestamp: Timestamp,
    ) -> (MacroBlock, Vec<Transaction>) {
        assert!(self.is_epoch_full());
        let epoch = self.epoch();
        let previous = self.last_macro_block_hash();
        let seed = mix(self.last_macro_block_random(), view_change);
        let random = pbc::make_VRF(network_skey, &seed);
        // Sic: difficulty is constant.
        let difficulty = self.difficulty();

        let mut transactions: Vec<Transaction> = Vec::new();

        //
        // Coinbase.
        //
        {
            let block_reward = self.cfg.block_reward;
            let data = PaymentPayloadData::Comment("Block reward".to_string());
            let (output, gamma, _rvalue) = PaymentOutput::with_payload(
                None,
                &beneficiary_pkey,
                block_reward,
                data.clone(),
                None,
            )
            .expect("invalid keys");

            info!(
                "Created reward UTXO: hash={}, amount={}, data={:?}",
                Hash::digest(&output),
                self.cfg.block_reward,
                data
            );

            let coinbase_tx = CoinbaseTransaction {
                block_reward,
                block_fee: 0,
                gamma: -gamma,
                txouts: vec![output.into()],
            };

            transactions.push(coinbase_tx.into());
        };

        let mut full_reward: i64 =
            self.cfg.block_reward * (self.cfg.micro_blocks_in_epoch as i64 + 1i64);

        //
        // Service Awards.
        //
        let (activity_map, winner) = self.awards_from_active_epoch(&random);
        if let Some((k, reward)) = winner {
            let output = PublicPaymentOutput::new(&k, reward);
            let tx = ServiceAwardTransaction {
                winner_reward: vec![output.into()],
            };
            full_reward += reward;
            transactions.push(tx.into());
        }

        let extra_transactions = transactions.clone();

        // Collect transactions from epoch.
        let count = self.cfg.micro_blocks_in_epoch as usize;
        let blocks: Vec<Block> = self.blocks_starting(self.epoch, 0).take(count).collect();
        for (offset, block) in blocks.into_iter().enumerate() {
            let block = if let Block::MicroBlock(block) = block {
                block
            } else {
                panic!(
                    "Expected micro block: epoch={}, offset={}",
                    self.epoch, offset
                );
            };

            transactions.extend(block.transactions);
        }

        let block = MacroBlock::from_transactions(
            previous,
            epoch,
            view_change,
            network_pkey,
            random,
            difficulty,
            timestamp,
            full_reward,
            activity_map,
            &transactions,
        )
        .expect("Transactions are valid");

        (block, extra_transactions)
    }

    ///
    /// Add a new block into blockchain.
    ///
    /// # Arguments
    ///
    /// * `block` - a macro block to add.
    /// * `timestamp` - block arrival timestamp.
    ///
    pub fn push_macro_block(
        &mut self,
        block: MacroBlock,
        timestamp: Timestamp,
    ) -> Result<(Vec<Output>, Vec<Output>), StorageError> {
        assert_eq!(self.epoch, block.header.epoch);
        assert_eq!(self.offset(), 0);

        //
        // Resolve inputs.
        //
        let mut inputs: Vec<Output> = Vec::with_capacity(block.inputs.len());
        for input_hash in &block.inputs {
            let input = self.output_by_hash(input_hash)?.expect("Missing output");
            inputs.push(input);
        }

        //
        // Double-check if debug.
        //
        if cfg!(debug_assertions) {
            self.validate_macro_block(&block, &inputs, timestamp)
                .expect("block is valid");
        }

        //
        // Write the macro block to the disk.
        //
        let lsn = LSN(self.epoch, MACRO_BLOCK_OFFSET);
        self.write_block(lsn, Block::MacroBlock(block.clone()))?;

        //
        // Update in-memory indexes and metadata.
        //
        let (inputs, outputs) = self.register_macro_block(lsn, block, inputs);

        Ok((inputs, outputs))
    }

    ///
    /// Update indexes and metadata.
    /// Must never fail.
    ///
    fn register_macro_block(
        &mut self,
        lsn: LSN,
        block: MacroBlock,
        inputs: Vec<Output>,
    ) -> (Vec<Output>, Vec<Output>) {
        assert_eq!(block.header.version, VERSION);
        assert_eq!(self.epoch, block.header.epoch);
        assert_eq!(self.offset(), 0);
        assert_eq!(block.header.previous, self.last_macro_block_hash);
        assert!(block.header.timestamp > self.last_macro_block_timestamp);
        assert!(block.header.timestamp > self.last_block_timestamp);
        let epoch = block.header.epoch;
        let block_hash = Hash::digest(&block);

        debug!(
            "Registering a macro block: epoch={}, block={}",
            epoch, &block_hash
        );

        //
        // Prepare inputs.
        //
        assert!(block.inputs.len() <= std::u32::MAX as usize);
        assert_eq!(block.header.inputs_len, block.inputs.len() as u32);
        let input_hashes = block.inputs;

        //
        // Prepare outputs.
        //
        assert!(block.outputs.len() <= std::u32::MAX as usize);
        assert_eq!(block.header.outputs_len, block.outputs.len() as u32);
        let outputs: Vec<Output> = block.outputs;
        let output_keys: Vec<OutputKey> = outputs
            .iter()
            .enumerate()
            .map(|(output_id, _o)| OutputKey::MacroBlock {
                epoch,
                output_id: output_id as u32,
            })
            .collect();

        // update award (skip genesis).
        if epoch > 0 {
            let validators_activity = self
                .epoch_activity_from_macro_block(&block.header.activity_map)
                .unwrap();
            self.awards
                .finalize_epoch(self.cfg.service_award_per_epoch, validators_activity);
            let winner = self.awards.check_winners(block.header.random.rand);
            if let Some((winner_pk, amount)) = winner {
                info!(
                    "Service award found a winner: winner_pk={}, amount={}",
                    winner_pk, amount
                );
            }
            // calculate block reward + service award.
            let full_reward = self.cfg().block_reward
                * (self.cfg().micro_blocks_in_epoch as i64 + 1i64)
                + winner.map(|(_, a)| a).unwrap_or(0);

            assert_eq!(
                block.header.block_reward, full_reward,
                "Invalid macro block reward"
            );
        }

        //
        // Register block.
        //
        self.register_inputs_and_outputs(
            lsn,
            block_hash,
            input_hashes,
            &inputs,
            output_keys,
            &outputs,
            block.header.gamma,
            block.header.block_reward,
        );

        //
        // Update metadata.
        //
        self.epoch += 1;
        self.offset = 0;
        self.last_block_timestamp = block.header.timestamp;
        self.last_block_hash = block_hash;
        self.last_macro_block_timestamp = block.header.timestamp;
        self.last_macro_block_random = block.header.random.rand;
        self.last_macro_block_hash = block_hash;
        assert_eq!(self.last_block_hash, block_hash);
        self.election_result.insert(
            lsn,
            (),
            election::select_validators_slots(
                self.escrow
                    .get_stakers_majority(self.epoch, self.cfg.min_stake_amount),
                block.header.random,
                self.cfg.max_slot_count,
            ),
        );
        self.difficulty = block.header.difficulty;
        debug!("Set difficulty to to {}", self.difficulty);

        info!(
            "Registered a macro block: epoch={}, block={}, inputs={:?}, outputs={:?}",
            epoch,
            block_hash,
            inputs
                .iter()
                .map(|o| Hash::digest(o).to_string())
                .collect::<Vec<String>>(),
            outputs
                .iter()
                .map(|o| Hash::digest(o).to_string())
                .collect::<Vec<String>>(),
        );
        debug!("Validators: {:?}", &self.validators());

        //
        // Update metrics.
        //
        metrics::EPOCH.inc();
        metrics::OFFSET.set(0);
        metrics::MACRO_BLOCK_INPUTS.set(inputs.len() as i64);
        metrics::MACRO_BLOCK_OUTPUTS.set(outputs.len() as i64);
        metrics::MACRO_BLOCK_INPUTS_HG.observe(inputs.len() as f64);
        metrics::MACRO_BLOCK_OUTPUTS_HG.observe(outputs.len() as f64);
        metrics::UTXO_LEN.set(self.output_by_hash.len() as i64);
        metrics::EMISSION.set(self.balance().block_reward);
        metrics::DIFFICULTY.set(self.difficulty as i64);
        for (key, stake) in self.validators().iter() {
            let key_str = key.to_string();
            metrics::VALIDATOR_STAKE_GAUGEVEC
                .with_label_values(&[key_str.as_str()])
                .set(*stake);
        }

        //
        // Finalize storage.
        //
        self.block_by_hash.checkpoint();
        self.output_by_hash.checkpoint();
        self.balance.checkpoint();
        self.escrow.checkpoint();

        (inputs, outputs)
    }

    // ---------------------------------------------------------------------------------------------
    // Micro Blocks
    // ---------------------------------------------------------------------------------------------

    ///
    /// Add a new micro block into blockchain.
    ///
    /// # Arguments
    ///
    /// * `block` - a micro block to add.
    /// * `timestamp` - block arrival timestamp.
    ///
    pub fn push_micro_block(
        &mut self,
        block: MicroBlock,
        timestamp: Timestamp,
    ) -> Result<(Vec<Output>, Vec<Output>, HashMap<Hash, Transaction>), StorageError> {
        assert_eq!(self.epoch, block.header.epoch);
        assert_eq!(self.offset, block.header.offset);

        //
        // Double-check if debug.
        //
        if cfg!(debug_assertions) {
            self.validate_micro_block(&block, timestamp, true)
                .expect("block is valid");
        }

        //
        // Write the micro block to the disk.
        //
        let lsn = LSN(self.epoch, self.offset);
        self.write_block(lsn, Block::MicroBlock(block.clone()))?;

        //
        // Update in-memory indexes and metadata.
        //
        self.register_micro_block(lsn, block)
    }

    ///
    /// Common part of register_macro_block()/register_micro_block().
    ///
    fn register_inputs_and_outputs(
        &mut self,
        lsn: LSN,
        block_hash: Hash,
        input_hashes: Vec<Hash>,
        inputs: &[Output],
        output_keys: Vec<OutputKey>,
        outputs: &[Output],
        gamma: Fr,
        block_reward: i64,
    ) {
        let epoch = self.epoch;

        //
        // Update block_by_hash index.
        //
        if let Some(_) = self.block_by_hash.insert(lsn, block_hash, lsn) {
            panic!(
                "Block hash collision: epoch={}, block={}",
                epoch, block_hash
            );
        }
        assert_eq!(self.block_by_hash.current_lsn(), lsn);

        let mut burned = Pt::identity();
        let mut created = Pt::identity();

        //
        // Process inputs.
        //
        for (input_hash, input) in input_hashes.iter().zip(inputs) {
            debug_assert_eq!(input_hash, &Hash::digest(input));
            if self.output_by_hash.remove(lsn, input_hash).is_none() {
                panic!(
                    "Missing input UTXO: epoch={}, block={}, utxo={}",
                    epoch, block_hash, &input_hash
                );
            }

            if cfg!(debug_assertions) {
                input.validate().expect("valid UTXO");
            }
            burned += input
                .pedersen_commitment()
                .expect("valid Pedersen commitment");

            match input {
                Output::PaymentOutput(_o) => {}
                Output::PublicPaymentOutput(_o) => {}
                Output::StakeOutput(o) => {
                    self.escrow
                        .unstake(lsn, o.validator, input_hash.clone(), self.epoch);
                    assert_eq!(self.escrow.current_lsn(), lsn);
                }
            }

            debug!(
                "Pruned UXTO: epoch={}, block={}, utxo={}",
                epoch, block_hash, input_hash
            );
        }

        //
        // Process outputs.
        //
        for (output_key, output) in output_keys.into_iter().zip(outputs) {
            let output_hash = Hash::digest(output);

            // Update indexes.
            if let Some(_) = self
                .output_by_hash
                .insert(lsn, output_hash.clone(), output_key)
            {
                panic!(
                    "UTXO hash collision: epoch={}, block={}, utxo={}",
                    epoch, &block_hash, &output_hash
                );
            }
            assert_eq!(self.output_by_hash.current_lsn(), lsn);

            created += output
                .pedersen_commitment()
                .expect("valid Pedersen commitment");

            match output {
                Output::PaymentOutput(_o) => {}
                Output::PublicPaymentOutput(_o) => {}
                Output::StakeOutput(o) => {
                    self.escrow.stake(
                        lsn,
                        o.validator,
                        o.recipient,
                        output_hash,
                        self.epoch,
                        self.cfg.stake_epochs,
                        o.amount,
                    );
                    assert_eq!(self.escrow.current_lsn(), lsn);
                }
            }

            debug!(
                "Registered UXTO: epoch={}, block={}, utxo={}",
                epoch, &block_hash, &output_hash
            );
        }

        //
        // Update monetary balance.
        //

        // Check the block monetary balance.
        if fee_a(block_reward) + burned - created != gamma * Pt::one() {
            panic!(
                "Invalid block monetary balance: epoch={}, block={}",
                epoch, &block_hash
            )
        }

        // Global monetary balance.
        let orig_balance = self.balance();
        let balance = Balance {
            created: orig_balance.created + created,
            burned: orig_balance.burned + burned,
            gamma: orig_balance.gamma + gamma,
            block_reward: orig_balance.block_reward + block_reward,
        };
        if fee_a(balance.block_reward) + balance.burned - balance.created
            != balance.gamma * Pt::one()
        {
            panic!(
                "Invalid global monetary balance: epoch={}, block={}",
                epoch, &block_hash
            );
        }
        self.balance.insert(lsn, (), balance);
        assert_eq!(self.balance.current_lsn(), lsn);
    }

    ///
    /// Register a new micro block.
    ///
    fn register_micro_block(
        &mut self,
        lsn: LSN,
        block: MicroBlock,
    ) -> Result<(Vec<Output>, Vec<Output>, HashMap<Hash, Transaction>), StorageError> {
        assert_eq!(block.header.version, VERSION);
        assert_eq!(self.epoch, block.header.epoch);
        assert_eq!(self.offset, block.header.offset);
        assert_eq!(block.header.previous, self.last_block_hash);
        assert!(block.header.timestamp > self.last_block_timestamp);
        assert!(!self.is_epoch_full());
        let epoch = self.epoch;
        let offset = self.offset;
        let block_hash = Hash::digest(&block);

        //
        // Prepare inputs && outputs.
        //
        let mut input_hashes = Vec::new();
        let mut inputs: Vec<Output> = Vec::new();
        let mut output_keys: Vec<OutputKey> = Vec::new();
        let mut outputs: Vec<Output> = Vec::new();
        let mut gamma = Fr::zero();
        let mut block_reward: i64 = 0;
        let mut txs = HashMap::new();
        // Regular transactions.
        for (tx_id, tx) in block.transactions.into_iter().enumerate() {
            assert!(tx_id < std::u32::MAX as usize);

            let tx_hash = Hash::digest(&tx);
            for input_hash in tx.txins() {
                let input = self.output_by_hash(input_hash)?.expect("Missing output");
                inputs.push(input);
                input_hashes.push(input_hash.clone());
            }
            for (txout_id, output) in tx.txouts().iter().enumerate() {
                assert!(txout_id < std::u32::MAX as usize);
                let output_key = OutputKey::MicroBlock {
                    epoch,
                    offset,
                    tx_id: tx_id as u32,
                    txout_id: txout_id as u32,
                };
                outputs.push(output.clone());
                output_keys.push(output_key);
            }
            match &tx {
                Transaction::CoinbaseTransaction(tx) => {
                    block_reward += tx.block_reward;
                    gamma += tx.gamma;
                }
                Transaction::PaymentTransaction(tx) => {
                    gamma += tx.gamma;
                }
                Transaction::RestakeTransaction(_tx) => {}
                Transaction::SlashingTransaction(tx) => {
                    info!(
                        "Found slashing transaction, removing validator, from list: cheater={}",
                        tx.cheater()
                    );
                    let validators = &self.election_result().validators;
                    // remove cheater for current epoch.
                    let new_validators = validators
                        .into_iter()
                        .filter_map(|(k, v)| {
                            if k != &tx.cheater() {
                                Some((*k, *v))
                            } else {
                                None
                            }
                        })
                        .collect();
                    let mut election_result = self.election_result().clone();
                    election_result.validators = new_validators;
                    self.election_result.insert(lsn, (), election_result);
                }
                Transaction::ServiceAwardTransaction(_tx) => unreachable!(),
            }

            assert!(txs.insert(tx_hash, tx).is_none());
        }

        //
        // Update service awards
        //
        // Set skipped validators to inactive.
        for skiped_view_change in 0..block.header.view_change {
            let leader = self.election_result().select_leader(skiped_view_change);
            self.epoch_activity.insert(
                lsn,
                leader,
                ValidatorAwardState::FailedAt(self.epoch(), self.offset()),
            );
        }

        // set current leader to active, if it was unknown.
        let leader = self
            .election_result()
            .select_leader(block.header.view_change);
        if self.epoch_activity.get(&leader).is_none() {
            self.epoch_activity
                .insert(lsn, leader, ValidatorAwardState::Active);
        }

        //
        // Register block.
        //
        self.register_inputs_and_outputs(
            lsn,
            block_hash,
            input_hashes,
            &inputs,
            output_keys,
            &outputs,
            gamma,
            block_reward,
        );

        //
        // Update metadata.
        //
        self.last_block_timestamp = block.header.timestamp;
        self.last_block_hash = block_hash;
        self.offset += 1;
        let mut election_result = self.election_result().clone();
        election_result.view_change = 0;
        election_result.random = block.header.random;
        self.election_result.insert(lsn, (), election_result);

        //
        // Update metrics.
        //
        metrics::OFFSET.set(self.offset as i64);
        metrics::MICRO_BLOCK_INPUTS.set(inputs.len() as i64);
        metrics::MICRO_BLOCK_OUTPUTS.set(outputs.len() as i64);
        metrics::MICRO_BLOCK_TRANSACTIONS.set(txs.len() as i64);
        metrics::MICRO_BLOCK_INPUTS_HG.observe(inputs.len() as f64);
        metrics::MICRO_BLOCK_OUTPUTS_HG.observe(outputs.len() as f64);
        metrics::MICRO_BLOCK_TRANSACTIONS_HG.observe(txs.len() as f64);
        metrics::UTXO_LEN.set(self.output_by_hash.len() as i64);
        metrics::EMISSION.set(self.balance().block_reward);

        info!(
            "Registered a micro block: epoch={}, offset={}, block={}, txs={:?}, inputs={:?}, outputs={:?}",
            epoch,
            offset,
            block_hash,
            txs.iter()
                .map(|(h, _1tx)| h.to_string())
                .collect::<Vec<String>>(),
            inputs.iter()
                .map(|o| Hash::digest(o).to_string())
                .collect::<Vec<String>>(),
            outputs.iter()
                .map(|o| Hash::digest(o).to_string())
                .collect::<Vec<String>>(),
        );

        Ok((inputs, outputs, txs))
    }

    pub fn pop_micro_block(
        &mut self,
    ) -> Result<(Vec<Output>, Vec<Output>, Vec<Transaction>), StorageError> {
        assert!(self.epoch > 0, "doesn't work for genesis");
        assert!(self.offset > 0, "attempt to revert the macro block");
        let offset = self.offset - 1;
        //
        // Remove from the disk.
        //
        let block = self.micro_block(self.epoch, offset)?;
        let (previous, lsn, last_block_timestamp) = if offset == 0 {
            // Previous block is Macro Block.
            let block = self.macro_block(self.epoch - 1)?;
            let lsn = LSN(self.epoch - 1, MACRO_BLOCK_OFFSET);
            (Hash::digest(&block), lsn, block.header.timestamp)
        } else {
            // Previous block is Micro Block.
            let block = self.micro_block(self.epoch, offset - 1)?;
            let lsn = LSN(self.epoch, offset - 1);
            (Hash::digest(&block), lsn, block.header.timestamp)
        };
        self.database
            .delete(&Self::block_key(LSN(self.epoch, offset)))?;
        let block_hash = Hash::digest(&block);

        //
        // Revert metadata.
        //
        self.block_by_hash.rollback_to_lsn(lsn);
        self.output_by_hash.rollback_to_lsn(lsn);
        self.balance.rollback_to_lsn(lsn);
        self.escrow.rollback_to_lsn(lsn);
        self.epoch_activity.rollback_to_lsn(lsn);

        self.election_result.rollback_to_lsn(lsn);
        assert_eq!(self.block_by_hash.current_lsn(), lsn);
        assert_eq!(self.election_result.current_lsn(), lsn);
        assert!(self.epoch_activity.current_lsn() <= lsn);
        assert!(self.output_by_hash.current_lsn() <= lsn);
        assert!(self.balance.current_lsn() <= lsn);
        assert!(self.escrow.current_lsn() <= lsn);
        self.offset = offset;
        self.last_block_hash = previous;
        self.last_block_timestamp = last_block_timestamp;
        self.reset_view_change();

        let mut created: Vec<Output> = Vec::new();
        let mut pruned: Vec<Output> = Vec::new();
        let mut removed = Vec::new();
        for tx in block.transactions {
            for input_hash in tx.txins() {
                let input = self.output_by_hash(input_hash)?.expect("exists");
                created.push(input);
                debug!(
                    "Restored UXTO: epoch={}, block={}, utxo={}",
                    self.epoch, &block_hash, &input_hash
                );
            }
            for output in tx.txouts() {
                pruned.push(output.clone());
                let output_hash = Hash::digest(output);
                debug!(
                    "Reverted UTXO: epoch={}, block={}, utxo={}",
                    self.epoch, &block_hash, &output_hash
                );
            }
            match tx {
                Transaction::PaymentTransaction(_) | Transaction::RestakeTransaction(_) => {
                    removed.push(tx)
                }
                _ => continue,
            }
        }

        //
        // Update metrics.
        //
        metrics::OFFSET.set(self.offset as i64);
        metrics::UTXO_LEN.set(self.output_by_hash.len() as i64);
        metrics::EMISSION.set(self.balance().block_reward);

        info!(
            "Reverted a micro block: epoch={}, offset={}, block={}, inputs={:?}, outputs={:?}",
            self.epoch,
            offset,
            &block_hash,
            created
                .iter()
                .map(|o| Hash::digest(o).to_string())
                .collect::<Vec<String>>(),
            pruned
                .iter()
                .map(|o| Hash::digest(o).to_string())
                .collect::<Vec<String>>(),
        );

        Ok((pruned, created, removed))
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    use crate::test;
    use crate::timestamp::Timestamp;
    use rand::Rng;
    use simple_logger;
    use std::collections::BTreeMap;
    use std::time::Duration;
    use tempdir::TempDir;

    #[test]
    fn basic() {
        simple_logger::init_with_level(log::Level::Debug).unwrap_or_default();

        let timestamp = Timestamp::now();
        let cfg: ChainConfig = Default::default();
        let (keychains, block1) = test::fake_genesis(
            cfg.min_stake_amount,
            10 * cfg.min_stake_amount,
            3,
            timestamp,
            None,
        );
        let chain_dir = TempDir::new("test").unwrap();
        let blockchain = Blockchain::new(cfg, chain_dir.path(), false, block1.clone(), timestamp)
            .expect("Failed to create blockchain");
        let outputs: Vec<Output> = block1.outputs.clone();
        let mut unspent: Vec<Hash> = outputs.iter().map(|o| Hash::digest(o)).collect();
        unspent.sort();
        let mut unspent2: Vec<Hash> = blockchain.unspent().cloned().collect();
        unspent2.sort();
        assert_eq!(unspent, unspent2);

        let validators = blockchain
            .escrow
            .get_stakers_majority(blockchain.epoch, blockchain.cfg.min_stake_amount);
        assert_eq!(validators.len(), keychains.len());
        let validators_map: BTreeMap<_, _> = validators.iter().cloned().collect();
        for keychain in &keychains {
            let stake = validators_map.get(&keychain.network_pkey).expect("exists");
            assert_eq!(*stake, blockchain.cfg.min_stake_amount);
        }
        assert_eq!(blockchain.last_block_hash(), Hash::digest(&block1));

        let blocks2: Vec<Block> = blockchain.blocks().collect();
        assert_eq!(blocks2.len(), 1);
        assert_eq!(Hash::digest(&blocks2[0]), Hash::digest(&block1));

        assert!(blockchain.contains_block(&Hash::digest(&block1)));
        assert!(!blockchain.contains_block(&Hash::digest("test")));

        assert_eq!(
            Hash::digest(&blockchain.block(LSN(0, MACRO_BLOCK_OFFSET)).unwrap()),
            Hash::digest(&block1)
        );

        assert!(!blockchain.contains_output(&Hash::digest("test")));
        assert!(blockchain
            .output_by_hash(&Hash::digest("test"))
            .expect("no disk errors")
            .is_none());
        for output in block1.outputs.iter() {
            let output_hash = Hash::digest(&output);
            let output2 = blockchain
                .output_by_hash(&output_hash)
                .expect("no disk errors")
                .expect("exists");
            assert_eq!(Hash::digest(&output2), output_hash);
            assert!(blockchain.contains_output(&output_hash));
        }
    }

    #[test]
    fn iterate() {
        const NUM_NODES: usize = 32;
        const STAKE_EPOCHS: u64 = 2;
        const EPOCHS: u64 = 10;

        simple_logger::init_with_level(log::Level::Debug).unwrap_or_default();

        let mut rng = rand::thread_rng();
        let mut cfg: ChainConfig = Default::default();
        cfg.stake_epochs = STAKE_EPOCHS;
        cfg.micro_blocks_in_epoch = 2;
        let mut timestamp = Timestamp::now();
        let (keychains, genesis) = test::fake_genesis(
            cfg.min_stake_amount,
            (NUM_NODES as i64) * cfg.min_stake_amount + 100,
            NUM_NODES,
            timestamp,
            None,
        );
        let chain_dir = TempDir::new("test").unwrap();
        let mut chain = Blockchain::new(
            cfg.clone(),
            chain_dir.path(),
            true,
            genesis.clone(),
            timestamp,
        )
        .expect("Failed to create blockchain");

        for _epoch in 0..EPOCHS {
            let epoch = chain.epoch();

            //
            // Non-empty block.
            //
            timestamp += Duration::from_secs(rng.gen_range(5, 10));
            let (block, input_hashes, output_hashes) =
                test::create_fake_micro_block(&mut chain, &keychains, timestamp);
            let hash = Hash::digest(&block);
            let offset = chain.offset();
            chain
                .push_micro_block(block, timestamp)
                .expect("no I/O errors");
            assert_eq!(hash, chain.last_block_hash());
            assert_eq!(offset + 1, chain.offset());
            for input_hash in input_hashes {
                assert!(!chain.contains_output(&input_hash));
            }
            for output_hash in output_hashes {
                assert!(chain.contains_output(&output_hash));
            }

            for offset in 1..chain.cfg().micro_blocks_in_epoch {
                //
                // Empty block.
                //
                timestamp += Duration::from_secs(rng.gen_range(5, 10));
                let block =
                    test::create_micro_block_with_coinbase(&mut chain, &keychains, timestamp);
                let hash = Hash::digest(&block);
                chain
                    .push_micro_block(block, timestamp)
                    .expect("no I/O errors");
                assert_eq!(hash, chain.last_block_hash());
                assert_eq!(offset + 1, chain.offset());
            }

            //
            // Macro block.
            //

            // Create a macro block.
            timestamp += Duration::from_secs(5 + rng.gen_range(0, 10));
            let (block, extra_transactions) =
                test::create_fake_macro_block(&chain, &keychains, timestamp);
            let hash = Hash::digest(&block);

            // Collect unspent outputs.
            let mut unspent: Vec<Hash> = chain.unspent().cloned().collect();
            for tx in extra_transactions {
                assert_eq!(tx.txins().len(), 0);
                for output in tx.txouts() {
                    let output_hash = Hash::digest(output);
                    unspent.push(output_hash);
                }
            }
            unspent.sort();

            // Remove all micro blocks.
            while chain.offset() > 0 {
                chain.pop_micro_block().expect("Should be ok");
            }
            // Push the macro block.
            chain
                .push_macro_block(block, timestamp)
                .expect("Invalid block");
            let mut unspent2: Vec<Hash> = chain.unspent().cloned().collect();
            unspent2.sort();
            assert_eq!(unspent, unspent2);
            assert_eq!(hash, chain.last_block_hash());
            assert_eq!(epoch + 1, chain.epoch());
            assert_eq!(0, chain.offset());
        }

        //
        // Recovery.
        //
        let epoch = chain.epoch();
        let offset = chain.offset();
        let block_count = chain.blocks().count();
        let block_hash = chain.last_block_hash();
        let balance = chain.balance().clone();
        drop(chain);
        let chain = Blockchain::new(cfg, chain_dir.path(), true, genesis, timestamp)
            .expect("Failed to create blockchain");
        assert_eq!(epoch, chain.epoch());
        assert_eq!(offset, chain.offset());
        assert_eq!(block_hash, chain.last_block_hash());
        assert_eq!(block_count, chain.blocks().count());
        assert_eq!(&balance, chain.balance());
    }

    #[test]
    fn rollback() {
        simple_logger::init_with_level(log::Level::Debug).unwrap_or_default();

        //
        // Create genesis block.
        //
        let mut timestamp = Timestamp::now();
        let block_timestamp0 = timestamp;
        let cfg: ChainConfig = Default::default();
        let (keychains, genesis) = test::fake_genesis(
            cfg.min_stake_amount,
            10 * cfg.min_stake_amount,
            1,
            block_timestamp0,
            None,
        );
        let block_hash0 = Hash::digest(&genesis);
        let chain_dir = TempDir::new("test").unwrap();
        timestamp += Duration::from_millis(1);
        let mut chain = Blockchain::new(
            cfg.clone(),
            chain_dir.path(),
            true,
            genesis.clone(),
            timestamp,
        )
        .expect("Failed to create blockchain");
        assert_eq!(1, chain.epoch());
        assert_eq!(0, chain.offset());
        assert_eq!(1, chain.blocks().count());
        assert_eq!(0, chain.view_change());
        assert_eq!(block_hash0, chain.last_block_hash());
        assert_eq!(block_hash0, chain.last_macro_block_hash());
        assert_eq!(block_timestamp0, chain.last_block_timestamp());
        assert_eq!(block_timestamp0, chain.last_macro_block_timestamp());
        let epoch0 = chain.epoch();
        let offset0 = chain.offset();
        let count0 = chain.blocks().count();
        let balance0 = chain.balance().clone();
        let escrow0 = chain.escrow_info().clone();

        //
        // Register a micro block #1.
        //
        timestamp += Duration::from_millis(1);
        let block_timestamp1 = timestamp;
        let (block1, input_hashes1, output_hashes1) =
            test::create_fake_micro_block(&mut chain, &keychains, block_timestamp1);
        let block_hash1 = Hash::digest(&block1);
        timestamp += Duration::from_millis(1);
        chain
            .push_micro_block(block1, timestamp)
            .expect("no I/O errors");
        assert_eq!(2, chain.blocks().count());
        assert_eq!(1, chain.offset());
        assert_eq!(0, chain.view_change());
        assert_eq!(block_hash1, chain.last_block_hash());
        assert_eq!(block_timestamp1, chain.last_block_timestamp());
        assert_eq!(block_hash0, chain.last_macro_block_hash());
        assert_eq!(block_timestamp0, chain.last_macro_block_timestamp());
        assert_ne!(&balance0, chain.balance());
        for input_hash in &input_hashes1 {
            assert!(!chain.contains_output(input_hash));
        }
        for output_hash in &output_hashes1 {
            assert!(chain.contains_output(output_hash));
        }
        let count1 = chain.blocks().count();
        let offset1 = chain.offset();
        let block_hash1 = chain.last_block_hash();
        let balance1 = chain.balance().clone();
        let escrow1 = chain.escrow_info().clone();

        //
        // Register a micro block #2.
        //
        timestamp += Duration::from_millis(1);
        let block_timestamp2 = timestamp;
        let (block2, input_hashes2, output_hashes2) =
            test::create_fake_micro_block(&mut chain, &keychains, block_timestamp2);
        let block_hash2 = Hash::digest(&block2);
        timestamp += Duration::from_millis(1);
        chain
            .push_micro_block(block2, timestamp)
            .expect("no I/O errors");
        assert_eq!(1, chain.epoch());
        assert_eq!(2, chain.offset());
        assert_eq!(0, chain.view_change());
        assert_eq!(3, chain.blocks().count());
        assert_eq!(block_hash2, chain.last_block_hash());
        assert_eq!(block_timestamp2, chain.last_block_timestamp());
        assert_eq!(block_hash0, chain.last_macro_block_hash());
        assert_eq!(block_timestamp0, chain.last_macro_block_timestamp());
        assert_ne!(&balance1, chain.balance());
        for input_hash in &input_hashes2 {
            assert!(!chain.contains_output(input_hash));
        }
        for output_hash in &output_hashes2 {
            assert!(chain.contains_output(output_hash));
        }

        //
        // Pop micro block #2.
        //
        chain.pop_micro_block().expect("no disk errors");
        assert_eq!(epoch0, chain.epoch());
        assert_eq!(offset1, chain.offset());
        assert_eq!(0, chain.view_change());
        assert_eq!(count1, chain.blocks().count());
        assert_eq!(block_hash1, chain.last_block_hash());
        assert_eq!(block_timestamp1, chain.last_block_timestamp());
        assert_eq!(block_hash0, chain.last_macro_block_hash());
        assert_eq!(block_timestamp0, chain.last_macro_block_timestamp());
        assert_eq!(&balance1, chain.balance());
        assert_eq!(escrow1, chain.escrow_info());
        for input_hash in &input_hashes2 {
            assert!(chain.contains_output(input_hash));
        }
        for output_hash in &output_hashes2 {
            assert!(!chain.contains_output(output_hash));
        }

        //
        // Try to recover after popping micro block #2.
        //
        drop(chain);
        timestamp += Duration::from_secs(60 * 60 * 24);
        let mut chain = Blockchain::new(
            cfg.clone(),
            chain_dir.path(),
            true,
            genesis.clone(),
            timestamp,
        )
        .expect("Failed to create blockchain");
        assert_eq!(epoch0, chain.epoch());
        assert_eq!(offset1, chain.offset());
        assert_eq!(0, chain.view_change());
        assert_eq!(count1, chain.blocks().count());
        assert_eq!(block_hash1, chain.last_block_hash());
        assert_eq!(block_timestamp1, chain.last_block_timestamp());
        assert_eq!(block_hash0, chain.last_macro_block_hash());
        assert_eq!(block_timestamp0, chain.last_macro_block_timestamp());
        assert_eq!(&balance1, chain.balance());
        assert_eq!(escrow1, chain.escrow_info());
        for input_hash in &input_hashes2 {
            assert!(chain.contains_output(&input_hash));
        }
        for output_hash in &output_hashes2 {
            assert!(!chain.contains_output(&output_hash));
        }

        //
        // Pop micro block #1.
        //
        chain.pop_micro_block().expect("no disk errors");
        assert_eq!(epoch0, chain.epoch());
        assert_eq!(offset0, chain.offset());
        assert_eq!(0, chain.view_change());
        assert_eq!(count0, chain.blocks().count());
        assert_eq!(block_hash0, chain.last_block_hash());
        assert_eq!(block_timestamp0, chain.last_block_timestamp());
        assert_eq!(block_hash0, chain.last_macro_block_hash());
        assert_eq!(block_timestamp0, chain.last_macro_block_timestamp());
        assert_eq!(&balance0, chain.balance());
        assert_eq!(escrow0, chain.escrow_info());
        for input_hash in &input_hashes1 {
            assert!(chain.contains_output(&input_hash));
        }
        for output_hash in &output_hashes1 {
            assert!(!chain.contains_output(&output_hash));
        }

        //
        // Try to recover after popping micro block #1.
        //
        drop(chain);
        timestamp += Duration::from_secs(60 * 60 * 24);
        let chain = Blockchain::new(
            cfg.clone(),
            chain_dir.path(),
            true,
            genesis.clone(),
            timestamp,
        )
        .expect("Failed to create blockchain");
        assert_eq!(epoch0, chain.epoch());
        assert_eq!(offset0, chain.offset());
        assert_eq!(0, chain.view_change());
        assert_eq!(count0, chain.blocks().count());
        assert_eq!(block_hash0, chain.last_block_hash());
        assert_eq!(block_timestamp0, chain.last_block_timestamp());
        assert_eq!(block_hash0, chain.last_macro_block_hash());
        assert_eq!(block_timestamp0, chain.last_macro_block_timestamp());
        assert_eq!(&balance0, chain.balance());
        assert_eq!(escrow0, chain.escrow_info());
        for input_hash in &input_hashes1 {
            assert!(chain.contains_output(&input_hash));
        }
        for output_hash in &output_hashes1 {
            assert!(!chain.contains_output(&output_hash));
        }
        drop(chain);
    }

    #[test]
    fn block_iter_limit() {
        simple_logger::init_with_level(log::Level::Debug).unwrap_or_default();
        let mut timestamp = Timestamp::now();
        let mut cfg: ChainConfig = Default::default();
        cfg.micro_blocks_in_epoch = 100500;
        let stake = cfg.min_stake_amount;
        let (keychains, blocks) =
            test::fake_genesis(stake, 10 * cfg.min_stake_amount, 1, timestamp, None);
        let chain_dir = TempDir::new("test").unwrap();
        let mut blockchain = Blockchain::new(cfg, chain_dir.path(), true, blocks, timestamp)
            .expect("Failed to create blockchain");
        let epoch = blockchain.epoch();
        let starting_offset = blockchain.offset();
        // len of genesis
        assert!(blockchain.epoch() > 0);
        for _offset in 2..12 {
            timestamp += Duration::from_millis(1);
            let block = test::create_micro_block_with_coinbase(&blockchain, &keychains, timestamp);
            blockchain
                .push_micro_block(block, timestamp)
                .expect("Invalid block");
        }

        assert_eq!(
            blockchain
                .blocks_starting(epoch, starting_offset)
                .take(1)
                .count(),
            1
        );

        assert_eq!(
            blockchain
                .blocks_starting(epoch, starting_offset)
                .take(4)
                .count(),
            4
        );
        // limit
        assert_eq!(
            blockchain
                .blocks_starting(epoch, starting_offset)
                .take(20)
                .count(),
            10
        );
        // empty
        assert_eq!(
            blockchain
                .blocks_starting(epoch, blockchain.offset())
                .take(1)
                .count(),
            0
        );
    }
}
