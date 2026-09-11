//! Pure mapping from external provider rows to import proposals.
//!
//! This module never touches the filesystem or SQLite: the caller reads
//! database rows and injects them. A custom-provider credential becomes the
//! draft API key for profile persistence; diagnostics and warnings carry only
//! field names. Official rows become a credential-free official route and
//! never expose or copy the source login material.

mod codex;
mod mapping;
mod row;
#[cfg(test)]
mod tests;
mod usage;

pub use mapping::map_row;
pub use row::{
    CcSwitchProposal, CcSwitchProviderDraft, CcSwitchRow, CcSwitchSkip, CodexCatalogSeed,
    CodexImportSeed,
};
