use codec::{Decode, Encode};
use log::debug;
use sc_executor::RuntimeVersion;

use sp_api::{
    ApiError, ApiExt, CallApiAt, CallApiAtParams, InitializeBlock, ProofRecorder, RuntimeApiInfo,
    StorageChanges, StorageTransactionCache, TransactionOutcome,
};

use sp_core::{ExecutionContext, NativeOrEncoded, OpaqueMetadata};
use sp_runtime::{
    generic::BlockId,
    traits::{Block as BlockT, HashFor, Header as HeaderT, NumberFor},
    ApplyExtrinsicResult, DispatchError, KeyTypeId,
};
use sp_state_machine::OverlayedChanges;
use sp_trie::StorageProof;
use std::marker::PhantomData;

use pallet_grandpa::fg_primitives;
use pallet_grandpa::{AuthorityId as GrandpaId, AuthorityList as GrandpaAuthorityList};
use sp_consensus_babe::{BabeGenesisConfiguration, Epoch, Slot};
use sp_inherents::{CheckInherentsResult, InherentData};
use sp_runtime::transaction_validity::TransactionValidityError;
pub use sp_state_machine::ChangesTrieState;
use std::cell::RefCell;

// empty MockRuntimeApi used to passing to bin/cli/service.rs
pub struct MockRuntimeApi {}

impl<Block, C> sp_api::ConstructRuntimeApi<Block, C> for MockRuntimeApi
where
    //B: sc_client_api::backend::Backend<Block> + 'static + Send + Sync,
    Block: BlockT,
    C: CallApiAt<Block> + 'static + Send + Sync,
    C::StateBackend: sp_api::StateBackend<sp_api::HashFor<Block>>,
{
    // the associate RuntimeApi is MockRuntimeApiImpl which implements all mock api
    // type RuntimeApi = MockRuntimeApiImpl<C::StateBackend, C, Block>;
    type RuntimeApi = MockRuntimeApiImpl<C, Block>;

    fn construct_runtime_api<'a>(call: &'a C) -> sp_api::ApiRef<'a, Self::RuntimeApi> {
        MockRuntimeApiImpl {
            call: unsafe { std::mem::transmute(call) },
            initialized_block: None.into(),
            changes: Default::default(),
            storage: Default::default(),
            _ph: Default::default(),
            // _ph2: Default::default(),
        }
        .into()
    }
}

#[allow(missing_docs)]
pub struct MockRuntimeApiImpl<C: 'static, Block>
where
    Block: BlockT,
    C: CallApiAt<Block> + 'static + Send + Sync,
    C::StateBackend: sp_api::StateBackend<sp_api::HashFor<Block>>,
    // B: sc_client_api::backend::Backend<Block> + 'static + Send + Sync,
    // B::State: sp_api::StateBackend<sp_api::HashFor<Block>>,
{
    pub call: &'static C,
    pub initialized_block: RefCell<Option<BlockId<Block>>>,
    pub changes: RefCell<OverlayedChanges>,
    pub storage: RefCell<StorageTransactionCache<Block, C::StateBackend>>,
    // pub storage: RefCell<StorageTransactionCache<Block, B::State>>,
    pub _ph: PhantomData<Block>,
    // pub _ph2: PhantomData<B>,
}

unsafe impl<C, Block> Sync for MockRuntimeApiImpl<C, Block>
where
    Block: BlockT,
    C: CallApiAt<Block> + 'static + Send + Sync,
    C::StateBackend: sp_api::StateBackend<sp_api::HashFor<Block>>,
    // B: sc_client_api::backend::Backend<Block> + 'static + Send + Sync,
    // B::State: sp_api::StateBackend<sp_api::HashFor<Block>>,
{
}

unsafe impl<C, Block> Send for MockRuntimeApiImpl<C, Block>
where
    Block: BlockT,
    C: CallApiAt<Block> + 'static + Send + Sync,
    C::StateBackend: sp_api::StateBackend<sp_api::HashFor<Block>>,
    // B: sc_client_api::backend::Backend<Block> + 'static + Send + Sync,
    // B::State: sp_api::StateBackend<sp_api::HashFor<Block>>,
{
}

