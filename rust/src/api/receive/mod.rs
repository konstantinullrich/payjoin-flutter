pub use std::str::FromStr;
pub use std::sync::Arc;

use flutter_rust_bridge::{frb, DartFnFuture};
use payjoin::persist::Persister;
use payjoin::receive::v2 as inner;
pub use payjoin::OutputSubstitution;
use payjoin_ffi::bitcoin_ffi;

use super::uri::{FfiOhttpKeys, FfiPjUri};
use crate::api::bitcoin_ffi::FfiScript;
use crate::api::receive::error::{
    FfiError, FfiImplementationError, FfiInputContributionError, FfiJsonReply,
    FfiOutputSubstitutionError, FfiPsbtInputError, FfiReplyableError, FfiSelectionError,
    FfiSessionError,
};
use crate::api::uri::error::FfiIntoUrlError;
use crate::api::FfiSerdeJsonError;
use crate::frb_generated::RustOpaque;
use crate::utils::types::{ClientResponse, Network, OutPoint, PsbtInput, Request, TxIn, TxOut};

pub mod error;

#[derive(Clone, Debug)]
pub struct FfiNewReceiver(pub RustOpaque<payjoin_ffi::receive::NewReceiver>);

impl From<payjoin_ffi::receive::NewReceiver> for FfiNewReceiver {
    fn from(value: payjoin_ffi::receive::NewReceiver) -> Self {
        Self(RustOpaque::new(value))
    }
}

impl FfiNewReceiver {
    /// Creates a new `FfiReceiver` with the provided parameters.
    ///
    /// # Parameters
    /// - `address`: The Bitcoin address for the payjoin session.
    /// - `network`: The network to use for address verification.
    /// - `directory`: The URL of the store-and-forward payjoin directory.
    /// - `ohttp_keys`: The OHTTP keys used for encrypting and decrypting HTTP requests and responses.
    /// - `ohttp_relay`: The URL of the OHTTP relay, used to keep client IP address confidential.
    /// - `expire_after`: The duration in seconds after which the session expires.
    ///
    /// # Returns
    /// A new instance of `FfiReceiver`.
    ///
    /// # References
    /// - [BIP 77: Payjoin Version 2: Serverless Payjoin](https://github.com/bitcoin/bips/pull/1483)
    #[frb(sync)]
    pub fn create(
        address: String,
        network: Network,
        directory: String,
        ohttp_keys: FfiOhttpKeys,
        expire_after: Option<u64>,
    ) -> Result<Self, FfiIntoUrlError> {
        let address = bitcoin_ffi::Address::new(address, network.into())
            .expect("TODO: handle parse address error, perhaps by passing Address instead of String,Network");
        payjoin_ffi::receive::NewReceiver::new(
            address,
            directory.into(),
            ohttp_keys.into(),
            expire_after,
        )
        .map(Into::into)
        .map_err(Into::into)
    }

    pub fn persist(
        &self,
        persister: &mut DartReceiverPersister,
    ) -> Result<ReceiverToken, FfiImplementationError> {
        self.0.persist(persister).map(Into::into).map_err(Into::into)
    }
}

impl FfiReceiver {
    pub fn load(
        token: ReceiverToken,
        persister: DartReceiverPersister,
    ) -> Result<Self, FfiImplementationError> {
        persister
            .load(token.into())
            .map(|receiver| FfiReceiver::from(payjoin_ffi::receive::Receiver::from(receiver)))
            .map_err(Into::into)
    }

    ///The per-session public key to use as an identifier
    #[frb(sync)]
    pub fn id(&self) -> String {
        self.0.id()
    }

    pub fn pj_uri(&self) -> FfiPjUri {
        self.0.pj_uri().into()
    }

    pub fn extract_req(&self, ohttp_relay: String) -> Result<(Request, ClientResponse), FfiError> {
        self.0.extract_req(ohttp_relay).map(|e| (e.0.into(), e.1.into())).map_err(Into::into)
    }
    pub fn process_res(
        &self,
        body: &[u8],
        ctx: &ClientResponse,
    ) -> Result<Option<FfiUncheckedProposal>, FfiError> {
        self.0.process_res(body, &ctx.into()).map(|e| e.map(|o| o.into())).map_err(Into::into)
    }

