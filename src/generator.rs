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
use assert_matches::assert_matches;
use futures::sync::mpsc::UnboundedReceiver;
use futures::sync::oneshot;
use futures::{Async, Future, Poll, Stream};
use log::*;
use rand::seq::SliceRandom;
use std::time::Duration;
use stegos_crypto::curve1174::PublicKey;
use stegos_keychain::input;
use stegos_wallet::{Wallet, WalletNotification, WalletRequest, WalletResponse};
use tokio_timer::Delay;

static WAIT_TIMEOUT: Duration = Duration::from_secs(15);
const MIN_BALANCE: i64 = 10_000; // just some small number

pub struct Generator {
    // start generator with specific delay, because our network could be not ready.
    timeout: Option<Delay>,
    wallet: Wallet,
    password_file: String,
    destinations: Vec<PublicKey>,
    state: GeneratorState,
    mode: GeneratorMode,
}

#[derive(Debug)]
enum GeneratorState {
    NotInited(oneshot::Receiver<WalletResponse>),
    WaitForBalance(UnboundedReceiver<WalletNotification>),
    CreateNew,
    WaitForWallet(oneshot::Receiver<WalletResponse>),
    WaitForConfirmation(oneshot::Receiver<WalletResponse>),
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum GeneratorMode {
    Regular,
    ValueShuffle,
}

impl Generator {
    /// Crates new TransactionPool.
    pub fn new(
        wallet: Wallet,
        password_file: String,
        destinations: Vec<PublicKey>,
        mode: GeneratorMode,
        with_delay: bool,
    ) -> Generator {
        info!("Creating generator with keys = {:?}", destinations);
        assert!(
            !destinations.is_empty(),
            "Started generator without destinations keys."
        );
        let state = GeneratorState::NotInited(wallet.request(WalletRequest::BalanceInfo {}));
        let timeout = if with_delay {
            Delay::new(tokio_timer::clock::now() + WAIT_TIMEOUT).into()
        } else {
            None
        };
        Generator {
            timeout,
            wallet,
            password_file,
            destinations,
            state,
            mode,
        }
    }
    pub fn add_destinations(&mut self, destinations: Vec<PublicKey>) {
        info!("Adding keys to generator list: keys = {:?}", destinations);
        self.destinations.extend(destinations)
    }

    fn handle_wait_creation(&mut self, response: WalletResponse) {
        let tx_hash = match response {
            WalletResponse::TransactionCreated { tx_hash, .. }
                if self.mode == GeneratorMode::Regular =>
            {
                tx_hash
            }
            WalletResponse::ValueShuffleStarted { session_id }
                if self.mode == GeneratorMode::ValueShuffle =>
            {
                session_id
            }
            WalletResponse::Error { error } => {
                debug!("Error on transaction creation: error = {}", error);
                self.state =
                    GeneratorState::NotInited(self.wallet.request(WalletRequest::BalanceInfo {}));
                return;
            }
            e => {
                warn!(
                    "Unexpected WalletResponse = {:?}, mode = {:?}",
                    e, self.mode
                );
                return;
            }
        };
        debug!("Transaction was created: hash = {}", tx_hash);
        self.state = GeneratorState::WaitForConfirmation(
            self.wallet
                .request(WalletRequest::WaitForCommit { tx_hash }),
        );
    }

    /// Process wallet notification, transient to create new transaction.
    fn handle_wait_confirm(&mut self, response: WalletResponse) {
        match response {
            WalletResponse::TransactionCommitted(result) => {
                debug!("Transaction was processed: result = {:?}", result);
                self.state = GeneratorState::CreateNew;
            }
            WalletResponse::Error { error } => {
                debug!("Error on transaction creation: error:{}", error);
                self.state =
                    GeneratorState::NotInited(self.wallet.request(WalletRequest::BalanceInfo {}))
            }
            e => warn!("Unexpected WalletResponse = {:?}", e),
        }
    }