impl<C, Block> MockRuntimeApiImpl<C, Block>
where
    Block: BlockT,
    C: CallApiAt<Block> + 'static + Send + Sync,
    C::StateBackend: sp_api::StateBackend<sp_api::HashFor<Block>>,
    // B: sc_client_api::backend::Backend<Block> + 'static + Send + Sync,
    // B::State: sp_api::StateBackend<sp_api::HashFor<Block>>,
{
    fn commit_or_rollback(&self, commit: bool) {
        let proof = "\
					We only close a transaction when we opened one ourself.
					Other parts of the runtime that make use of transactions (state-machine)
					also balance their transactions. The runtime cannot close client initiated
					transactions. qed";
        // if *self.commit_on_success.borrow() {
        if commit {
            self.changes.borrow_mut().commit_transaction().expect(proof);
        } else {
            self.changes
                .borrow_mut()
                .rollback_transaction()
                .expect(proof);
        }
        // }
    }

    fn _call_api_at<T>(
        &self,
        method: &'static str,
        context: Option<ExecutionContext>,
        init_block: bool,
        _update_init_block: bool,
        at: &BlockId<Block>,
        args: Vec<u8>,
    ) -> Result<T, sp_api::ApiError>
    where
        T: Encode + Decode + PartialEq,
    {
        let execution_context = if context.is_some() {
            context.unwrap()
        } else {
            ExecutionContext::OffchainCall(None)
        };
        self.changes.borrow_mut().start_transaction();
        let initialize_block = if !init_block {
            InitializeBlock::Skip
        } else {
            InitializeBlock::Do(&self.initialized_block)
        };
        // let update_initialized_block = if update_init_block {
        //     || *self.initialized_block.borrow_mut() = Some(*at)
        // };
        let call_params: CallApiAtParams<
            '_,
            Block,
            MockRuntimeApiImpl<C, Block>,
            fn() -> Result<T, sp_api::ApiError>,
            <C as CallApiAt<Block>>::StateBackend,
            // <B as Backend<Block>>::State,
        > = CallApiAtParams {
            core_api: self,
            at,
            function: method,
            native_call: None,
            arguments: args,
            overlayed_changes: &self.changes,
            storage_transaction_cache: &self.storage,
            initialize_block,
            context: execution_context,
            recorder: &None,
        };
        debug!(target: "mock", "call {} begin...", method);
        let res = self.call.call_api_at(call_params);
        debug!(target: "mock", "call {} end, at:{}", method, at);
        if res.is_ok() {
            self.changes
                .borrow_mut()
                .commit_transaction()
                .expect("commit error!");
        } else {
            self.changes
                .borrow_mut()
                .rollback_transaction()
                .expect("rollback error!");
        }
        // if update_init_block {
        //     update_initialized_block();
        // }
        let res = res.and_then(|r| match r {
            sp_api::NativeOrEncoded::Native(n) => Ok(n),
            sp_api::NativeOrEncoded::Encoded(r) => <T as sp_api::Decode>::decode(&mut &r[..])
                .map_err(|err| sp_api::ApiError::FailedToDecodeReturnValue {
                    function: method,
                    error: err,
                }),
        });
        res
    }

    fn _call_api_at_native_code<T>(
        &self,
        method: &'static str,
        context: Option<ExecutionContext>,
        init_block: bool,
        _update_init_block: bool,
        at: &BlockId<Block>,
        args: Vec<u8>,
    ) -> Result<NativeOrEncoded<T>, sp_api::ApiError>
    where
        T: Encode + Decode + PartialEq,
    {
        let execution_context = if context.is_some() {
            context.unwrap()
        } else {
            ExecutionContext::OffchainCall(None)
        };
        self.changes.borrow_mut().start_transaction();
        let initialize_block = if !init_block {
            InitializeBlock::Skip
        } else {
            InitializeBlock::Do(&self.initialized_block)
        };
        // let update_initialized_block = if update_init_block {
        //     || *self.initialized_block.borrow_mut() = Some(*at)
        // };
        let call_params: CallApiAtParams<
            '_,
            Block,
            MockRuntimeApiImpl<C, Block>,
            fn() -> Result<T, sp_api::ApiError>,
            <C as CallApiAt<Block>>::StateBackend,
            // <B as Backend<Block>>::State,
        > = CallApiAtParams {
            core_api: self,
            at,
            function: method,
            native_call: None,
            arguments: args,
            overlayed_changes: &self.changes,
            storage_transaction_cache: &self.storage,
            initialize_block,
            context: execution_context,
            recorder: &None,
        };
        debug!(target: "mock", "call {} begin...", method);
        let res = self.call.call_api_at(call_params);
        debug!(target: "mock", "call {} end, at:{}", method, at);
        if res.is_ok() {
            self.changes
                .borrow_mut()
                .commit_transaction()
                .expect("commit error!");
        } else {
            self.changes
                .borrow_mut()
                .rollback_transaction()
                .expect("rollback error!");
        }
        // if update_init_block {
        //     update_initialized_block();
        // }
        res
    }
}