    #[frb(sync)]
    pub fn to_json(&self) -> Result<String, FfiSerdeJsonError> {
        self.0.to_json().map_err(Into::into)
    }

    #[frb(sync)]
    pub fn from_json(json: String) -> Result<Self, FfiSerdeJsonError> {
        payjoin_ffi::receive::Receiver::from_json(&json).map(Into::into).map_err(Into::into)
    }

    #[frb(sync)]
    pub fn key(&self) -> ReceiverToken {
        self.0.key().into()
    }
}

#[derive(Clone, Debug)]
pub struct FfiReceiver(pub(crate) payjoin_ffi::receive::Receiver);

impl From<payjoin_ffi::receive::Receiver> for FfiReceiver {
    fn from(value: payjoin_ffi::receive::Receiver) -> Self {
        Self(value)
    }
}

#[derive(Clone)]
pub struct FfiUncheckedProposal(pub RustOpaque<payjoin_ffi::receive::UncheckedProposal>);
impl From<payjoin_ffi::receive::UncheckedProposal> for FfiUncheckedProposal {
    fn from(value: payjoin_ffi::receive::UncheckedProposal) -> Self {
        Self(RustOpaque::new(value))
    }
}
impl FfiUncheckedProposal {
    ///The Sender’s Original PSBT
    pub fn extract_tx_to_schedule_broadcast(&self) -> Vec<u8> {
        self.0.extract_tx_to_schedule_broadcast()
    }
    /// Call after checking that the Original PSBT can be broadcast.
    ///
    /// Receiver MUST check that the Original PSBT from the sender can be broadcast, i.e. testmempoolaccept bitcoind rpc returns { “allowed”: true,.. } for get_transaction_to_check_broadcast() before calling this method.
    ///
    /// Do this check if you generate bitcoin uri to receive Payjoin on sender request without manual human approval, like a payment processor. Such so called “non-interactive” receivers are otherwise vulnerable to probing attacks. If a sender can make requests at will, they can learn which bitcoin the receiver owns at no cost. Broadcasting the Original PSBT after some time in the failure case makes incurs sender cost and prevents probing.
    ///
    /// Call this after checking downstream.
    pub fn check_broadcast_suitability(
        &self,
        min_fee_rate: Option<u64>,
        can_broadcast: impl Fn(Vec<u8>) -> DartFnFuture<bool>,
    ) -> Result<FfiMaybeInputsOwned, FfiReplyableError> {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        self.0
            .check_broadcast_suitability(min_fee_rate, |x| {
                Ok(runtime.block_on(can_broadcast(x.clone())))
            })
            .map(Into::into)
            .map_err(Into::into)
    }
    /// Call this method if the only way to initiate a Payjoin with this receiver
    /// requires manual intervention, as in most consumer wallets.
    ///
    /// So-called "non-interactive" receivers, like payment processors, that allow arbitrary requests are otherwise vulnerable to probing attacks.
    /// Those receivers call `extract_tx_to_check_broadcast()` and `attest_tested_and_scheduled_broadcast()` after making those checks downstream.
    pub fn assume_interactive_receiver(&self) -> FfiMaybeInputsOwned {
        self.0.clone().assume_interactive_receiver().into()
    }

    /// Extract an OHTTP Encapsulated HTTP POST request to return
    /// a Receiver Error Response
    pub fn extract_err_req(
        &self,
        err: FfiJsonReply,
        ohttp_relay: String,
    ) -> Result<(Request, ClientResponse), FfiSessionError> {
        self.0
            .extract_err_req(&(err.into()), ohttp_relay)
            .map(|e| (e.0.into(), e.1.into()))
            .map_err(Into::into)
    }

    /// Process an OHTTP Encapsulated HTTP POST Error response
    /// to ensure it has been posted properly
    pub fn process_err_res(
        &self,
        body: &[u8],
        context: &ClientResponse,
    ) -> Result<(), FfiSessionError> {
        self.0.process_err_res(body, &context.into()).map_err(Into::into)
    }
}
#[derive(Clone)]
pub struct FfiMaybeInputsOwned(pub RustOpaque<payjoin_ffi::receive::MaybeInputsOwned>);

