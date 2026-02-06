//! Polymarket taker fee calculation
//!
//! Implements the exact fee formula from Polymarket's 15-min crypto markets.
//! Formula: fee = C × feeRate × (p × (1 - p))^exponent
//! Where C = shares, p = price, feeRate = 0.25, exponent = 2

use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};

/// Polymarket taker fee parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolymarketFeeConfig {
    /// Fee rate (default 0.25)
    pub fee_rate: Decimal,
    /// Exponent for the price factor (default 2)
    pub exponent: u32,
}

impl Default for PolymarketFeeConfig {
    fn default() -> Self {
        Self {
            fee_rate: dec!(0.25),
            exponent: 2,
        }
    }
}

/// Calculate taker fee for a given number of shares at a given price.
///
/// Formula: fee = C × feeRate × (p × (1 - p))^exponent
/// - Result rounded down to 4 decimal places
/// - Returns 0 if fee < 0.0001 USDC
pub fn calculate_taker_fee(
    shares: Decimal,
    price: Decimal,
    config: &PolymarketFeeConfig,
) -> Decimal {
    if shares <= Decimal::ZERO || price <= Decimal::ZERO || price >= Decimal::ONE {
        return Decimal::ZERO;
    }

    let p_complement = Decimal::ONE - price;
    let base = price * p_complement; // p × (1 - p)

    // base^exponent (for exponent=2: base * base)
    let mut factor = Decimal::ONE;
    for _ in 0..config.exponent {
        factor *= base;
    }

    let raw_fee = shares * config.fee_rate * factor;

    // Round down to 4 decimal places
    let scale_factor = dec!(10000);
    let rounded = (raw_fee * scale_factor).floor() / scale_factor;

    if rounded < dec!(0.0001) {
        Decimal::ZERO
    } else {
        rounded
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_fee_at_50_50() {
        let config = PolymarketFeeConfig::default();
        // 100 shares at $0.50: fee = 100 * 0.25 * (0.50 * 0.50)^2 = 100 * 0.25 * 0.0625 = 1.5625
        let fee = calculate_taker_fee(dec!(100), dec!(0.50), &config);
        assert_eq!(fee, dec!(1.5625));
    }

    #[test]
    fn test_fee_at_extreme_low() {
        let config = PolymarketFeeConfig::default();
        // 100 shares at $0.05: fee = 100 * 0.25 * (0.05 * 0.95)^2 = 100 * 0.25 * 0.002256... ≈ 0.0564
        let fee = calculate_taker_fee(dec!(100), dec!(0.05), &config);
        assert_eq!(fee, dec!(0.0564));
    }

    #[test]
    fn test_fee_at_extreme_high() {
        let config = PolymarketFeeConfig::default();
        // 100 shares at $0.90: fee = 100 * 0.25 * (0.90 * 0.10)^2 = 100 * 0.25 * 0.0081 = 0.2025
        let fee = calculate_taker_fee(dec!(100), dec!(0.90), &config);
        assert_eq!(fee, dec!(0.2025));
    }

    #[test]
    fn test_fee_symmetry() {
        let config = PolymarketFeeConfig::default();
        // Fee at 0.30 should equal fee at 0.70
        let fee_30 = calculate_taker_fee(dec!(100), dec!(0.30), &config);
        let fee_70 = calculate_taker_fee(dec!(100), dec!(0.70), &config);
        assert_eq!(fee_30, fee_70);
    }

    #[test]
    fn test_fee_zero_shares() {
        let config = PolymarketFeeConfig::default();
        assert_eq!(
            calculate_taker_fee(Decimal::ZERO, dec!(0.50), &config),
            Decimal::ZERO
        );
    }

    #[test]
    fn test_fee_edge_prices() {
        let config = PolymarketFeeConfig::default();
        // At p=0 or p=1, fee should be 0
        assert_eq!(
            calculate_taker_fee(dec!(100), Decimal::ZERO, &config),
            Decimal::ZERO
        );
        assert_eq!(
            calculate_taker_fee(dec!(100), Decimal::ONE, &config),
            Decimal::ZERO
        );
    }

    #[test]
    fn test_fee_single_share() {
        let config = PolymarketFeeConfig::default();
        // 1 share at $0.50: fee = 1 * 0.25 * 0.0625 = 0.015625 → rounds to 0.0156
        let fee = calculate_taker_fee(Decimal::ONE, dec!(0.50), &config);
        assert_eq!(fee, dec!(0.0156));
    }
}
