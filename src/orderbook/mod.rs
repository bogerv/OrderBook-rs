//! OrderBook implementation for managing multiple price levels and order matching.

pub mod book;
pub mod error;
pub mod matching;

mod cache;
/// Contains the core logic for modifying the order book state, such as adding, canceling, or updating orders.
pub mod modifications;
pub mod operations;
mod pool;
mod private;
pub mod snapshot;
mod tests;

pub use book::{OrderBook, TradeListener, TradeResult};
pub use error::OrderBookError;
pub use snapshot::{
    ORDERBOOK_SNAPSHOT_FORMAT_VERSION, OrderBookSnapshot, OrderBookSnapshotPackage,
};
