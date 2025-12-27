//! Futures contract utilities
//!
//! Handles automatic detection of front-month contracts for futures symbols.
//! Supports CME futures (NQ, ES, etc.) with standard quarterly expiration cycles.

use chrono::{Datelike, NaiveDate, Utc};

/// CME futures expiration months
/// H = March, M = June, U = September, Z = December
const QUARTERLY_MONTHS: [(char, u32); 4] = [
    ('H', 3),  // March
    ('M', 6),  // June
    ('U', 9),  // September
    ('Z', 12), // December
];

/// Get the front-month contract symbol for a given base symbol
///
/// # Arguments
/// * `base_symbol` - Base symbol like "NQ" or "ES"
///
/// # Returns
/// The current front-month contract symbol (e.g., "NQH26" for March 2026)
///
/// # Example
/// ```
/// let symbol = get_front_month_contract("NQ");
/// // Returns "NQH26" if current date is before March 2026 expiry
/// ```
pub fn get_front_month_contract(base_symbol: &str) -> String {
    let today = Utc::now().naive_utc().date();
    get_front_month_contract_for_date(base_symbol, today)
}

/// Get the front-month contract for a specific date
pub fn get_front_month_contract_for_date(base_symbol: &str, date: NaiveDate) -> String {
    let (month_code, year) = get_front_month_info(date);
    let year_short = year % 100;
    format!(
        "{}{}{:02}",
        base_symbol.to_uppercase(),
        month_code,
        year_short
    )
}

/// Get the front month code and year for a given date
fn get_front_month_info(date: NaiveDate) -> (char, i32) {
    let current_month = date.month();
    let current_year = date.year();
    let current_day = date.day();

    // CME futures typically expire on the 3rd Friday of the contract month
    // We'll roll to next contract ~1 week before expiry (around day 8-10)
    // For simplicity, we roll when we're past day 10 of the expiry month

    for &(code, exp_month) in &QUARTERLY_MONTHS {
        if current_month < exp_month {
            // This expiry is in the future
            return (code, current_year);
        } else if current_month == exp_month {
            // We're in the expiry month - check if we should roll
            if current_day <= 10 {
                // Still early in the month, use this contract
                return (code, current_year);
            }
            // Past rollover date, continue to find next contract
        }
        // Current month is past this expiry, continue
    }

    // All quarterly months for this year have passed, use first month of next year
    (QUARTERLY_MONTHS[0].0, current_year + 1)
}

/// Parse a Sierra Chart symbol to extract base symbol and contract info
///
/// # Examples
/// * "NQH26" -> Some(("NQ", 'H', 2026))
/// * "ESM25" -> Some(("ES", 'M', 2025))
/// * "NQ" -> None
pub fn parse_sierra_symbol(symbol: &str) -> Option<(&str, char, i32)> {
    if symbol.len() < 4 {
        return None;
    }

    // Find where the month code is (should be followed by 2 digits)
    let chars: Vec<char> = symbol.chars().collect();
    let len = chars.len();

    // Last 3 chars should be: month_code + 2-digit year
    if len < 4 {
        return None;
    }

    let month_code = chars[len - 3];
    let year_str: String = chars[len - 2..].iter().collect();

    // Validate month code
    if !QUARTERLY_MONTHS.iter().any(|(c, _)| *c == month_code) {
        return None;
    }

    // Parse year
    let year_short: i32 = year_str.parse().ok()?;
    let year = if year_short >= 50 {
        1900 + year_short
    } else {
        2000 + year_short
    };

    // Base symbol is everything before month code
    let base = &symbol[..len - 3];

    Some((base, month_code, year))
}

/// Convert a display symbol to Sierra Chart format (without exchange suffix)
///
/// # Examples
/// * "NQ" -> "NQH26" (auto-detect front month)
/// * "NQ=F" -> "NQH26" (yfinance format)
/// * "NQH26" -> "NQH26" (already Sierra format)
/// * "NQH26-CME" -> "NQH26" (strip exchange suffix)
pub fn to_sierra_symbol(symbol: &str) -> String {
    let symbol = symbol.trim().to_uppercase();

    // Strip exchange suffix if present (e.g., "-CME", "-NYMEX")
    let symbol = if let Some(idx) = symbol.find('-') {
        &symbol[..idx]
    } else {
        &symbol
    };

    // Check if already a Sierra symbol (has month code + year)
    if parse_sierra_symbol(symbol).is_some() {
        return symbol.to_string();
    }

    // Strip yfinance suffix if present
    let base = if symbol.ends_with("=F") {
        &symbol[..symbol.len() - 2]
    } else {
        symbol
    };

    // Get front month contract
    get_front_month_contract(base)
}

