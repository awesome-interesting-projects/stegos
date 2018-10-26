//! Secure Pairings using BN Curve FR256 (type F, r approx 256 bits)

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

//! --------------------------------------------------------------------------
//! Field and group elements can be constructed from byte-vectors
//! with UTF8 hex chars, as in b"FF3C...". Never use str format "FF3C..."
//!
//! This pairing system is intended for blockchain BLS mulit-signatures, and
//! encrypted payloads in UTXO's. No math is performed on the individual groups,
//! and so we do not provide convenient infix access to such operations.
//! --------------------------------------------------------------------------

use super::*;
use hash::*;
use utils::*;

#[derive(Copy, Clone)]
#[repr(C)]
pub struct Zr([u8; ZR_SIZE_FR256]);

impl Zr {
    pub fn new() -> Zr {
        Zr(Zr::wv())
    }

    fn wv() -> [u8; ZR_SIZE_FR256] {
        [0u8; ZR_SIZE_FR256]
    }

    pub fn random() -> Zr {
        Zr(random::<[u8; ZR_SIZE_FR256]>())
    }

    pub fn base_vector(&self) -> &[u8] {
        &self.0
    }

    pub fn from_str(s: &str) -> Result<Zr, hex::FromHexError> {
        // result might be larger than prime order, r,
        // but will be interpreted by PBC lib as (Zr mod r).
        let mut v = Zr::wv();
        hexstr_to_bev_u8(&s, &mut v)?;
        Ok(Zr(v))
    }

    pub fn to_str(&self) -> String {
        u8v_to_typed_str("Zr", &self.base_vector())
    }
}

impl fmt::Display for Zr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

impl Hashable for Zr {
    fn hash(&self, state: &mut Hasher) {
        "Zr".hash(state);
        self.base_vector().hash(state);
    }
}

// -----------------------------------------
#[derive(Copy, Clone)]
#[repr(C)]
pub struct G1([u8; G1_SIZE_FR256]);

impl G1 {
    pub fn new() -> G1 {
        G1(G1::wv())
    }

    fn wv() -> [u8; G1_SIZE_FR256] {
        [0u8; G1_SIZE_FR256]
    }
    pub fn base_vector(&self) -> &[u8] {
        &self.0
    }

    pub fn to_str(&self) -> String {
        u8v_to_typed_str("G1", &self.base_vector())
    }

    pub fn from_str(s: &str) -> Result<G1, hex::FromHexError> {
        let mut v = G1::wv();
        hexstr_to_bev_u8(&s, &mut v)?;
        Ok(G1(v))
    }
}

impl fmt::Display for G1 {
    // for display of signatures
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

impl Hashable for G1 {
    fn hash(&self, state: &mut Hasher) {
        "G1".hash(state);
        self.base_vector().hash(state);
    }
}

// -----------------------------------------
#[derive(Copy, Clone)]
#[repr(C)]
pub struct G2([u8; G2_SIZE_FR256]);

impl G2 {
    pub fn new() -> G2 {
        G2(G2::wv())
    }

    fn wv() -> [u8; G2_SIZE_FR256] {
        [0u8; G2_SIZE_FR256]
    }

    pub fn base_vector(&self) -> &[u8] {
        &self.0
    }

    pub fn to_str(&self) -> String {
        u8v_to_typed_str("G2", &self.base_vector())
    }

    pub fn from_str(s: &str) -> Result<G2, hex::FromHexError> {
        let mut v = G2::wv();
        hexstr_to_bev_u8(&s, &mut v)?;
        Ok(G2(v))
    }
}

impl fmt::Display for G2 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

impl Hashable for G2 {
    fn hash(&self, state: &mut Hasher) {
        "G2".hash(state);
        self.base_vector().hash(state);
    }
}

// -----------------------------------------
#[derive(Copy, Clone)]
#[repr(C)]
pub struct GT([u8; GT_SIZE_FR256]);

impl GT {
    pub fn new() -> GT {
        GT(GT::wv())
    }

    fn wv() -> [u8; GT_SIZE_FR256] {
        [0u8; GT_SIZE_FR256]
    }

    pub fn base_vector(&self) -> &[u8] {
        &self.0
    }

    pub fn to_str(&self) -> String {
        u8v_to_typed_str("GT", &self.base_vector())
    }
}

impl fmt::Display for GT {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

impl Hashable for GT {
    fn hash(&self, state: &mut Hasher) {
        "GT".hash(state);
        self.base_vector().hash(state);
    }
}

// -----------------------------------------
#[derive(Copy, Clone)]
pub struct SecretKey(Zr);

impl SecretKey {
    pub fn base_vector(&self) -> &[u8] {
        self.0.base_vector()
    }

    pub fn to_str(&self) -> String {
        u8v_to_typed_str("SKey", &self.base_vector())
    }
}

impl fmt::Display for SecretKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

impl Hashable for SecretKey {
    fn hash(&self, state: &mut Hasher) {
        "SKey".hash(state);
        self.base_vector().hash(state);
    }
}

// -----------------------------------------
#[derive(Copy, Clone)]
pub struct PublicKey(G2);

impl PublicKey {
    pub fn base_vector(&self) -> &[u8] {
        self.0.base_vector()
    }

