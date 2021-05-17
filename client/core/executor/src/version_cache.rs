//! A cache of runtime versions
//! Will only call the `runtime_version` function once per wasm blob

use sc_service::error::{Error, Result};
use std::{
	collections::hash_map::DefaultHasher,
	hash::{Hash, Hasher as _},
	sync::Arc,
};

use std::marker::PhantomData;

use arc_swap::ArcSwap;
use codec::Decode;
use hashbrown::HashMap;

use sc_client_api::{Backend, StateBackend};
use sc_executor::{WasmExecutionMethod, WasmExecutor};

use sp_api::BlockId;
use sp_core::traits::CallInWasmExt;
use sp_runtime::traits::Block as BlockT;
use sp_state_machine::BasicExternalities;
use sp_storage::well_known_keys;
use sp_version::RuntimeVersion;
use sp_wasm_interface::HostFunctions;
// use std::option::NoneError;

pub struct RuntimeVersionCache<B: BlockT, D> {
	//where D: Backend<B> {
	/// Hash of the WASM Blob -> RuntimeVersion
	versions: ArcSwap<HashMap<u64, RuntimeVersion>>,
	backend: Arc<D>,
	exec: WasmExecutor,
	_phantom: PhantomData<B>,
}

impl<B: BlockT, D> RuntimeVersionCache<B, D>
where
	D: Backend<B> + 'static,
{
	pub fn new(backend: Arc<D>) -> Self {
		let funs = sp_io::SubstrateHostFunctions::host_functions()
			.into_iter()
			.filter(|f| f.name().matches("wasm_tracing").count() == 0)
			.filter(|f| f.name().matches("ext_offchain").count() == 0)
			.filter(|f| f.name().matches("ext_storage").count() == 0)
			.filter(|f| f.name().matches("ext_default_child_storage").count() == 0)
			.filter(|f| f.name().matches("ext_logging").count() == 0)
			.collect::<Vec<_>>();

		let exec = WasmExecutor::new(WasmExecutionMethod::Interpreted, Some(128), funs, 1, None);
		Self {
			versions: ArcSwap::from_pointee(HashMap::new()),
			backend,
			exec,
			_phantom: Default::default(),
		}
	}

	pub fn get1(&self, hash: BlockId<B>) -> Result<Option<RuntimeVersion>> {
		let code = self.backend.state_at(hash)?;
		let code = code
			.storage(well_known_keys::CODE)
			.unwrap_or_else(|_| panic!("No storage found for {:?}", hash));

		let code_hash = make_hash(&code);
		if self.versions.load().contains_key(&code_hash) {
			Ok(self.versions.load().get(&code_hash).cloned())
		} else {
			log::debug!("Adding new runtime code hash to cache: {:#X?}", code_hash);
			let mut ext = BasicExternalities::default();
			ext.register_extension(CallInWasmExt::new(self.exec.clone()));
			let version: RuntimeVersion = ext.execute_with(|| {
				// let ver = sp_io::misc::runtime_version(&code.unwrap())?; //.ok_or(Error::Other("wasm runtime".to_string()))?;
				let ver = sp_io::misc::runtime_version(&code.unwrap())
					.ok_or(Error::Other("wasm runtime".to_string()))?; //.ok_or(Error::Other("wasm runtime".to_string()))?;
				decode_version(ver.as_slice())
			})?;
			log::debug!("Registered a new runtime version: {:?}", version);
			self.versions.rcu(|cache| {
				let mut cache = HashMap::clone(&cache);
				cache.insert(code_hash, version.clone());
				cache
			});
			Ok(Some(version))
		}
	}

	/// Get a version of the runtime for some Block Hash
	/// Prefer `find_versions` when trying to get the runtime versions for
	/// many consecutive blocks
	pub fn get(&self, hash: B::Hash) -> Result<Option<RuntimeVersion>> {
		// Getting code from the backend is the slowest part of this. Takes an average of 6ms
		// let code = self.backend.storage(hash, well_known_keys::CODE).ok_or(BackendError::StorageNotExist)?;
		self.get1(BlockId::hash(hash))
	}
}

fn decode_version(version: &[u8]) -> Result<sp_version::RuntimeVersion> {
	let v: RuntimeVersion = sp_api::OldRuntimeVersion::decode(&mut &version[..])
		.map_err(|_e| Error::Other("decode error".to_string()))?
		.into();
	let core_api_id = sp_core::hashing::blake2_64(b"Core");
	if v.has_api_with(&core_api_id, |v| v >= 3) {
		sp_api::RuntimeVersion::decode(&mut &version[..])
			.map_err(|_e| Error::Other("decode error".to_string())) //.map_err(Into::into)
	} else {
		Ok(v)
	}
}

// Make a hash out of a byte string using the default hasher.
fn make_hash<K: Hash + ?Sized>(val: &K) -> u64 {
	let mut state = DefaultHasher::new();
	val.hash(&mut state);
	state.finish()
}

// impl From<codec::Error> for Error {
//     fn from(_: codec::Error) -> Self {
//         Error::from("codec error")
//     }
// }
// impl From<NoneError> for Error {
//     fn from(_: NoneError) -> Self {
//         Error::from("none error")
//     }
// }
