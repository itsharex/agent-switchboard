//! Pure ownership checks shared by discovery and repair planning.

use sha2::Digest;

use crate::extensions::contracts::ManagedFileEntry;
use crate::extensions::skill::ContentEntry;
use crate::extensions::validate::ContentEntryKind;

/// Whether every currently present entry is an unchanged member of the last
/// deployed version. Missing members are allowed; modifications, additions,
/// type changes, and mode changes are not.
pub fn is_unmodified_subset(current: &[ContentEntry], last_files: &[ManagedFileEntry]) -> bool {
    current.iter().all(|entry| {
        last_files.iter().any(|file| {
            file.relative_path == entry.relative_path
                && file.mode == entry.mode
                && match entry.kind {
                    ContentEntryKind::Dir => file.digest.is_empty() && file.size == 0,
                    ContentEntryKind::File => {
                        file.size == entry.bytes.len() as u64
                            && file.digest
                                == sha2::Sha256::digest(&entry.bytes)
                                    .iter()
                                    .map(|byte| format!("{byte:02x}"))
                                    .collect::<String>()
                    }
                    ContentEntryKind::Link => false,
                }
        })
    })
}
