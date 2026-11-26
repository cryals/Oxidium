use chrono::NaiveDate;
use rust_decimal::Decimal;

use crate::error::DomainError;

pub const MAX_TITLE_LEN: usize = 256;
pub const MAX_SHORT_NAME_LEN: usize = 32;
pub const MAX_TEXT_LEN: usize = 16_384;
pub const MAX_BATCH_SIZE: usize = 200;

pub fn validate_non_empty(field: &str, value: &str, max_len: usize) -> Result<String, DomainError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(DomainError::validation(field, "must not be empty"));
    }
    if trimmed.chars().count() > max_len {
        return Err(DomainError::validation(
            field,
            format!("must not exceed {max_len} characters"),
        ));
    }
    Ok(trimmed.to_string())
}

pub fn validate_optional_text(
    field: &str,
    value: &str,
    max_len: usize,
) -> Result<String, DomainError> {
    let trimmed = value.trim();
    if trimmed.chars().count() > max_len {
        return Err(DomainError::validation(
            field,
            format!("must not exceed {max_len} characters"),
        ));
    }
    Ok(trimmed.to_string())
}

pub fn validate_hex_color(field: &str, color: &str) -> Result<String, DomainError> {
    let trimmed = color.trim();
    let bytes = trimmed.as_bytes();
    let valid = (bytes.len() == 7 || bytes.len() == 9)
        && bytes[0] == b'#'
        && bytes[1..].iter().all(|b| b.is_ascii_hexdigit());
    if !valid {
        return Err(DomainError::validation(
            field,
            "must be a valid hex color (#RRGGBB or #RRGGBBAA)",
        ));
    }
    Ok(trimmed.to_uppercase())
}

pub fn validate_weight(weight: Decimal) -> Result<Decimal, DomainError> {
    let min = Decimal::new(1, 2); // 0.01
    let max = Decimal::from(100);
    if weight < min || weight > max {
        return Err(DomainError::validation(
            "weight",
            "must be between 0.01 and 100.00",
        ));
    }
    Ok(weight)
}

pub fn validate_target_average(target: Decimal) -> Result<Decimal, DomainError> {
    let min = Decimal::from(1);
    let max = Decimal::from(5);
    if target < min || target > max {
        return Err(DomainError::validation(
            "target_average",
            "must be between 1.0 and 5.0",
        ));
    }
    Ok(target)
}

pub fn validate_date_range(start: NaiveDate, end: NaiveDate) -> Result<(), DomainError> {
    if start > end {
        return Err(DomainError::validation(
            "date_range",
            "start_date must be on or before end_date",
        ));
    }
    Ok(())
}

pub fn validate_batch_size(len: usize) -> Result<(), DomainError> {
    if len == 0 {
        return Err(DomainError::validation(
            "items",
            "batch must contain at least one item",
        ));
    }
    if len > MAX_BATCH_SIZE {
        return Err(DomainError::validation(
            "items",
            format!("batch size {len} exceeds maximum of {MAX_BATCH_SIZE}"),
        ));
    }
    Ok(())
}

pub fn validate_port(port: u16) -> Result<u16, DomainError> {
    if port < 1024 {
        return Err(DomainError::validation(
            "port",
            "must be an unprivileged port (>= 1024)",
        ));
    }
    Ok(port)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_colors_and_weights() {
        assert_eq!(validate_hex_color("color", "#445e91").unwrap(), "#445E91");
        assert!(validate_hex_color("color", "red").is_err());
        assert!(validate_hex_color("color", "#12345").is_err());

        assert!(validate_weight(Decimal::from(1)).is_ok());
        assert!(validate_weight(Decimal::ZERO).is_err());
        assert!(validate_weight(Decimal::from(-1)).is_err());
        assert!(validate_weight(Decimal::from(101)).is_err());
    }

    #[test]
    fn validates_date_ranges_and_batches() {
        let d1 = NaiveDate::from_ymd_opt(2026, 9, 1).unwrap();
        let d2 = NaiveDate::from_ymd_opt(2026, 12, 31).unwrap();
        assert!(validate_date_range(d1, d2).is_ok());
        assert!(validate_date_range(d1, d1).is_ok());
        assert!(validate_date_range(d2, d1).is_err());

        assert!(validate_batch_size(0).is_err());
        assert!(validate_batch_size(MAX_BATCH_SIZE).is_ok());
        assert!(validate_batch_size(MAX_BATCH_SIZE + 1).is_err());
    }
}
