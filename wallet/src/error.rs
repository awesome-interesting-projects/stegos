//! Wallet - Errors.

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
#[derive(Debug, Fail, PartialEq, Eq)]
pub enum WalletError {
    #[fail(display = "Not enough money.")]
    NotEnoughMoney,
    #[fail(display = "No unspent public outputs was found.")]
    NoPublicOutputs,
    #[fail(display = "Negative amount: amount={}", _0)]
    NegativeAmount(i64),
    #[fail(display = "Insufficient stake: min={}, got={}.", _0, _1)]
    InsufficientStake(i64, i64),
    #[fail(display = "Incorrect TXIN type")]
    IncorrectTXINType,
    #[fail(display = "Incorrect UTXO data")]
    InvalidUTXOData,
    #[fail(display = "Nothing to re-stake")]
    NothingToRestake,
    #[fail(display = "Snownall already started")]
    SnowballBusy,
}
