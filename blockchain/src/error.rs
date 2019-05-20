//! Blockchain Errors.

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

use crate::view_changes::ViewChangeProof;
use crate::OutputError;
use failure::Fail;
use stegos_crypto::hash::Hash;
use stegos_crypto::pbc;
use stegos_crypto::CryptoError;

#[derive(Debug, Fail)]
pub enum BlockchainError {
    #[fail(
        display = "Found a saved chain that is not compatible to our genesis at height = {}, \
                   genesis_block = {:?}, database_block = {:?}",
        _0, _1, _2
    )]
    IncompatibleChain(u64, Hash, Hash),
    #[fail(
        display = "Stake is locked: validator={}, expected_balance={}, minimum_balance={}",
        _0, _1, _2
    )]
    StakeIsLocked(pbc::PublicKey, i64, i64),
    #[fail(display = "Internal storage error={}", _0)]
    StorageError(failure::Error),
    #[fail(display = "Transaction error={}", _0)]
    TransactionError(TransactionError),
    #[fail(display = "Block error={}", _0)]
    BlockError(BlockError),
    #[fail(display = "Output error={}", _0)]
    OutputError(OutputError),
    #[fail(display = "Crypto error={}", _0)]
    CryptoError(CryptoError),
}

/// Transaction errors.
#[derive(Debug, Fail)]
pub enum TransactionError {
    #[fail(display = "Invalid signature: tx={}", _0)]
    InvalidSignature(Hash),
    #[fail(display = "Invalid monetary balance: tx={}", _0)]
    InvalidMonetaryBalance(Hash),
    #[fail(display = "Negative fee: tx={}", _0)]
    NegativeFee(Hash),
    #[fail(display = "Negative reward: tx={}", _0)]
    NegativeReward(Hash),
    #[fail(display = "No inputs: tx={}", _0)]
    NoInputs(Hash),
    #[fail(display = "Missing transaction input: tx={}, utxo={}", _0, _1)]
    MissingInput(Hash, Hash),
    #[fail(display = "Duplicate input: tx={}, utxo={}", _0, _1)]
    DuplicateInput(Hash, Hash),
    #[fail(display = "Duplicate output: tx={}, utxo={}", _0, _1)]
    DuplicateOutput(Hash, Hash),
    #[fail(display = "Output hash collision: tx={}, utxo={}", _0, _1)]
    OutputHashCollision(Hash, Hash),

    #[fail(
        display = "CoinbaseTransaction must contain only PaymentUTXOs: tx={}, utxo={}",
        _0, _1
    )]
    NonPaymentOutputInCoinbase(Hash, Hash),

    #[fail(display = "Non-StakeUTXO found in TXINs: tx = {}. utxo={}", _0, _1)]
    InvalidRestakingInput(Hash, Hash),

    #[fail(display = "TXINs with mixed validator keys: tx = {}, utxo={}", _0, _1)]
    RestakingValidatorKeyMismatch(Hash, Hash),

    #[fail(
        display = "StakeUTXOs with mixed recipient keys: tx = {}, utxo={}",
        _0, _1
    )]
    MixedRestakingOwners(Hash, Hash),

    #[fail(display = "No TXINs: tx={}", _0)]
    NoRestakingTxins(Hash),

    #[fail(display = "Non-StakeUTXO found in TXOUTs: tx={}, utxo={}", _0, _1)]
    InvalidRestakingOutput(Hash, Hash),

    #[fail(display = "TXOUTs with mixed validator keys: tx={}, utxo={}", _0, _1)]
    MixedTxoutValidators(Hash, Hash),

    #[fail(display = "TXIN amount .ne. TXOUT amount: tx={}", _0)]
    ImbalancedRestaking(Hash),
}

#[derive(Debug, Fail)]
pub enum MultisignatureError {
    #[fail(
        display = "Signature bitmap too big: len={}, validators_len={} ",
        _0, _1
    )]
    TooBigBitmap(usize, usize),
    #[fail(
        display = "Not enough votes in signature: votes={}, needed_votes={} ",
        _0, _1
    )]
    NotEnoughtVotes(i64, i64),
    #[fail(display = "Signature is not valid: hash={} ", _0)]
    InvalidSignature(Hash),
}

