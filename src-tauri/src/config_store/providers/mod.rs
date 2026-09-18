//! Provider storage: exactly one JSON file per provider, named by its stable
//! UUID under `providers/{client}/`. The directory owns the client
//! association; the file owns its sort position. No index file exists.

mod load;
mod mutations;


pub(crate) use load::load_provider_files;