/// Convert a symbol to full Sierra Chart format with exchange suffix
///
/// # Examples
/// * "NQ" -> "NQH26-CME" (auto-detect front month + add exchange)
/// * "NQH26" -> "NQH26-CME" (add exchange suffix)
/// * "NQH26-CME" -> "NQH26-CME" (already has exchange)
pub fn to_sierra_symbol_with_exchange(symbol: &str, exchange: &str) -> String {
    let base_symbol = to_sierra_symbol(symbol);
    format!("{}-{}", base_symbol, exchange)
}

/// Convert a Sierra Chart symbol to a display symbol
///
/// For now, keeps the Sierra format for display consistency.
pub fn to_display_symbol(sierra_symbol: &str) -> String {
    sierra_symbol.to_string()
}

/// Sierra/CME base symbols that use the Rust server with Sierra Chart data
const SIERRA_BASE_SYMBOLS: &[&str] = &["NQ", "ES", "YM"];

/// Check if a symbol is a Sierra/CME symbol that should use Rust-native processing
///
/// Returns true for NQ, ES, YM and their contract variants (e.g., NQH26, ES=F)
pub fn is_sierra_symbol(symbol: &str) -> bool {
    let upper = symbol.to_uppercase();

    // Strip common suffixes
    let base = if upper.ends_with("=F") {
        &upper[..upper.len() - 2]
    } else if upper.contains('-') {
        // Strip exchange suffix like -CME
        upper.split('-').next().unwrap_or(&upper)
    } else {
        &upper
    };

    // Check if it starts with any Sierra base symbol
    SIERRA_BASE_SYMBOLS.iter().any(|s| base.starts_with(s))
}

/// Get all symbols that should be subscribed for a given base symbol
///
/// Returns symbols for multiple timeframe analysis if needed.
pub fn get_subscription_symbols(base_symbol: &str) -> Vec<String> {
    vec![to_sierra_symbol(base_symbol)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn test_front_month_january() {
        let date = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
        let symbol = get_front_month_contract_for_date("NQ", date);
        assert_eq!(symbol, "NQH26"); // March 2026
    }

    #[test]
    fn test_front_month_march_early() {
        let date = NaiveDate::from_ymd_opt(2026, 3, 5).unwrap();
        let symbol = get_front_month_contract_for_date("NQ", date);
        assert_eq!(symbol, "NQH26"); // Still March, early in month
    }

    #[test]
    fn test_front_month_march_late() {
        let date = NaiveDate::from_ymd_opt(2026, 3, 15).unwrap();
        let symbol = get_front_month_contract_for_date("NQ", date);
        assert_eq!(symbol, "NQM26"); // Rolled to June
    }

    #[test]
    fn test_front_month_december_late() {
        let date = NaiveDate::from_ymd_opt(2025, 12, 20).unwrap();
        let symbol = get_front_month_contract_for_date("ES", date);
        assert_eq!(symbol, "ESH26"); // March 2026
    }

    #[test]
    fn test_parse_sierra_symbol() {
        assert_eq!(parse_sierra_symbol("NQH26"), Some(("NQ", 'H', 2026)));
        assert_eq!(parse_sierra_symbol("ESM25"), Some(("ES", 'M', 2025)));
        assert_eq!(parse_sierra_symbol("NQ"), None);
        assert_eq!(parse_sierra_symbol("NQX26"), None); // Invalid month code
    }

    #[test]
    fn test_to_sierra_symbol() {
        // Already Sierra format
        assert_eq!(to_sierra_symbol("NQH26"), "NQH26");

        // yfinance format
        let result = to_sierra_symbol("NQ=F");
        assert!(result.starts_with("NQ"));
        assert!(result.len() == 5); // NQ + month + 2-digit year

        // Base symbol
        let result = to_sierra_symbol("ES");
        assert!(result.starts_with("ES"));
        assert!(result.len() == 5);
    }
}
