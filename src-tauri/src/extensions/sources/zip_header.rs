use super::{SourceError, MAX_SOURCE_ENTRIES};

/// The ZIP reader allocates from the declared entry count and collapses
/// duplicate names. Gate its central directory before creating that reader.
pub(super) fn validate_zip_directory(bytes: &[u8]) -> Result<usize, SourceError> {
    let footer = (bytes.len().saturating_sub(65_557)..bytes.len().saturating_sub(21))
        .rev()
        .find(|&offset| {
            bytes.get(offset..offset + 4) == Some(b"PK\x05\x06")
                && read_u16(bytes, offset + 20).is_ok_and(|size| offset + 22 + size == bytes.len())
        })
        .ok_or_else(|| invalid("missing or truncated ZIP footer"))?;
    let count = read_u16(bytes, footer + 10)?;
    if read_u16(bytes, footer + 4)? != 0
        || read_u16(bytes, footer + 6)? != 0
        || read_u16(bytes, footer + 8)? != count
    {
        return Err(invalid("multipart archives are not supported"));
    }
    if count > MAX_SOURCE_ENTRIES {
        return Err(invalid(
            "entry count exceeds the source limit; ZIP64 archives are not supported",
        ));
    }
    let size = read_u32(bytes, footer + 12)?;
    let start = read_u32(bytes, footer + 16)?;
    if start.checked_add(size) != Some(footer) {
        return Err(invalid(
            "central directory is truncated or uses unsupported ZIP64 offsets",
        ));
    }
    let mut position = start;
    let mut names = std::collections::BTreeSet::new();
    for _ in 0..count {
        if position + 46 > footer || bytes.get(position..position + 4) != Some(b"PK\x01\x02") {
            return Err(invalid("central directory entry is truncated"));
        }
        let name_length = read_u16(bytes, position + 28)?;
        let extra_length = read_u16(bytes, position + 30)?;
        let comment_length = read_u16(bytes, position + 32)?;
        let end = position + 46 + name_length + extra_length + comment_length;
        if end > footer || read_u16(bytes, position + 34)? != 0 {
            return Err(invalid("invalid central directory entry bounds"));
        }
        let name = &bytes[position + 46..position + 46 + name_length];
        if !names.insert(name) {
            return Err(invalid("duplicate entry names are not supported"));
        }
        position = end;
    }
    if position != footer {
        return Err(invalid(
            "central directory count does not match its contents",
        ));
    }
    Ok(count)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<usize, SourceError> {
    let value: [u8; 2] = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| invalid("truncated ZIP header"))?
        .try_into()
        .expect("fixed slice length");
    Ok(u16::from_le_bytes(value) as usize)
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<usize, SourceError> {
    let value: [u8; 4] = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| invalid("truncated ZIP header"))?
        .try_into()
        .expect("fixed slice length");
    Ok(u32::from_le_bytes(value) as usize)
}

fn invalid(message: &str) -> SourceError {
    SourceError::Rejected(format!("ZIP validation failed: {message}"))
}
