// #![feature(try_trait)]
//! Substrate service. Starts a thread that spins up the network, client, and extrinsic pool.
//! Manages communication between them.

#![recursion_limit = "1024"]

use std::pin::Pin;
// pub use sp_transaction_pool::{error::IntoPoolError, InPoolTransaction, TransactionPool};
use std::task::Poll;
use std::time::Duration;
pub use std::{ops::Deref, result::Result, sync::Arc};

use futures::{stream, FutureExt, Stream, StreamExt};
pub use sc_chain_spec::{
    ChainSpec, ChainType, Extension as ChainSpecExtension, GenericChainSpec, NoExtension,
    Properties, RuntimeGenesis,
};
use sc_client_api::{blockchain::HeaderBackend, BlockchainEvents};
pub use sc_executor::NativeExecutionDispatch;
pub use sc_network::config::{OnDemand, TransactionImport, TransactionImportFuture};
use sc_network::{network_state::NetworkState, NetworkStatus};
pub use sc_service::config::{
    BasePath, Configuration, DatabaseConfig, KeepBlocks, PruningMode, Role, RpcMethods,
    TaskExecutor, TaskType, TransactionStorageMode,
};
pub use sc_service::error::Error;
use sc_service::SpawnTaskHandle;
use sc_service::TaskManager;
pub use sp_consensus::import_queue::ImportQueue;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
use sp_utils::{
    mpsc::{tracing_unbounded, TracingUnboundedReceiver},
    status_sinks,
};

pub use self::builder::{
    build_network, new_client, new_db_backend, new_full_client, new_full_parts, BuildNetworkParams,
    NetworkStarter, TFullBackend, TFullCallExecutor, TFullClient,
};
pub use self::client::{ClientConfig, LocalCallExecutor};

mod builder;
pub mod client;
mod notifies;

/// Sinks to propagate network status updates.
/// For each element, every time the `Interval` fires we push an element on the sender.
#[derive(Clone)]
pub struct NetworkStatusSinks<Block: BlockT> {
    status: Arc<status_sinks::StatusSinks<NetworkStatus<Block>>>,
    state: Arc<status_sinks::StatusSinks<NetworkState>>,
}

impl<Block: BlockT> NetworkStatusSinks<Block> {
    fn new() -> Self {
        Self {
            status: Arc::new(status_sinks::StatusSinks::new()),
            state: Arc::new(status_sinks::StatusSinks::new()),
        }
    }

    /// Returns a receiver that periodically yields a [`NetworkStatus`].
    pub fn status_stream(
        &self,
        interval: Duration,
    ) -> TracingUnboundedReceiver<NetworkStatus<Block>> {
        let (sink, stream) = tracing_unbounded("mpsc_network_status");
        self.status.push(interval, sink);
        stream
    }

    /// Returns a receiver that periodically yields a [`NetworkState`].
    pub fn state_stream(&self, interval: Duration) -> TracingUnboundedReceiver<NetworkState> {
        let (sink, stream) = tracing_unbounded("mpsc_network_state");
        self.state.push(interval, sink);
        stream
    }
}

/// An incomplete set of chain components, but enough to run the chain ops subcommands.
pub struct PartialComponents<Client, Backend, SelectChain, ImportQueue, Other> {
    /// A shared client instance.
    pub client: Arc<Client>,
    /// A shared backend instance.
    pub backend: Arc<Backend>,
    /// The chain task manager.
    pub task_manager: TaskManager,
    /// A keystore container instance..
    // pub keystore_container: KeystoreContainer,
    /// A chain selection algorithm instance.
    pub select_chain: SelectChain,
    /// An import queue.
    pub import_queue: ImportQueue,
    /// A shared transaction pool.
    // pub transaction_pool: Arc<TransactionPool>,
    /// A registry of all providers of `InherentData`.
    // pub inherent_data_providers: sp_inherents::InherentDataProviders,
    /// Everything else that needs to be passed into the main build function.
    pub other: Other,
}

/// Builds a never-ending future that continuously polls the network.
///
/// The `status_sink` contain a list of senders to send a periodic network status to.
async fn build_network_future<
    B: BlockT,
    C: BlockchainEvents<B> + HeaderBackend<B>,
    H: sc_network::ExHashT,
>(
    _role: Role,
    mut network: sc_network::NetworkWorker<B, H>,
    client: Arc<C>,
    status_sinks: NetworkStatusSinks<B>,
    // mut rpc_rx: TracingUnboundedReceiver<sc_rpc::system::Request<B>>,
    _should_have_peers: bool,
    announce_imported_blocks: bool,
) {
    let mut imported_blocks_stream = client.import_notification_stream().fuse();

    // Current best block at initialization, to report to the RPC layer.
    let _starting_block = client.info().best_number;

    // Stream of finalized blocks reported by the client.
    let mut finality_notification_stream = {
        let mut finality_notification_stream = client.finality_notification_stream().fuse();

        // We tweak the `Stream` in order to merge together multiple items if they happen to be
        // ready. This way, we only get the latest finalized block.
        stream::poll_fn(move |cx| {
            let mut last = None;
            while let Poll::Ready(Some(item)) =
                Pin::new(&mut finality_notification_stream).poll_next(cx)
            {
                last = Some(item);
            }
            if let Some(last) = last {
                Poll::Ready(Some(last))
            } else {
                Poll::Pending
            }
        })
        .fuse()
    };

    loop {
        futures::select! {
            // List of blocks that the client has imported.
            notification = imported_blocks_stream.next() => {
                let notification = match notification {
                    Some(n) => n,
                    // If this stream is shut down, that means the client has shut down, and the
                    // most appropriate thing to do for the network future is to shut down too.
                    None => return,
                };

                if announce_imported_blocks {
                    network.service().announce_block(notification.hash, None);
                }

                if notification.is_new_best {
                    network.service().new_best_block_imported(
                        notification.hash,
                        *notification.header.number(),
                    );
                }
            }

            // List of blocks that the client has finalized.
            notification = finality_notification_stream.select_next_some() => {
                network.on_block_finalized(notification.hash, notification.header);
            }

            // The network worker has done something. Nothing special to do, but could be
            // used in the future to perform actions in response of things that happened on
            // the network.
            _ = (&mut network).fuse() => {}

            // At a regular interval, we send high-level status as well as
            // detailed state information of the network on what are called
            // "status sinks".

            status_sink = status_sinks.status.next().fuse() => {
                status_sink.send(network.status());
            }

            state_sink = status_sinks.state.next().fuse() => {
                state_sink.send(network.network_state());
            }
        }
    }
}