    /// Wait for wallet initialization.
    fn handle_wait_init(&mut self, info: WalletResponse) {
        match info {
            WalletResponse::BalanceInfo { balance } => self.try_init(balance),
            WalletResponse::Error { error } => {
                debug!("Error on requesting balance: error:{}", error);
                self.state =
                    GeneratorState::NotInited(self.wallet.request(WalletRequest::BalanceInfo {}))
            }
            e => warn!("Unexpected WalletResponse = {:?}", e),
        }
    }

    fn try_init(&mut self, balance: i64) {
        if balance < MIN_BALANCE {
            debug!(
                "Balance is too small, stop working, wait for balance changed = {}",
                balance
            );
            self.state = GeneratorState::WaitForBalance(self.wallet.subscribe());
        } else {
            debug!("Wallet inited, with balance = {}", balance);
            self.state = GeneratorState::CreateNew;
        }
    }

    /// Wait for balance changed.
    fn handle_wait_for_balance(&mut self, info: WalletNotification) {
        match info {
            WalletNotification::BalanceChanged { balance } => self.try_init(balance),
            _ => {} // we just waiting for concrete notification, ignore other.
        }
    }

    /// Start transaction generating, transient to GenerateTx.
    fn generate_tx(&mut self) {
        assert_matches!(self.state, GeneratorState::CreateNew);
        let mut rng = rand::thread_rng();

        let recipient = self.destinations.choose(&mut rng).unwrap().clone();
        let password =
            input::read_password(&self.password_file, false).expect("Failed to read password");
        let request = match self.mode {
            GeneratorMode::ValueShuffle => WalletRequest::SecurePayment {
                password,
                comment: "generator".into(),
                amount: 1,
                recipient,
                locked_timestamp: None,
            },
            GeneratorMode::Regular => WalletRequest::Payment {
                password,
                comment: "generator".into(),
                amount: 1,
                recipient,
                locked_timestamp: None,
            },
        };

        debug!("Sending new transaction: request={:?}", request);

        self.state = GeneratorState::WaitForWallet(self.wallet.request(request));
    }
}

impl Future for Generator {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if let Some(ref mut timer) = self.timeout {
            match timer.poll() {
                Ok(Async::Ready(_)) => {
                    debug!("Delay ended, start generating transactions.");
                    self.timeout = None;
                }
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Err(e) => panic!("failed to poll timer: error={}", e),
            }
        }
        loop {
            match self.state {
                GeneratorState::WaitForWallet(ref mut request) => match request.poll() {
                    Ok(Async::Ready(response)) => {
                        self.state = GeneratorState::CreateNew;
                        self.handle_wait_creation(response);
                    }
                    Ok(Async::NotReady) => break,
                    _ => panic!("Wallet disconnected."),
                },
                GeneratorState::WaitForConfirmation(ref mut receiver) => match receiver.poll() {
                    Ok(Async::Ready(response)) => {
                        self.state = GeneratorState::CreateNew;
                        self.handle_wait_confirm(response);
                    }
                    Ok(Async::NotReady) => break,
                    _ => panic!("Wallet disconnected."),
                },
                GeneratorState::CreateNew => {
                    info!("Starting transaction generator.");
                    self.generate_tx();
                }
                GeneratorState::NotInited(ref mut sender) => match sender.poll() {
                    Ok(Async::Ready(response)) => {
                        self.handle_wait_init(response);
                    }
                    Ok(Async::NotReady) => break,
                    _ => panic!("Node disconnected."),
                },
                GeneratorState::WaitForBalance(ref mut sender) => {
                    match sender.poll() {
                        Ok(Async::Ready(Some(response))) => {
                            // process all notifications, if receiving balance changed create new tx.
                            self.handle_wait_for_balance(response);
                        }
                        Ok(Async::NotReady) => break,
                        _ => panic!("Node disconnected."),
                    }
                }
            }
        }
        Ok(Async::NotReady)
    }
}
