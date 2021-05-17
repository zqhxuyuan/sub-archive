use sc_executor::native_executor_instance;
pub use sc_executor::NativeExecutor;

// Declare an instance of the native executor named `Executor`. Not not has wasm part.
native_executor_instance!(
	pub Executor,
	archive_runtime::api::dispatch,
	archive_runtime::native_version,
);