// todo: the Client already implements CallApiAt
// impl<C, Block> CallApiAt<Block> for MockRuntimeApi<C, Block>
//     where
//         Block: BlockT,
//         C: CallApiAt<Block, StateBackend=B::State> + 'static + Send + Sync,
//         B: sc_client_api::backend::Backend<Block> + 'static + Send + Sync,{
//     type StateBackend = B::State;
//
//     fn call_api_at<
//         'a,
//         R: Encode + Decode + PartialEq,
//         NC: FnOnce() -> Result<R, ApiError> + UnwindSafe,
//         C: Core<Block>,
//     >(&self, params: CallApiAtParams<'a, Block, C, NC, Self::StateBackend>) -> Result<NativeOrEncoded<R>, ApiError> {
//         unimplemented!()
//     }
//
//     fn runtime_version_at(&self, at: &BlockId<Block>) -> Result<RuntimeVersion, ApiError> {
//         unimplemented!()
//     }
// }

impl<C, Block> ApiExt<Block> for MockRuntimeApiImpl<C, Block>
where
    Block: BlockT,
    C: CallApiAt<Block> + 'static + Send + Sync,
    C::StateBackend: sp_api::StateBackend<sp_api::HashFor<Block>>,
    // B: sc_client_api::backend::Backend<Block> + 'static + Send + Sync,
    // B::State: sp_api::StateBackend<sp_api::HashFor<Block>>,
{
    type StateBackend = C::StateBackend;
    // type StateBackend = B::State;

    fn execute_in_transaction<F: FnOnce(&Self) -> TransactionOutcome<R>, R>(&self, call: F) -> R
    where
        Self: Sized,
    {
        debug!(target: "mock", "call execute_in_transaction");
        // unimplemented!()
        self.changes.borrow_mut().start_transaction();
        //TODO add commit_on_success flag to MockRuntimeAPi
        // *self.commit_on_success.borrow_mut() = false;
        let res = call(self);
        // *self.commit_on_success.borrow_mut() = true;
        self.commit_or_rollback(match res {
            sp_api::TransactionOutcome::Commit(_) => true,
            _ => false,
        });
        res.into_inner()
    }

    fn has_api<A: RuntimeApiInfo + ?Sized>(&self, at: &BlockId<Block>) -> Result<bool, ApiError>
    where
        Self: Sized,
    {
        // unimplemented!()
        debug!(target: "mock", "call has_api");
        // Ok(true)
        self.call
            .runtime_version_at(at)
            .map(|v| v.has_api_with(&A::ID, |v| v == A::VERSION))
    }

    fn has_api_with<A: RuntimeApiInfo + ?Sized, P: Fn(u32) -> bool>(
        &self,
        at: &BlockId<Block>,
        pred: P,
    ) -> Result<bool, ApiError>
    where
        Self: Sized,
    {
        // unimplemented!()
        debug!(target: "mock", "call has_api_with");
        // Ok(true)
        self.call
            .runtime_version_at(at)
            .map(|v| v.has_api_with(&A::ID, pred))
    }

    fn record_proof(&mut self) {
        debug!(target: "mock", "call record_proof");
        unimplemented!()
    }

    fn extract_proof(&mut self) -> Option<StorageProof> {
        debug!(target: "mock", "call extract_proof");
        unimplemented!()
    }

    fn proof_recorder(&self) -> Option<ProofRecorder<Block>> {
        unimplemented!()
    }

    // block import prepare_block_storage_changes() will invoke this method after execute_block()
    fn into_storage_changes(
        &self,
        backend: &Self::StateBackend,
        changes_trie_state: Option<&ChangesTrieState<HashFor<Block>, NumberFor<Block>>>,
        parent_hash: <Block as BlockT>::Hash,
    ) -> Result<StorageChanges<Self::StateBackend, Block>, String>
    where
        Self: Sized,
    {
        // debug!(target: "mock", "call into_storage_changes");
        self.initialized_block.borrow_mut().take();
        self.changes
            .replace(Default::default())
            .into_storage_changes(
                backend,
                changes_trie_state,
                parent_hash,
                self.storage.replace(Default::default()),
            )
    }
}

