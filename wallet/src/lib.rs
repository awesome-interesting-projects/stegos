//! Wallet.

//
// Copyright (c) 2018 Stegos
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

#![deny(warnings)]

mod change;
mod error;
mod transaction;

pub use crate::transaction::*;
use failure::Error;
use log::*;
use std::collections::HashMap;
use stegos_blockchain::Output;
use stegos_blockchain::PaymentOutput;
use stegos_blockchain::StakeOutput;
use stegos_blockchain::Transaction;
use stegos_crypto::curve1174::cpt::PublicKey;
use stegos_crypto::curve1174::cpt::SecretKey;
use stegos_crypto::hash::Hash;
use stegos_crypto::pbc::secure;

pub enum WalletNotification {
    BalanceChanged { balance: i64 },
    MessageReceived { msg: Vec<u8>, prune_tx: Transaction },
}

pub struct Wallet {
    /// Secret Key.
    skey: SecretKey,
    /// Public Key.
    pkey: PublicKey,
    /// Unspent Payment UXTO.
    unspent: HashMap<Hash, (PaymentOutput, i64)>,
    /// Unspent Stake UTXO.
    unspent_stakes: HashMap<Hash, StakeOutput>,
    /// Calculated Node's balance.
    balance: i64,
}

impl Wallet {
    /// Create a new wallet.
    pub fn new(skey: SecretKey, pkey: PublicKey) -> Self {
        let unspent: HashMap<Hash, (PaymentOutput, i64)> = HashMap::new();
        let unspent_stakes: HashMap<Hash, StakeOutput> = HashMap::new();
        let balance: i64 = 0;
        Wallet {
            skey,
            pkey,
            unspent,
            unspent_stakes,
            balance,
        }
    }

    /// Send money.
    pub fn payment(&self, recipient: &PublicKey, amount: i64) -> Result<Transaction, Error> {
        let tx =
            create_payment_transaction(&self.skey, &self.pkey, recipient, &self.unspent, amount)?;
        Ok(tx)
    }

    /// Send message.
    pub fn message(
        &self,
        recipient: &PublicKey,
        ttl: u64,
        data: Vec<u8>,
    ) -> Result<Transaction, Error> {
        let tx =
            create_data_transaction(&self.skey, &self.pkey, recipient, &self.unspent, ttl, data)?;
        Ok(tx)
    }

    /// Stake money into the escrow.
    pub fn stake(
        &self,
        validator_pkey: &secure::PublicKey,
        amount: i64,
    ) -> Result<Transaction, Error> {
        let tx = create_staking_transaction(
            &self.skey,
            &self.pkey,
            validator_pkey,
            &self.unspent,
            amount,
        )?;
        Ok(tx)
    }

    /// Unstake money from the escrow.
    /// NOTE: amount must include PAYMENT_FEE.
    pub fn unstake(
        &self,
        validator_pkey: &secure::PublicKey,
        amount: i64,
    ) -> Result<Transaction, Error> {
        let tx = create_unstaking_transaction(
            &self.skey,
            &self.pkey,
            validator_pkey,
            &self.unspent_stakes,
            amount,
        )?;
        Ok(tx)
    }

    /// Unstake all of the money from the escrow.
    pub fn unstake_all(&self, validator_pkey: &secure::PublicKey) -> Result<Transaction, Error> {
        let mut amount: i64 = 0;
        for output in self.unspent_stakes.values() {
            amount += output.amount;
        }
        self.unstake(validator_pkey, amount)
    }

    /// Called when outputs registered and/or pruned.
    pub fn on_outputs_changed(
        &mut self,
        inputs: Vec<Output>,
        outputs: Vec<Output>,
    ) -> Vec<WalletNotification> {
        let mut notifications: Vec<WalletNotification> = Vec::new();

        let saved_balance = self.balance;

        for input in inputs {
            self.on_output_pruned(input);
        }

        for output in outputs {
            if let Some(notification) = self.on_output_created(output) {
                notifications.push(notification);
            }
        }

        if saved_balance != self.balance {
            let balance = self.balance;
            let notification = WalletNotification::BalanceChanged { balance };
            notifications.push(notification);
        }

        notifications
    }

    /// Called when UTXO is created.
    fn on_output_created(&mut self, output: Output) -> Option<WalletNotification> {
        let hash = Hash::digest(&output);
        match output {
            Output::PaymentOutput(o) => {
                if let Ok((_delta, _gamma, amount)) = o.decrypt_payload(&self.skey) {
                    info!("Received payment UTXO: hash={}, amount={}", hash, amount);
                    let missing = self.unspent.insert(hash, (o, amount));
                    assert!(missing.is_none());
                    assert!(amount >= 0);
                    self.balance += amount
                }
            }
            Output::DataOutput(o) => {
                if let Ok((_delta, _gamma, msg)) = o.decrypt_payload(&self.skey) {
                    info!(
                        "Received data UTXO: hash={}, msg={}",
                        hash,
                        String::from_utf8_lossy(&msg)
                    );

                    // Send a prune transaction.
                    debug!("Pruning data");
                    let prune_tx =
                        create_data_pruning_transaction(&self.skey, o).expect("cannot fail");
                    debug!("Created transaction: hash={}", Hash::digest(&prune_tx.body));

                    // Notify subscribers.
                    let notification = WalletNotification::MessageReceived { msg, prune_tx };
                    return Some(notification);
                }
            }
            Output::StakeOutput(o) => {
                if let Ok(_delta) = o.decrypt_payload(&self.skey) {
                    info!("Staked money to escrow: hash={}, amount={}", hash, o.amount);
                    let missing = self.unspent_stakes.insert(hash, o);
                    assert!(missing.is_none());
                }
            }
        };
        None
    }

    /// Called when UTXO is spent.
    fn on_output_pruned(&mut self, output: Output) {
        let hash = Hash::digest(&output);
        match output {
            Output::PaymentOutput(o) => {
                if let Ok((_delta, _gamma, amount)) = o.decrypt_payload(&self.skey) {
                    info!("Spent payment UTXO: hash={}, amount={}", hash, amount);
                    let exists = self.unspent.remove(&hash);
                    assert!(exists.is_some());
                    self.balance -= amount;
                    assert!(self.balance >= 0);
                }
            }
            Output::DataOutput(o) => {
                if let Ok((_delta, _gamma, data)) = o.decrypt_payload(&self.skey) {
                    info!(
                        "Pruned data UTXO: hash={}, msg={}",
                        hash,
                        String::from_utf8_lossy(&data)
                    );
                }
            }
            Output::StakeOutput(o) => {
                if let Ok(_delta) = o.decrypt_payload(&self.skey) {
                    info!(
                        "Unstaked money from escrow: hash={}, amount={}",
                        hash, o.amount
                    );
                    let exists = self.unspent_stakes.remove(&hash);
                    assert!(exists.is_some());
                }
            }
        }
    }
}