impl From<payjoin_ffi::receive::MaybeInputsOwned> for FfiMaybeInputsOwned {
    fn from(value: payjoin_ffi::receive::MaybeInputsOwned) -> Self {
        Self(RustOpaque::new(value))
    }
}
impl FfiMaybeInputsOwned {
    ///Check that the Original PSBT has no receiver-owned inputs. Return original-psbt-rejected error or otherwise refuse to sign undesirable inputs.
    /// An attacker could try to spend receiver's own inputs. This check prevents that.
    pub fn check_inputs_not_owned(
        &self,
        is_owned: impl Fn(Vec<u8>) -> DartFnFuture<bool>,
    ) -> Result<FfiMaybeInputsSeen, FfiReplyableError> {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        self.0
            .check_inputs_not_owned(|o| Ok(runtime.block_on(is_owned(o.clone()))))
            .map(Into::into)
            .map_err(Into::into)
    }
}

pub struct FfiMaybeInputsSeen(pub RustOpaque<payjoin_ffi::receive::MaybeInputsSeen>);

impl From<payjoin_ffi::receive::MaybeInputsSeen> for FfiMaybeInputsSeen {
    fn from(value: payjoin_ffi::receive::MaybeInputsSeen) -> Self {
        Self(RustOpaque::new(value))
    }
}
impl FfiMaybeInputsSeen {
    /// Make sure that the original transaction inputs have never been seen before.
    /// This prevents probing attacks. This prevents reentrant Payjoin, where a sender
    /// proposes a Payjoin PSBT as a new Original PSBT for a new Payjoin.
    pub fn check_no_inputs_seen_before(
        &self,
        is_known: impl Fn(OutPoint) -> DartFnFuture<bool>,
    ) -> Result<FfiOutputsUnknown, FfiReplyableError> {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        self.0
            .check_no_inputs_seen_before(|o| Ok(runtime.block_on(is_known(o.into()))))
            .map(Into::into)
            .map_err(Into::into)
    }
}

pub struct FfiOutputsUnknown(pub RustOpaque<payjoin_ffi::receive::OutputsUnknown>);

impl From<payjoin_ffi::receive::OutputsUnknown> for FfiOutputsUnknown {
    fn from(value: payjoin_ffi::receive::OutputsUnknown) -> Self {
        Self(RustOpaque::new(value))
    }
}
impl FfiOutputsUnknown {
    /// Find which outputs belong to the receiver
    pub fn identify_receiver_outputs(
        &self,
        is_receiver_output: impl Fn(Vec<u8>) -> DartFnFuture<bool>,
    ) -> Result<FfiWantsOutputs, FfiReplyableError> {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        self.0
            .identify_receiver_outputs(|o| Ok(runtime.block_on(is_receiver_output(o.clone()))))
            .map(Into::into)
            .map_err(Into::into)
    }
}

pub struct FfiWantsOutputs(pub RustOpaque<payjoin_ffi::receive::WantsOutputs>);

impl From<payjoin_ffi::receive::WantsOutputs> for FfiWantsOutputs {
    fn from(value: payjoin_ffi::receive::WantsOutputs) -> Self {
        Self(RustOpaque::new(value))
    }
}

impl FfiWantsOutputs {
    pub fn output_substitution(&self) -> OutputSubstitution {
        self.0.output_substitution()
    }

    pub fn replace_receiver_outputs(
        &self,
        replacement_outputs: Vec<TxOut>,
        drain_script: &FfiScript,
    ) -> Result<FfiWantsOutputs, FfiOutputSubstitutionError> {
        let replacement_outputs: Vec<bitcoin_ffi::TxOut> =
            replacement_outputs.iter().map(|o| o.clone().into()).collect();
        self.0
            .clone()
            .replace_receiver_outputs(replacement_outputs, &drain_script.into())
            .map(Into::into)
            .map_err(Into::into)
    }