const ID: [u8; 8] = [223u8, 106u8, 203u8, 104u8, 153u8, 7u8, 96u8, 155u8];

impl<C, Block> sp_api::Core<Block> for MockRuntimeApiImpl<C, Block>
where
    Block: BlockT,
    C: CallApiAt<Block> + 'static + Send + Sync,
    C::StateBackend: sp_api::StateBackend<sp_api::HashFor<Block>>,
    // B: sc_client_api::backend::Backend<Block> + 'static + Send + Sync,
    // B::State: sp_api::StateBackend<sp_api::HashFor<Block>>,
{
    fn version(&self, at: &BlockId<Block>) -> Result<RuntimeVersion, sp_api::ApiError> {
        // VERSION
        let args = sp_api::Encode::encode(&());
        self._call_api_at::<RuntimeVersion>("Core_version", None, true, false, at, args)
    }
    fn execute_block(&self, at: &BlockId<Block>, block: Block) -> Result<(), sp_api::ApiError> {
        let args = sp_api::Encode::encode(&(&block));
        self._call_api_at::<()>("Core_execute_block", None, false, false, at, args)
    }
    fn initialize_block(
        &self,
        at: &BlockId<Block>,
        header: &<Block as BlockT>::Header,
    ) -> Result<(), sp_api::ApiError> {
        let version = self.call.runtime_version_at(at)?;
        let args = sp_api::Encode::encode(&(&header));
        let method = if version.apis.iter().any(|(s, v)| s == &ID && *v < 2u32) {
            "Core_initialise_block"
        } else {
            "Core_initialize_block"
        };
        let update_initialized_block = || *self.initialized_block.borrow_mut() = Some(*at);
        let res = self._call_api_at::<()>(method, None, false, true, at, args);
        update_initialized_block();
        res
    }
    fn Core_version_runtime_api_impl(
        &self,
        _: &BlockId<Block>,
        _: ExecutionContext,
        _: std::option::Option<()>,
        _: Vec<u8>,
    ) -> std::result::Result<NativeOrEncoded<RuntimeVersion>, sp_api::ApiError> {
        debug!(target: "mock", "call Core_version");
        unimplemented!()
    }
    fn Core_execute_block_runtime_api_impl(
        &self,
        at: &BlockId<Block>,
        context: ExecutionContext,
        params: std::option::Option<Block>,
        _: Vec<u8>,
    ) -> std::result::Result<NativeOrEncoded<()>, sp_api::ApiError> {
        //todo: could we just use params_encode which also Vec<u8> as args
        let args = sp_api::Encode::encode(&(&params.unwrap()));
        self._call_api_at_native_code::<()>(
            "Core_execute_block",
            Some(context),
            false,
            false,
            at,
            args,
        )
    }
    fn Core_initialize_block_runtime_api_impl(
        &self,
        _: &BlockId<Block>,
        _: ExecutionContext,
        _: std::option::Option<&<Block as BlockT>::Header>,
        _: Vec<u8>,
    ) -> std::result::Result<NativeOrEncoded<()>, sp_api::ApiError> {
        debug!(target: "mock", "call Core_initialize_block");
        unimplemented!()
    }
}
impl<C, Block> sp_api::Metadata<Block> for MockRuntimeApiImpl<C, Block>
where
    Block: BlockT,
    C: CallApiAt<Block> + 'static + Send + Sync, // + sp_api::Core<Block>,
    C::StateBackend: sp_api::StateBackend<sp_api::HashFor<Block>>,
    // B: sc_client_api::backend::Backend<Block> + 'static + Send + Sync,
    // B::State: sp_api::StateBackend<sp_api::HashFor<Block>>,
{
    fn metadata(&self, at: &BlockId<Block>) -> Result<sp_core::OpaqueMetadata, sp_api::ApiError> {
        let args = sp_api::Encode::encode(&());
        self._call_api_at::<sp_core::OpaqueMetadata>(
            "Metadata_metadata",
            None,
            true,
            false,
            at,
            args,
        )
    }
    fn Metadata_metadata_runtime_api_impl(
        &self,
        at: &BlockId<Block>,
        context: ExecutionContext,
        _: std::option::Option<()>,
        _: Vec<u8>,
    ) -> std::result::Result<NativeOrEncoded<OpaqueMetadata>, sp_api::ApiError> {
        let args = sp_api::Encode::encode(&());
        self._call_api_at_native_code::<sp_core::OpaqueMetadata>(
            "Metadata_metadata",
            Some(context),
            true,
            false,
            at,
            args,
        )
    }
}

