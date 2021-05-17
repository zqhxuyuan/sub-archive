use sc_cli::{ChainSpec, RuntimeVersion, SubstrateCli};

use crate::cli::Cli;
use crate::{chain_spec, service};
use ac_common::config::ArchiveCli;

impl SubstrateCli for Cli {
	fn impl_name() -> String {
		"Archive Dev Node".into()
	}

	fn impl_version() -> String {
		env!("SUBSTRATE_CLI_IMPL_VERSION").into()
	}

	fn description() -> String {
		env!("CARGO_PKG_DESCRIPTION").into()
	}

	fn author() -> String {
		env!("CARGO_PKG_AUTHORS").into()
	}

	fn support_url() -> String {
		"https://github.com/patractlabs/patracts-archive".into()
	}

	fn copyright_start_year() -> i32 {
		2021
	}

	fn load_spec(&self, id: &str) -> Result<Box<dyn ChainSpec>, String> {
		Ok(match id {
			"dev" => Box::new(chain_spec::development_config()?),
			path => Box::new(chain_spec::ChainSpec::from_json_file(
				std::path::PathBuf::from(path),
			)?),
		})
	}

	fn native_runtime_version(_: &Box<dyn ChainSpec>) -> &'static RuntimeVersion {
		&archive_runtime::VERSION
	}
}

/// Parse and run command line arguments
pub fn run() -> sc_cli::Result<()> {
	let cli = Cli::from_args();
	let archive_config = ArchiveCli::init()?;

	let command = &cli.run;
	let runner = cli.create_runner(command)?;
	runner
		.run_node_until_exit(|config| async move { service::new_full(config, archive_config) })
		.map_err(sc_cli::Error::Service)
}