    pub fn substitute_receiver_script(
        &self,
        output_script: &FfiScript,
    ) -> Result<FfiWantsOutputs, FfiOutputSubstitutionError> {
        self.0
            .clone()
            .substitute_receiver_script(&output_script.into())
            .map(Into::into)
            .map_err(Into::into)
    }

    pub fn commit_outputs(&self) -> FfiWantsInputs {
        self.0.clone().commit_outputs().into()
    }
}

pub struct FfiWantsInputs(pub RustOpaque<payjoin_ffi::receive::WantsInputs>);

impl From<payjoin_ffi::receive::WantsInputs> for FfiWantsInputs {
    fn from(value: payjoin_ffi::receive::WantsInputs) -> Self {
        Self(RustOpaque::new(value))
    }
}

impl FfiWantsInputs {
    pub fn try_preserving_privacy(
        &self,
        candidate_inputs: Vec<FfiInputPair>,
    ) -> Result<FfiInputPair, FfiSelectionError> {
        let candidate_inputs: Vec<payjoin_ffi::receive::InputPair> =
            candidate_inputs.into_iter().map(Into::into).collect();
        self.0.clone().try_preserving_privacy(candidate_inputs).map(Into::into).map_err(Into::into)
    }

    pub fn contribute_inputs(
        &self,
        replacement_inputs: Vec<FfiInputPair>,
    ) -> Result<FfiWantsInputs, FfiInputContributionError> {
        let replacement_inputs: Vec<payjoin_ffi::receive::InputPair> =
            replacement_inputs.into_iter().map(Into::into).collect();
        self.0.clone().contribute_inputs(replacement_inputs).map(Into::into).map_err(Into::into)
    }

    pub fn commit_inputs(&self) -> FfiProvisionalProposal {
        self.0.clone().commit_inputs().into()
    }
}

#[derive(Debug, Clone)]
pub struct FfiInputPair(pub RustOpaque<payjoin_ffi::receive::InputPair>);

impl FfiInputPair {
    pub fn new(txin: TxIn, psbtin: PsbtInput) -> Result<Self, FfiPsbtInputError> {
        payjoin_ffi::receive::InputPair::new(txin.into(), psbtin.into())
            .map(Into::into)
            .map_err(Into::into)
    }
}

impl From<payjoin_ffi::receive::InputPair> for FfiInputPair {
    fn from(value: payjoin_ffi::receive::InputPair) -> Self {
        Self(RustOpaque::new(value))
    }
}

impl From<FfiInputPair> for payjoin_ffi::receive::InputPair {
    fn from(value: FfiInputPair) -> Self {
        (*value.0).clone()
    }
}

pub struct FfiProvisionalProposal(pub RustOpaque<payjoin_ffi::receive::ProvisionalProposal>);
impl From<payjoin_ffi::receive::ProvisionalProposal> for FfiProvisionalProposal {
    fn from(value: payjoin_ffi::receive::ProvisionalProposal) -> Self {
        Self(RustOpaque::new(value))
    }
}
impl FfiProvisionalProposal {
    pub fn finalize_proposal(
        &self,
        process_psbt: impl Fn(String) -> DartFnFuture<String>, // TODO return Result<String, ImplementationError>
        min_fee_rate_sat_per_vb: Option<u64>,
        max_fee_rate_sat_per_vb: Option<u64>,
    ) -> Result<FfiPayjoinProposal, FfiReplyableError> {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        self.0
            .finalize_proposal(
                |o| Ok(runtime.block_on(process_psbt(o.clone()))),
                min_fee_rate_sat_per_vb,
                max_fee_rate_sat_per_vb,
            )
            .map(Into::into)
            .map_err(Into::into)
    }
}
pub struct FfiPayjoinProposal(pub RustOpaque<payjoin_ffi::receive::PayjoinProposal>);
impl From<payjoin_ffi::receive::PayjoinProposal> for FfiPayjoinProposal {
    fn from(value: payjoin_ffi::receive::PayjoinProposal) -> Self {
        Self(RustOpaque::new(value))
    }
}
impl FfiPayjoinProposal {
    pub fn utxos_to_be_locked(&self) -> Vec<OutPoint> {
        self.0.utxos_to_be_locked().iter().map(|x| x.into()).collect()
    }

