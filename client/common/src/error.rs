#[derive(Debug)]
pub enum ArchiveError {
	Client(sp_blockchain::Error),

	Migration(archive_postgres::SqlxError),

	Io(std::io::Error),

	Toml(toml::de::Error),
}