impl<C, Block> sp_block_builder::BlockBuilder<Block> for MockRuntimeApiImpl<C, Block>
where
    Block: BlockT,
    C: CallApiAt<Block> + 'static + Send + Sync,
    C::StateBackend: sp_api::StateBackend<sp_api::HashFor<Block>>,
    // B: sc_client_api::backend::Backend<Block> + 'static + Send + Sync,
    // B::State: sp_api::StateBackend<sp_api::HashFor<Block>>,
{
    fn apply_extrinsic(
        &self,
        _at: &BlockId<Block>,
        _extrinsic: <Block as BlockT>::Extrinsic,
    ) -> Result<ApplyExtrinsicResult, sp_api::ApiError> {
        debug!(target: "mock", "call apply_extrinsic");
        unimplemented!()
    }
    fn finalize_block(
        &self,
        _at: &BlockId<Block>,
    ) -> Result<<Block as BlockT>::Header, sp_api::ApiError> {
        debug!(target: "mock", "call finalize_block");
        unimplemented!()
    }
    fn inherent_extrinsics(
        &self,
        _at: &BlockId<Block>,
        _data: InherentData,
    ) -> Result<Vec<<Block as BlockT>::Extrinsic>, sp_api::ApiError> {
        // data.create_extrinsics()
        debug!(target: "mock", "call inherent_extrinsics");
        unimplemented!()
    }
    fn check_inherents(
        &self,
        at: &BlockId<Block>,
        block: Block,
        data: InherentData,
    ) -> Result<CheckInherentsResult, sp_api::ApiError> {
        let args = sp_api::Encode::encode(&(&block, &data));
        self._call_api_at::<CheckInherentsResult>(
            "BlockBuilder_check_inherents",
            None,
            false,
            false,
            at,
            args,
        )
    }
    // fn random_seed(
    // 	&self,
    // 	_at: &BlockId<Block>,
    // ) -> Result<<Block as BlockT>::Hash, sp_api::ApiError> {
    // 	// pallet_babe::RandomnessFromOneEpochAgo::<Runtime>::random_seed().0
    // 	debug!(target: "mock", "call random_seed");
    // 	unimplemented!()
    // }

    fn BlockBuilder_apply_extrinsic_runtime_api_impl(
        &self,
        _: &BlockId<Block>,
        _: ExecutionContext,
        _: std::option::Option<<Block as BlockT>::Extrinsic>,
        _: Vec<u8>,
    ) -> std::result::Result<
        NativeOrEncoded<
            std::result::Result<std::result::Result<(), DispatchError>, TransactionValidityError>,
        >,
        sp_api::ApiError,
    > {
        debug!(target: "mock", "call BlockBuilder_apply_extrinsic_runtime_api_impl");
        unimplemented!()
    }
    fn BlockBuilder_finalize_block_runtime_api_impl(
        &self,
        _: &BlockId<Block>,
        _: ExecutionContext,
        _: std::option::Option<()>,
        _: Vec<u8>,
    ) -> std::result::Result<NativeOrEncoded<<Block as BlockT>::Header>, sp_api::ApiError> {
        debug!(target: "mock", "call BlockBuilder_finalize_block_runtime_api_impl");
        unimplemented!()
    }
    fn BlockBuilder_inherent_extrinsics_runtime_api_impl(
        &self,
        _: &BlockId<Block>,
        _: ExecutionContext,
        _: std::option::Option<sp_consensus::InherentData>,
        _: Vec<u8>,
    ) -> std::result::Result<NativeOrEncoded<Vec<<Block as BlockT>::Extrinsic>>, sp_api::ApiError>
    {
        debug!(target: "mock", "call BlockBuilder_inherent_extrinsics_runtime_api_impl");
        unimplemented!()
    }
    fn BlockBuilder_check_inherents_runtime_api_impl(
        &self,
        _: &BlockId<Block>,
        _: ExecutionContext,
        _: std::option::Option<(Block, sp_consensus::InherentData)>,
        _: Vec<u8>,
    ) -> std::result::Result<NativeOrEncoded<CheckInherentsResult>, sp_api::ApiError> {
        debug!(target: "mock", "call BlockBuilder_check_inherents_runtime_api_impl");
        unimplemented!()
    }
    // fn BlockBuilder_random_seed_runtime_api_impl(
    // 	&self,
    // 	_: &BlockId<Block>,
    // 	_: ExecutionContext,
    // 	_: std::option::Option<()>,
    // 	_: Vec<u8>,
    // ) -> std::result::Result<NativeOrEncoded<<Block as BlockT>::Hash>, sp_api::ApiError> {
    // 	debug!(target: "mock", "call BlockBuilder_random_seed_runtime_api_impl");
    // 	unimplemented!()
    // }
}

