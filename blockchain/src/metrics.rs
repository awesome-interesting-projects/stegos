//! Blockchain definition.

//
// Copyright (c) 2019 Stegos AG
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

use lazy_static::lazy_static;
use prometheus::*;

lazy_static! {
    pub static ref VALIDATOR_STAKE_GAUGEVEC: IntGaugeVec = register_int_gauge_vec!(
        "stegos_stakes_vector",
        "stakes per validator",
        &["validator"]
    )
    .unwrap();
    pub static ref EPOCH: IntCounter =
        register_int_counter!("stegos_blockchain_epoch", "Current blockchain epoch").unwrap();
    pub static ref OFFSET: IntGauge =
        register_int_gauge!("stegos_blockchain_offset", "Current microblock number").unwrap();
    pub static ref UTXO_LEN: IntGauge =
        register_int_gauge!("stegos_blockchain_utxo", "Size of UTXO map").unwrap();
}
