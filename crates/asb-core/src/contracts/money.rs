//! Client-independent, exact decimal currency arithmetic.
/// Decimal money has a single fixed precision; floats/exponents are not accepted.
pub fn decimal_micros(value: &str) -> Result<u64, String> {
    let bad = || "金额必须是非负十进制数，最多六位小数".to_string();
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > 6
        || !fraction.bytes().all(|b| b.is_ascii_digit())
        || (value.contains('.') && fraction.is_empty())
    {
        return Err(bad());
    }
    let whole = whole.parse::<u64>().map_err(|_| bad())?;
    let fraction = format!("{fraction:0<6}")
        .parse::<u64>()
        .map_err(|_| bad())?;
    whole
        .checked_mul(1_000_000)
        .and_then(|n| n.checked_add(fraction))
        .ok_or_else(bad)
}

pub fn format_usd_micros(value: u64) -> String {
    format!("{}.{:06}", value / 1_000_000, value % 1_000_000)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn money_is_decimal_and_exact() {
        assert_eq!(decimal_micros("1.000001").unwrap(), 1000001);
        assert_eq!(decimal_micros("0.10").unwrap(), 100000);
        assert_eq!(format_usd_micros(100001), "0.100001");
        for invalid in ["NaN", "-1", "1e3", "1.0000001", "1.", ".5", " 1"] {
            assert!(decimal_micros(invalid).is_err());
        }
    }
}