impl<C, Block> fg_primitives::GrandpaApi<Block> for MockRuntimeApiImpl<C, Block>
where
    Block: BlockT,
    C: CallApiAt<Block> + 'static + Send + Sync,
    C::StateBackend: sp_api::StateBackend<sp_api::HashFor<Block>>,
    // B: sc_client_api::backend::Backend<Block> + 'static + Send + Sync,
    // B::State: sp_api::StateBackend<sp_api::HashFor<Block>>,
{
    fn grandpa_authorities(
        &self,
        _: &BlockId<Block>,
    ) -> Result<GrandpaAuthorityList, sp_api::ApiError> {
        // Grandpa::grandpa_authorities()
        debug!(target: "mock", "call grandpa_authorities");
        unimplemented!()
    }
    fn submit_report_equivocation_unsigned_extrinsic(
        &self,
        _: &BlockId<Block>,
        _equivocation_proof: fg_primitives::EquivocationProof<
            <Block as BlockT>::Hash,
            NumberFor<Block>,
        >,
        _key_owner_proof: fg_primitives::OpaqueKeyOwnershipProof,
    ) -> Result<Option<()>, sp_api::ApiError> {
        debug!(target: "mock", "call submit_report_equivocation_unsigned_extrinsic");
        unimplemented!()
    }
    fn generate_key_ownership_proof(
        &self,
        _: &BlockId<Block>,
        _set_id: fg_primitives::SetId,
        _authority_id: GrandpaId,
    ) -> Result<Option<fg_primitives::OpaqueKeyOwnershipProof>, sp_api::ApiError> {
        debug!(target: "mock", "call generate_key_ownership_proof");
        unimplemented!()
    }
    fn GrandpaApi_grandpa_authorities_runtime_api_impl(
        &self,
        _: &BlockId<Block>,
        _: ExecutionContext,
        _: std::option::Option<()>,
        _: Vec<u8>,
    ) -> std::result::Result<
        NativeOrEncoded<Vec<(fg_primitives::AuthorityId, u64)>>,
        sp_api::ApiError,
    > {
        debug!(target: "mock", "call GrandpaApi_grandpa_authorities_runtime_api_impl");
        todo!()
    }
    fn GrandpaApi_submit_report_equivocation_unsigned_extrinsic_runtime_api_impl(
        &self,
        _: &BlockId<Block>,
        _: ExecutionContext,
        _: std::option::Option<(
            fg_primitives::EquivocationProof<
                <Block as BlockT>::Hash,
                <<Block as BlockT>::Header as HeaderT>::Number,
            >,
            // pallet_grandpa::sp_finality_grandpa::OpaqueKeyOwnershipProof)>, _: Vec<u8>)
            fg_primitives::OpaqueKeyOwnershipProof,
        )>,
        _: Vec<u8>,
    ) -> std::result::Result<NativeOrEncoded<std::option::Option<()>>, sp_api::ApiError> {
        debug!(target: "mock", "call GrandpaApi_submit_report_equivocation_unsigned_extrinsic_runtime_api_impl");
        todo!()
    }
    fn GrandpaApi_generate_key_ownership_proof_runtime_api_impl(
        &self,
        _: &BlockId<Block>,
        _: ExecutionContext,
        _: std::option::Option<(u64, fg_primitives::AuthorityId)>,
        _: Vec<u8>,
    ) -> std::result::Result<
        NativeOrEncoded<std::option::Option<fg_primitives::OpaqueKeyOwnershipProof>>,
        sp_api::ApiError,
    > {
        debug!(target: "mock", "call GrandpaApi_generate_key_ownership_proof_runtime_api_impl");
        todo!()
    }
}