    pub fn psbt(&self) -> String {
        self.0.psbt()
    }

    pub fn extract_req(&self, ohttp_relay: String) -> Result<(Request, ClientResponse), FfiError> {
        self.0
            .clone()
            .extract_req(ohttp_relay)
            .map(|e| (e.0.into(), e.1.into()))
            .map_err(Into::into)
    }

    pub fn process_res(&self, res: &[u8], ohttp_context: &ClientResponse) -> Result<(), FfiError> {
        self.0.process_res(res, &ohttp_context.into()).map_err(Into::into)
    }
}

pub struct ReceiverToken(pub(crate) Arc<inner::ReceiverToken>);

impl From<inner::ReceiverToken> for ReceiverToken {
    fn from(value: inner::ReceiverToken) -> Self {
        Self(Arc::new(value))
    }
}

impl From<ReceiverToken> for inner::ReceiverToken {
    fn from(value: ReceiverToken) -> Self {
        (*value.0).clone()
    }
}

impl ReceiverToken {
    /// Convert the receiver token to a byte array
    /// This is most useful when storing the token as a key in a map
    #[frb(sync)]
    pub fn to_bytes(&self) -> Vec<u8> {
        (*self.0).as_ref().to_vec()
    }
}

pub trait ReceiverPersister: Send + Sync {
    fn save(
        &self,
        receiver: FfiReceiver,
    ) -> Result<ReceiverToken, payjoin_ffi::receive::ImplementationError>;
    fn load(
        &self,
        token: ReceiverToken,
    ) -> Result<FfiReceiver, payjoin_ffi::receive::ImplementationError>;
}

#[frb(opaque)]
pub struct DartReceiverPersister {
    save_cb: Box<
        dyn Fn(FfiReceiver) -> DartFnFuture<Result<ReceiverToken, anyhow::Error>> + Send + Sync,
    >,
    load_cb: Box<
        dyn Fn(ReceiverToken) -> DartFnFuture<Result<FfiReceiver, anyhow::Error>> + Send + Sync,
    >,
}

impl DartReceiverPersister {
    #[frb(sync)]
    pub fn new(
        save: impl Fn(FfiReceiver) -> DartFnFuture<Result<ReceiverToken, anyhow::Error>>
            + Send
            + Sync
            + 'static,
        load: impl Fn(ReceiverToken) -> DartFnFuture<Result<FfiReceiver, anyhow::Error>>
            + Send
            + Sync
            + 'static,
    ) -> DartReceiverPersister {
        DartReceiverPersister { save_cb: Box::new(save), load_cb: Box::new(load) }
    }
}

impl payjoin::persist::Persister<payjoin::receive::v2::Receiver> for DartReceiverPersister {
    type Token = inner::ReceiverToken;
    type Error = payjoin_ffi::receive::ImplementationError;

    fn save(
        &mut self,
        receiver: payjoin::receive::v2::Receiver,
    ) -> Result<Self::Token, Self::Error> {
        println!("Rust: Entering Persister:save");
        let receiver = FfiReceiver::from(payjoin_ffi::receive::Receiver::from(receiver));
        println!("Rust: Calling save_cb");
        let result = tokio::runtime::Runtime::new()
            .expect("Failed to create Tokio runtime")
            .block_on(async {
                (self.save_cb)(receiver)
                    .await
                    .map(Into::into)
                    .map_err(|e| payjoin_ffi::receive::ImplementationError::from(e.to_string()))
            });
        println!("Rust: save_cb returned");
        result
    }

    fn load(&self, token: Self::Token) -> Result<payjoin::receive::v2::Receiver, Self::Error> {
        tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime").block_on(async {
            (self.load_cb)(token.into())
                .await
                .map(|receiver| {
                    let ffi_receiver = receiver.0;
                    payjoin::receive::v2::Receiver::from(ffi_receiver)
                })
                .map_err(|e| payjoin_ffi::receive::ImplementationError::from(e.to_string()))
        })
    }
}
