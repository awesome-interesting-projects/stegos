//! Node - Errors.

//
// MIT License
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

use failure::Fail;
use std::time::SystemTime;
use stegos_crypto::hash::Hash;

#[derive(Debug, Fail, PartialEq, Eq)]
pub enum NodeTransactionError {
    #[fail(
        display = "Transaction fee is too low: tx={}, min={}, got={}",
        _0, _1, _2
    )]
    TooLowFee(Hash, i64, i64),
    #[fail(display = "Transaction already exists in mempool: tx={}", _0)]
    AlreadyExists(Hash),
    #[fail(display = "Missing transaction input: tx={}, utxo={}", _0, _1)]
    MissingInput(Hash, Hash),
    #[fail(display = "Output hash collision: tx={}, utxo={}", _0, _1)]
    OutputHashCollision(Hash, Hash),
}

#[derive(Debug, Fail, PartialEq, Eq)]
pub enum NodeBlockError {
    #[fail(
        display = "Unexpected monetary adjustment: height={}, block={}, got={}, expected={}",
        _0, _1, _2, _3
    )]
    InvalidMonetaryAdjustment(u64, Hash, i64, i64),
    #[fail(
        display = "Expected a key block, got monetary block: height={}, block={}",
        _0, _1
    )]
    ExpectedKeyBlock(u64, Hash),
    #[fail(
        display = "Expected a monetary block, got key block: height={}, block={}",
        _0, _1
    )]
    ExpectedMonetaryBlock(u64, Hash),
    #[fail(
        display = "Timestamp is out of sync: height={}, block={}, block_timestamp={:?}, our_timestamp={:?}",
        _0, _1, _2, _3
    )]
    OutOfSyncTimestamp(u64, Hash, SystemTime, SystemTime),
    #[fail(
        display = "Proposed view_change different from ours: height={}, block={}, block_viewchange={}, our_viewchange={}",
        _0, _1, _2, _3
    )]
    OutOfSyncViewChange(u64, Hash, u32, u32),
}
