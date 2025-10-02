//! OrderBook implementation for managing multiple price levels and order matching.

pub mod book;
pub mod error;
/// Multi-book management with centralized trade event routing.
pub mod manager;
pub mod matching;

mod cache;
/// Contains the core logic for modifying the order book state, such as adding, canceling, or updating orders.
pub mod modifications;
pub mod operations;
mod pool;
mod private;
pub mod snapshot;
mod tests;
/// Trade-related types including TradeResult and TradeListener for monitoring order executions.
pub mod trade;

pub use book::OrderBook;
pub use error::OrderBookError;
pub use snapshot::{
    ORDERBOOK_SNAPSHOT_FORMAT_VERSION, OrderBookSnapshot, OrderBookSnapshotPackage,
};