// impl<C, Block> sp_authority_discovery::AuthorityDiscoveryApi<Block> for MockRuntimeAPi<C, Block>
// where
//     Block: BlockT,
//     C: CallApiAt<Block, StateBackend=B::State> + 'static + Send + Sync,
//     // C::StateBackend: sp_api::StateBackend<sp_api::HashFor<Block>>,
// {
//     fn authorities(
//         &self,
//         _: &BlockId<Block>,
//     ) -> Result<Vec<sp_authority_discovery::AuthorityId>, sp_api::ApiError> {
//         // AuthorityDiscovery::authorities()
//         debug!(target: "mock", "call authorities");
//         unimplemented!()
//     }
//     fn AuthorityDiscoveryApi_authorities_runtime_api_impl(
//         &self,
//         _at: &BlockId<Block>,
//         _contxt: ExecutionContext,
//         _: std::option::Option<()>,
//         _: Vec<u8>,
//     ) -> std::result::Result<
//         NativeOrEncoded<Vec<sp_authority_discovery::AuthorityId>>,
//         sp_api::ApiError,
//     > {
//         debug!(target: "mock", "call AuthorityDiscoveryApi_authorities_runtime_api_impl");
//         todo!()
//     }
// }

impl<C, Block> sp_consensus_babe::BabeApi<Block> for MockRuntimeApiImpl<C, Block>
where
    Block: BlockT,
    C: CallApiAt<Block> + 'static + Send + Sync,
    C::StateBackend: sp_api::StateBackend<sp_api::HashFor<Block>>,
    // B: sc_client_api::backend::Backend<Block> + 'static + Send + Sync,
    // B::State: sp_api::StateBackend<sp_api::HashFor<Block>>,
{
    fn configuration(
        &self,
        at: &BlockId<Block>,
    ) -> Result<BabeGenesisConfiguration, sp_api::ApiError> {
        let args = sp_api::Encode::encode(&());
        self._call_api_at::<BabeGenesisConfiguration>(
            "BabeApi_configuration",
            None,
            false,
            false,
            at,
            args,
        )
    }
    fn current_epoch_start(&self, _: &BlockId<Block>) -> Result<Slot, sp_api::ApiError> {
        debug!(target: "mock", "call BabeApi current_epoch_start");
        unimplemented!()
    }
    fn current_epoch(&self, _: &BlockId<Block>) -> Result<Epoch, sp_api::ApiError> {
        debug!(target: "mock", "call BabeApi current_epoch");
        unimplemented!()
    }
    fn next_epoch(&self, _: &BlockId<Block>) -> Result<Epoch, sp_api::ApiError> {
        debug!(target: "mock", "call BabeApi next_epoch");
        unimplemented!()
    }
    fn generate_key_ownership_proof(
        &self,
        _: &BlockId<Block>,
        _slot: Slot,
        _authority_id: sp_consensus_babe::AuthorityId,
    ) -> Result<Option<sp_consensus_babe::OpaqueKeyOwnershipProof>, sp_api::ApiError> {
        debug!(target: "mock", "call BabeApi generate_key_ownership_proof");
        unimplemented!()
    }
    fn submit_report_equivocation_unsigned_extrinsic(
        &self,
        _: &BlockId<Block>,
        _equivocation_proof: sp_consensus_babe::EquivocationProof<<Block as BlockT>::Header>,
        _key_owner_proof: sp_consensus_babe::OpaqueKeyOwnershipProof,
    ) -> Result<Option<()>, sp_api::ApiError> {
        debug!(target: "mock", "call BabeApi submit_report_equivocation_unsigned_extrinsic");
        unimplemented!()
    }

    fn BabeApi_configuration_runtime_api_impl(
        &self,
        at: &BlockId<Block>,
        context: ExecutionContext,
        _: std::option::Option<()>,
        _: Vec<u8>,
    ) -> std::result::Result<NativeOrEncoded<BabeGenesisConfiguration>, sp_api::ApiError> {
        let args = sp_api::Encode::encode(&());
        self._call_api_at_native_code::<BabeGenesisConfiguration>(
            "BabeApi_configuration",
            Some(context),
            false,
            false,
            at,
            args,
        )
    }
    fn BabeApi_current_epoch_start_runtime_api_impl(
        &self,
        _: &BlockId<Block>,
        _: ExecutionContext,
        _: std::option::Option<()>,
        _: Vec<u8>,
    ) -> std::result::Result<NativeOrEncoded<Slot>, sp_api::ApiError> {
        debug!(target: "mock", "call BabeApi_current_epoch_start_runtime_api_impl");
        todo!()
    }
    fn BabeApi_current_epoch_runtime_api_impl(
        &self,
        _: &BlockId<Block>,
        _: ExecutionContext,
        _: std::option::Option<()>,
        _: Vec<u8>,
    ) -> std::result::Result<NativeOrEncoded<Epoch>, sp_api::ApiError> {
        debug!(target: "mock", "call BabeApi_current_epoch_runtime_api_impl");
        todo!()
    }
    fn BabeApi_next_epoch_runtime_api_impl(
        &self,
        _: &BlockId<Block>,
        _: ExecutionContext,
        _: std::option::Option<()>,
        _: Vec<u8>,
    ) -> std::result::Result<NativeOrEncoded<Epoch>, sp_api::ApiError> {
        debug!(target: "mock", "call BabeApi_next_epoch_runtime_api_impl");
        todo!()
    }
    fn BabeApi_generate_key_ownership_proof_runtime_api_impl(
        &self,
        _: &BlockId<Block>,
        _: ExecutionContext,
        _: std::option::Option<(Slot, sp_consensus_babe::AuthorityId)>,
        _: Vec<u8>,
    ) -> std::result::Result<
        NativeOrEncoded<std::option::Option<sp_consensus_babe::OpaqueKeyOwnershipProof>>,
        sp_api::ApiError,
    > {
        debug!(target: "mock", "call BabeApi_generate_key_ownership_proof_runtime_api_impl");
        todo!()
    }
    fn BabeApi_submit_report_equivocation_unsigned_extrinsic_runtime_api_impl(
        &self,
        _: &BlockId<Block>,
        _: ExecutionContext,
        _: std::option::Option<(
            sp_consensus_babe::EquivocationProof<<Block as BlockT>::Header>,
            sp_consensus_babe::OpaqueKeyOwnershipProof,
        )>,
        _: Vec<u8>,
    ) -> std::result::Result<NativeOrEncoded<std::option::Option<()>>, sp_api::ApiError> {
        debug!(target: "mock", "call BabeApi_submit_report_equivocation_unsigned_extrinsic_runtime_api_impl");
        todo!()
    }
}
impl<C, Block> sp_session::SessionKeys<Block> for MockRuntimeApiImpl<C, Block>
where
    Block: BlockT,
    C: CallApiAt<Block> + 'static + Send + Sync,
    C::StateBackend: sp_api::StateBackend<sp_api::HashFor<Block>>,
    // B: sc_client_api::backend::Backend<Block> + 'static + Send + Sync,
    // B::State: sp_api::StateBackend<sp_api::HashFor<Block>>,
{
    fn generate_session_keys(
        &self,
        at: &BlockId<Block>,
        seed: Option<Vec<u8>>,
    ) -> Result<Vec<u8>, sp_api::ApiError> {
        let args = sp_api::Encode::encode(&(&seed));
        self._call_api_at::<Vec<u8>>(
            "SessionKeys_generate_session_keys",
            None,
            false,
            false,
            at,
            args,
        )
    }
    fn decode_session_keys(
        &self,
        _: &BlockId<Block>,
        _encoded: Vec<u8>,
    ) -> Result<Option<Vec<(Vec<u8>, KeyTypeId)>>, sp_api::ApiError> {
        debug!(target: "mock", "call decode_session_keys");
        unimplemented!()
    }
    fn SessionKeys_generate_session_keys_runtime_api_impl(
        &self,
        _: &BlockId<Block>,
        _: ExecutionContext,
        _: std::option::Option<std::option::Option<Vec<u8>>>,
        _: Vec<u8>,
    ) -> std::result::Result<NativeOrEncoded<Vec<u8>>, sp_api::ApiError> {
        debug!(target: "mock", "call SessionKeys_generate_session_keys_runtime_api_impl");
        todo!()
    }
    fn SessionKeys_decode_session_keys_runtime_api_impl(
        &self,
        _: &BlockId<Block>,
        _: ExecutionContext,
        _: std::option::Option<Vec<u8>>,
        _: Vec<u8>,
    ) -> std::result::Result<
        NativeOrEncoded<std::option::Option<Vec<(Vec<u8>, KeyTypeId)>>>,
        sp_api::ApiError,
    > {
        debug!(target: "mock", "call SessionKeys_decode_session_keys_runtime_api_impl");
        todo!()
    }
}
