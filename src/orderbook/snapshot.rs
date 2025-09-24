//! Order book snapshot for market data

use pricelevel::PriceLevelSnapshot;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::trace;

use super::error::OrderBookError;

/// A snapshot of the order book state at a specific point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookSnapshot {
    /// The symbol or identifier for this order book
    pub symbol: String,

    /// Timestamp when the snapshot was created (milliseconds since epoch)
    pub timestamp: u64,

    /// Snapshot of bid price levels
    pub bids: Vec<PriceLevelSnapshot>,

    /// Snapshot of ask price levels
    pub asks: Vec<PriceLevelSnapshot>,
}

impl OrderBookSnapshot {
    /// Recomputes aggregate values for all included price levels.
    pub fn refresh_aggregates(&mut self) {
        for level in &mut self.bids {
            level.refresh_aggregates();
        }

        for level in &mut self.asks {
            level.refresh_aggregates();
        }
    }

    /// Get the best bid price and quantity
    pub fn best_bid(&self) -> Option<(u64, u64)> {
        let bids = self
            .bids
            .iter()
            .map(|level| (level.price, level.visible_quantity))
            .max_by_key(|&(price, _)| price);
        trace!("best_bid: {:?}", bids);
        bids
    }

    /// Get the best ask price and quantity
    pub fn best_ask(&self) -> Option<(u64, u64)> {
        let ask = self
            .asks
            .iter()
            .map(|level| (level.price, level.visible_quantity))
            .min_by_key(|&(price, _)| price);
        trace!("best_ask: {:?}", ask);
        ask
    }

    /// Get the mid price (average of best bid and best ask)
    pub fn mid_price(&self) -> Option<f64> {
        let mid_price = match (self.best_bid(), self.best_ask()) {
            (Some((bid_price, _)), Some((ask_price, _))) => {
                Some((bid_price as f64 + ask_price as f64) / 2.0)
            }
            _ => None,
        };
        trace!("mid_price: {:?}", mid_price);
        mid_price
    }

    /// Get the spread (best ask - best bid)
    pub fn spread(&self) -> Option<u64> {
        let spread = match (self.best_bid(), self.best_ask()) {
            (Some((bid_price, _)), Some((ask_price, _))) => {
                Some(ask_price.saturating_sub(bid_price))
            }
            _ => None,
        };
        trace!("spread: {:?}", spread);
        spread
    }

    /// Calculate the total volume on the bid side
    pub fn total_bid_volume(&self) -> u64 {
        let volume = self.bids.iter().map(|level| level.total_quantity()).sum();
        trace!("total_bid_volume: {:?}", volume);
        volume
    }

    /// Calculate the total volume on the ask side
    pub fn total_ask_volume(&self) -> u64 {
        let volume = self.asks.iter().map(|level| level.total_quantity()).sum();
        trace!("total_ask_volume: {:?}", volume);
        volume
    }

    /// Calculate the total value on the bid side (price * quantity)
    pub fn total_bid_value(&self) -> u64 {
        let value = self
            .bids
            .iter()
            .map(|level| level.price * level.total_quantity())
            .sum();
        trace!("total_bid_value: {:?}", value);
        value
    }

    /// Calculate the total value on the ask side (price * quantity)
    pub fn total_ask_value(&self) -> u64 {
        let value = self
            .asks
            .iter()
            .map(|level| level.price * level.total_quantity())
            .sum();
        trace!("total_ask_value: {:?}", value);
        value
    }
}

/// Format version used for checksum-enabled order book snapshots.
pub const ORDERBOOK_SNAPSHOT_FORMAT_VERSION: u32 = 1;

/// Wrapper that provides checksum validation for `OrderBookSnapshot` instances.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookSnapshotPackage {
    /// Version of the snapshot schema for forward compatibility.
    pub version: u32,
    /// Snapshot payload.
    pub snapshot: OrderBookSnapshot,
    /// Hex-encoded checksum of the serialized snapshot.
    pub checksum: String,
}

impl OrderBookSnapshotPackage {
    /// Creates a new snapshot package computing the checksum of the snapshot contents.
    pub fn new(mut snapshot: OrderBookSnapshot) -> Result<Self, OrderBookError> {
        snapshot.refresh_aggregates();

        let checksum = Self::compute_checksum(&snapshot)?;

        Ok(Self {
            version: ORDERBOOK_SNAPSHOT_FORMAT_VERSION,
            snapshot,
            checksum,
        })
    }

    /// Serializes the package to JSON.
    pub fn to_json(&self) -> Result<String, OrderBookError> {
        serde_json::to_string(self).map_err(|error| OrderBookError::SerializationError {
            message: error.to_string(),
        })
    }

    /// Deserializes the package from JSON.
    pub fn from_json(data: &str) -> Result<Self, OrderBookError> {
        serde_json::from_str(data).map_err(|error| OrderBookError::DeserializationError {
            message: error.to_string(),
        })
    }

    /// Validates the checksum and version.
    pub fn validate(&self) -> Result<(), OrderBookError> {
        if self.version != ORDERBOOK_SNAPSHOT_FORMAT_VERSION {
            return Err(OrderBookError::InvalidOperation {
                message: format!(
                    "Unsupported snapshot version: {} (expected {})",
                    self.version, ORDERBOOK_SNAPSHOT_FORMAT_VERSION
                ),
            });
        }

        let computed = Self::compute_checksum(&self.snapshot)?;
        if computed != self.checksum {
            return Err(OrderBookError::ChecksumMismatch {
                expected: self.checksum.clone(),
                actual: computed,
            });
        }

        Ok(())
    }

    /// Consumes the package and returns the validated snapshot.
    pub fn into_snapshot(self) -> Result<OrderBookSnapshot, OrderBookError> {
        self.validate()?;
        Ok(self.snapshot)
    }

    fn compute_checksum(snapshot: &OrderBookSnapshot) -> Result<String, OrderBookError> {
        let payload =
            serde_json::to_vec(snapshot).map_err(|error| OrderBookError::SerializationError {
                message: error.to_string(),
            })?;

        let mut hasher = Sha256::new();
        hasher.update(payload);

        let checksum_bytes = hasher.finalize();
        Ok(format!("{:x}", checksum_bytes))
    }
}