#[derive(Debug, Fail)]
pub enum BlockError {
    #[fail(
        display = "Previous hash mismatch: height={}, block={}, block_previous={}, our_previous={}",
        _0, _1, _2, _3
    )]
    InvalidPreviousHash(u64, Hash, Hash, Hash),
    #[fail(display = "Block hash collision: height={}, block={}", _0, _1)]
    BlockHashCollision(u64, Hash),
    #[fail(
        display = "Out of order block: block={}, block_height={}, our_height={}",
        _0, _1, _2
    )]
    OutOfOrderBlock(Hash, u64, u64),
    #[fail(
        display = "Invalid block fee: block={}, expected={}, got={}",
        _0, _1, _2
    )]
    InvalidFee(Hash, i64, i64),
    #[fail(
        display = "Coinbase transaction must be first in the block: block={}",
        _0
    )]
    CoinbaseMustBeFirst(Hash),
    #[fail(
        display = "Invalid block monetary balance: height={}, block={}",
        _0, _1
    )]
    InvalidBlockBalance(u64, Hash),
    #[fail(
        display = "Invalid block input hash: height={}, block={}, expected={}, got={}",
        _0, _1, _2, _3
    )]
    InvalidBlockInputsHash(u64, Hash, Hash, Hash),
    #[fail(
        display = "Invalid block output hash: height={}, block={}, expected={}, got={}",
        _0, _1, _2, _3
    )]
    InvalidBlockOutputsHash(u64, Hash, Hash, Hash),
    #[fail(
        display = "Missing block input: height={}, block={}, utxo={}",
        _0, _1, _1
    )]
    MissingBlockInput(u64, Hash, Hash),
    #[fail(
        display = "Duplicate block input: height={}, block={}, utxo={}",
        _0, _1, _1
    )]
    DuplicateBlockInput(u64, Hash, Hash),
    #[fail(
        display = "Duplicate block output: height={}, block={}, utxo={}",
        _0, _1, _2
    )]
    DuplicateBlockOutput(u64, Hash, Hash),
    #[fail(
        display = "Output hash collision: height={}, block={}, utxo={}",
        _0, _1, _2
    )]
    OutputHashCollision(u64, Hash, Hash),
    #[fail(display = "The leader must be validator: height={}, block={}", _0, _1)]
    LeaderIsNotValidator(u64, Hash),
    #[fail(
        display = "No leader signature was found in BLS signature: height={}, block={}",
        _0, _1
    )]
    NoLeaderSignatureFound(u64, Hash),
    #[fail(
        display = "Found propose with more than one signature: height={}, block={}",
        _0, _1
    )]
    MoreThanOneSignatureAtPropose(u64, Hash),
    #[fail(
        display = "Different leader found in received block: elected={}, sender={}",
        _0, _1
    )]
    DifferentPublicKey(pbc::PublicKey, pbc::PublicKey),
    #[fail(
        display = "Invalid leader signature found: height={}, block={}",
        _0, _1
    )]
    InvalidLeaderSignature(u64, Hash),
    #[fail(
        display = "Invalid block BLS multisignature: height={}, block={}, error={}",
        _1, _2, _0
    )]
    InvalidBlockSignature(MultisignatureError, u64, Hash),
    #[fail(
        display = "Invalid block version: height={}, block={}, block_version={}, our_version={}",
        _0, _1, _2, _3
    )]
    InvalidBlockVersion(u64, Hash, u64, u64),
    #[fail(
        display = "Received block with invalid random: height={}, block={}",
        _0, _1
    )]
    IncorrectRandom(u64, Hash),
    #[fail(
        display = "Received block with wrong view_change: height={}, block={}, block_view_change={}, our_view_change={}",
        _0, _1, _2, _3
    )]
    InvalidViewChange(u64, Hash, u32, u32),
    #[fail(
        display = "Invalid view change proof: height={}, proof={:?}, error={}",
        _0, _1, _2
    )]
    InvalidViewChangeProof(u64, ViewChangeProof, MultisignatureError),
    #[fail(
        display = "No proof of view change found for out of order block: height={}, block={}, block_view_change={}, our_view_change={}",
        _0, _1, _2, _3
    )]
    NoProofWasFound(u64, Hash, u32, u32),
}

impl From<failure::Error> for BlockchainError {
    fn from(error: failure::Error) -> BlockchainError {
        BlockchainError::StorageError(error)
    }
}

impl From<TransactionError> for BlockchainError {
    fn from(error: TransactionError) -> BlockchainError {
        BlockchainError::TransactionError(error)
    }
}

impl From<BlockError> for BlockchainError {
    fn from(error: BlockError) -> BlockchainError {
        BlockchainError::BlockError(error)
    }
}

impl From<OutputError> for BlockchainError {
    fn from(error: OutputError) -> BlockchainError {
        BlockchainError::OutputError(error)
    }
}

impl From<CryptoError> for BlockchainError {
    fn from(error: CryptoError) -> BlockchainError {
        BlockchainError::CryptoError(error)
    }
}