    pub fn to_str(&self) -> String {
        u8v_to_typed_str("PKey", &self.base_vector())
    }
}

impl fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

impl fmt::Display for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

impl Hashable for PublicKey {
    fn hash(&self, state: &mut Hasher) {
        "PKey".hash(state);
        self.base_vector().hash(state);
    }
}

// -----------------------------------------
#[derive(Copy, Clone)]
pub struct SecretSubKey(G1);

impl SecretSubKey {
    pub fn base_vector(&self) -> &[u8] {
        self.0.base_vector()
    }

    pub fn to_str(&self) -> String {
        u8v_to_typed_str("SSubKey", &self.base_vector())
    }
}

impl fmt::Display for SecretSubKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

impl Hashable for SecretSubKey {
    fn hash(&self, state: &mut Hasher) {
        "SSubKey".hash(state);
        self.base_vector().hash(state);
    }
}

// -----------------------------------------
#[derive(Copy, Clone)]
pub struct PublicSubKey(G2);

impl PublicSubKey {
    pub fn base_vector(&self) -> &[u8] {
        self.0.base_vector()
    }

    pub fn to_str(&self) -> String {
        u8v_to_typed_str("PSubKey", &self.base_vector())
    }
}

impl fmt::Display for PublicSubKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

impl Hashable for PublicSubKey {
    fn hash(&self, state: &mut Hasher) {
        "PSubKey".hash(state);
        self.base_vector().hash(state);
    }
}

// -----------------------------------------

#[derive(Copy, Clone)]
pub struct Signature(G1);

impl Signature {
    pub fn base_vector(&self) -> &[u8] {
        self.0.base_vector()
    }

    pub fn to_str(&self) -> String {
        u8v_to_typed_str("Sig", &self.base_vector())
    }
}

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

impl Hashable for Signature {
    fn hash(&self, state: &mut Hasher) {
        "Sig".hash(state);
        self.base_vector().hash(state);
    }
}

// -----------------------------------------

#[derive(Copy, Clone)]
pub struct BlsSignature {
    sig: Signature,
    pkey: PublicKey,
}

// ------------------------------------------------------------------------
// BLS Signature Generation & Checking

pub fn sign_hash(h: &Hash, skey: &SecretKey) -> Signature {
    // return a raw signature on a hash
    let v = G1::new();
    unsafe {
        rust_libpbc::sign_hash(
            PBC_CONTEXT_FR256 as u64,
            v.base_vector().as_ptr() as *mut _,
            skey.base_vector().as_ptr() as *mut _,
            h.base_vector().as_ptr() as *mut _,
            HASH_SIZE as u64,
        );
    }
    Signature(v)
}

pub fn check_hash(h: &Hash, sig: &Signature, pkey: &PublicKey) -> bool {
    // check a hash with a raw signature, return t/f
    unsafe {
        0 == rust_libpbc::check_signature(
            PBC_CONTEXT_FR256 as u64,
            sig.base_vector().as_ptr() as *mut _,
            h.base_vector().as_ptr() as *mut _,
            HASH_SIZE as u64,
            pkey.base_vector().as_ptr() as *mut _,
        )
    }
}

pub fn sign_message(msg: &[u8], skey: &SecretKey, pkey: &PublicKey) -> BlsSignature {
    // hash the message and form a BLS signature
    BlsSignature {
        sig: sign_hash(&Hash::from_vector(&msg), skey),
        pkey: pkey.clone(),
    }
}

pub fn check_message(msg: &[u8], sig: &BlsSignature) -> bool {
    // check the message against the BLS signature, return t/f
    check_hash(&Hash::from_vector(&msg), &sig.sig, &sig.pkey)
}

// ------------------------------------------------------------------
// Key Generation & Checking

pub fn make_deterministic_keys(seed: &[u8]) -> (SecretKey, PublicKey, Signature) {
    let h = Hash::from_vector(&seed);
    let sk = Zr::new(); // secret keys in Zr
    let pk = G2::new(); // public keys in G2
    unsafe {
        rust_libpbc::make_key_pair(
            PBC_CONTEXT_FR256 as u64,
            sk.base_vector().as_ptr() as *mut _,
            pk.base_vector().as_ptr() as *mut _,
            h.base_vector().as_ptr() as *mut _,
            HASH_SIZE as u64,
        );
    }
    let hpk = Hash::from_vector(&pk.base_vector());
    let skey = SecretKey(sk);
    let pkey = PublicKey(pk);
    let sig = sign_hash(&hpk, &skey);
    (skey, pkey, sig)
}

pub fn check_keying(pkey: &PublicKey, sig: &Signature) -> bool {
    check_hash(&Hash::from_vector(&pkey.base_vector()), &sig, &pkey)
}

pub fn make_random_keys() -> (SecretKey, PublicKey, Signature) {
    make_deterministic_keys(&Zr::random().base_vector())
}

// ------------------------------------------------------------------------
// Subkey generation and Sakai-Kasahara Encryption

pub fn make_secret_subkey(skey: &SecretKey, seed: &[u8]) -> SecretSubKey {
    let h = Hash::from_vector(&seed);
    let sk = G1::new();
    unsafe {
        rust_libpbc::make_secret_subkey(
            PBC_CONTEXT_FR256 as u64,
            sk.base_vector().as_ptr() as *mut _,
            skey.base_vector().as_ptr() as *mut _,
            h.base_vector().as_ptr() as *mut _,
            HASH_SIZE as u64,
        );
    }
    SecretSubKey(sk)
}

pub fn make_public_subkey(pkey: &PublicKey, seed: &[u8]) -> PublicSubKey {
    let h = Hash::from_vector(&seed);
    let pk = G2::new();
    unsafe {
        rust_libpbc::make_public_subkey(
            PBC_CONTEXT_FR256 as u64,
            pk.base_vector().as_ptr() as *mut _,
            pkey.base_vector().as_ptr() as *mut _,
            h.base_vector().as_ptr() as *mut _,
            HASH_SIZE as u64,
        );
    }
    PublicSubKey(pk)
}

// -----------------------------------------------------
// Sakai-Hasakara Encryption

#[derive(Copy, Clone)]
pub struct RVal(G2);

impl RVal {
    // TODO: new() should be removed later, just here for testing for now
    pub fn new() -> RVal {
        RVal(G2::new())
    }

    pub fn base_vector(&self) -> &[u8] {
        self.0.base_vector()
    }

    pub fn to_str(&self) -> String {
        u8v_to_typed_str("RVal", &self.base_vector())
    }
}

impl fmt::Display for RVal {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_str())
    }
}

impl Hashable for RVal {
    fn hash(&self, state: &mut Hasher) {
        "RVal".hash(state);
        self.base_vector().hash(state);
    }
}

// structure of a SAKKI encryption.
// ---------------------------------
// For use in UTXO's you will only want to store the
// ciphertext, cmsg, and the rval. Proper recipients
// already know their own public keys, and the IBE ID
// that was used to encrypt their payload.
// ----------------------------------
pub struct EncryptedPacket {
    pkey: PublicKey, // public key of recipient
    id: Vec<u8>,     // IBE ID
    rval: RVal,      // R_val used for SAKE encryption
    cmsg: Vec<u8>,   // encrypted payload
}

pub fn ibe_encrypt(msg: &[u8], pkey: &PublicKey, id: &[u8]) -> EncryptedPacket {
    let nmsg = msg.len();

    // compute IBE public key
    let pkid = make_public_subkey(&pkey, &id);

    // compute hash of concatenated id:msg
    let mut concv = Vec::from(id);
    for b in msg.to_vec() {
        concv.push(b);
    }
    let rhash = Hash::from_vector(&concv);

    let rval = G2::new();
    let pval = GT::new();
    unsafe {
        rust_libpbc::sakai_kasahara_encrypt(
            PBC_CONTEXT_FR256 as u64,
            rval.base_vector().as_ptr() as *mut _,
            pval.base_vector().as_ptr() as *mut _,
            pkid.base_vector().as_ptr() as *mut _,
            rhash.base_vector().as_ptr() as *mut _,
            HASH_SIZE as u64,
        );
    }
    // encrypt with (msg XOR H(pairing-val))
    let mut cmsg = hash_nbytes(nmsg, &pval.base_vector());
    for ix in 0..nmsg {
        cmsg[ix] ^= msg[ix];
    }
    EncryptedPacket {
        pkey: *pkey,
        id: id.to_vec(),
        rval: RVal(rval),
        cmsg: cmsg,
    }
}

pub fn ibe_decrypt(pack: &EncryptedPacket, skey: &SecretKey) -> Option<Vec<u8>> {
    let skid = make_secret_subkey(&skey, &pack.id);
    let pkid = make_public_subkey(&pack.pkey, &pack.id);
    let nmsg = pack.cmsg.len();
    let pval = GT::new();
    unsafe {
        rust_libpbc::sakai_kasahara_decrypt(
            PBC_CONTEXT_FR256 as u64,
            pval.base_vector().as_ptr() as *mut _,
            pack.rval.base_vector().as_ptr() as *mut _,
            skid.base_vector().as_ptr() as *mut _,
        );
    }
    // decrypt using (ctxt XOR H(pairing_val))
    let mut msg = hash_nbytes(nmsg, &pval.base_vector());
    for ix in 0..nmsg {
        msg[ix] ^= pack.cmsg[ix];
    }
    // Now check that message was correctly decrypted
    // compute hash of concatenated id:msg
    let mut concv = pack.id.clone();
    for b in msg.clone() {
        concv.push(b);
    }
    let rhash = Hash::from_vector(&concv);
    unsafe {
        let ans = rust_libpbc::sakai_kasahara_check(
            PBC_CONTEXT_FR256 as u64,
            pack.rval.base_vector().as_ptr() as *mut _,
            pkid.base_vector().as_ptr() as *mut _,
            rhash.base_vector().as_ptr() as *mut _,
            HASH_SIZE as u64,
        );
        if ans == 0 {
            Some(msg)
        } else {
            None
        }
    }
}