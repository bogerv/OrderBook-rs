//! Core OrderBook implementation for managing price levels and orders

use super::cache::PriceLevelCache;
use super::error::OrderBookError;
use super::snapshot::{OrderBookSnapshot, OrderBookSnapshotPackage};
use crate::orderbook::trade::{TradeListener, TradeResult};
use crate::utils::current_time_millis;
use crossbeam_skiplist::SkipMap;
use dashmap::DashMap;
use pricelevel::{MatchResult, OrderId, OrderType, PriceLevel, Side, UuidGenerator};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::marker::PhantomData;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tracing::trace;
use uuid::Uuid;

/// The OrderBook manages a collection of price levels for both bid and ask sides.
/// It supports adding, cancelling, and matching orders with lock-free operations where possible.
pub struct OrderBook<T = ()> {
    /// The symbol or identifier for this order book
    pub(super) symbol: String,

    /// Bid side price levels (buy orders), stored in a concurrent ordered map (skip list)
    /// The map is keyed by price levels and stores Arc references to PriceLevel instances
    /// Using SkipMap provides O(log N) operations with automatic ordering, eliminating
    /// the need to sort prices during matching (optimization from O(N log N) to O(M log N))
    pub(super) bids: SkipMap<u64, Arc<PriceLevel>>,

    /// Ask side price levels (sell orders), stored in a concurrent ordered map (skip list)
    /// The map is keyed by price levels and stores Arc references to PriceLevel instances
    /// Using SkipMap provides O(log N) operations with automatic ordering, eliminating
    /// the need to sort prices during matching (optimization from O(N log N) to O(M log N))
    pub(super) asks: SkipMap<u64, Arc<PriceLevel>>,

    /// A concurrent map from order ID to (price, side) for fast lookups
    /// This avoids having to search through all price levels to find an order
    pub(super) order_locations: DashMap<OrderId, (u64, Side)>,

    /// Generator for unique transaction IDs
    pub(super) transaction_id_generator: UuidGenerator,

    /// The last price at which a trade occurred
    pub(super) last_trade_price: AtomicU64,

    /// Flag indicating if there was a trade
    pub(super) has_traded: AtomicBool,

    /// The timestamp of market close, if applicable (for DAY orders)
    pub(super) market_close_timestamp: AtomicU64,

    /// Flag indicating if market close is set
    pub(super) has_market_close: AtomicBool,

    /// A cache for storing best bid/ask prices to avoid recalculation
    pub(super) cache: PriceLevelCache,

    /// listens to possible trades when an order is added
    pub trade_listener: Option<TradeListener>,

    /// Phantom data to maintain generic type parameter
    _phantom: PhantomData<T>,
}

impl<T> Serialize for OrderBook<T>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        use std::collections::HashMap;
        use std::sync::atomic::Ordering;

        let mut state = serializer.serialize_struct("OrderBook", 9)?;

        // Serialize symbol
        state.serialize_field("symbol", &self.symbol)?;

        // Serialize bids as HashMap<u64, PriceLevel> using snapshots
        let bids: HashMap<u64, _> = self
            .bids
            .iter()
            .map(|entry| (*entry.key(), entry.value().snapshot()))
            .collect();
        state.serialize_field("bids", &bids)?;

        // Serialize asks as HashMap<u64, PriceLevel> using snapshots
        let asks: HashMap<u64, _> = self
            .asks
            .iter()
            .map(|entry| (*entry.key(), entry.value().snapshot()))
            .collect();
        state.serialize_field("asks", &asks)?;

        // Serialize order_locations as HashMap
        let order_locations: HashMap<OrderId, (u64, Side)> = self
            .order_locations
            .iter()
            .map(|entry| (*entry.key(), *entry.value()))
            .collect();
        state.serialize_field("order_locations", &order_locations)?;

        // Serialize atomic values by loading them
        state.serialize_field(
            "last_trade_price",
            &self.last_trade_price.load(Ordering::Relaxed),
        )?;
        state.serialize_field("has_traded", &self.has_traded.load(Ordering::Relaxed))?;
        state.serialize_field(
            "market_close_timestamp",
            &self.market_close_timestamp.load(Ordering::Relaxed),
        )?;
        state.serialize_field(
            "has_market_close",
            &self.has_market_close.load(Ordering::Relaxed),
        )?;

        // Serialize cache
        state.serialize_field("cache", &self.cache)?;

        // Skip trade_listener (cannot be serialized) and transaction_id_generator, _phantom

        state.end()
    }
}

impl<T> OrderBook<T>
where
    T: Default + Clone + Send + Sync + 'static,
{
    /// Convert OrderType<()> to `OrderType<T>` for return values
    pub fn convert_from_unit_type(&self, order: &OrderType<()>) -> OrderType<T>
    where
        T: Default,
    {
        match order {
            OrderType::Standard {
                id,
                price,
                quantity,
                side,
                timestamp,
                time_in_force,
                ..
            } => OrderType::Standard {
                id: *id,
                price: *price,
                quantity: *quantity,
                side: *side,
                timestamp: *timestamp,
                time_in_force: *time_in_force,
                extra_fields: T::default(),
            },
            OrderType::IcebergOrder {
                id,
                price,
                visible_quantity,
                hidden_quantity,
                side,
                timestamp,
                time_in_force,
                ..
            } => OrderType::IcebergOrder {
                id: *id,
                price: *price,
                visible_quantity: *visible_quantity,
                hidden_quantity: *hidden_quantity,
                side: *side,
                timestamp: *timestamp,
                time_in_force: *time_in_force,
                extra_fields: T::default(),
            },
            OrderType::PostOnly {
                id,
                price,
                quantity,
                side,
                timestamp,
                time_in_force,
                ..
            } => OrderType::PostOnly {
                id: *id,
                price: *price,
                quantity: *quantity,
                side: *side,
                timestamp: *timestamp,
                time_in_force: *time_in_force,
                extra_fields: T::default(),
            },
            OrderType::TrailingStop {
                id,
                price,
                quantity,
                side,
                timestamp,
                time_in_force,
                trail_amount,
                last_reference_price,
                ..
            } => OrderType::TrailingStop {
                id: *id,
                price: *price,
                quantity: *quantity,
                side: *side,
                timestamp: *timestamp,
                time_in_force: *time_in_force,
                trail_amount: *trail_amount,
                last_reference_price: *last_reference_price,
                extra_fields: T::default(),
            },
            OrderType::PeggedOrder {
                id,
                price,
                quantity,
                side,
                timestamp,
                time_in_force,
                reference_price_offset,
                reference_price_type,
                ..
            } => OrderType::PeggedOrder {
                id: *id,
                price: *price,
                quantity: *quantity,
                side: *side,
                timestamp: *timestamp,
                time_in_force: *time_in_force,
                reference_price_offset: *reference_price_offset,
                reference_price_type: *reference_price_type,
                extra_fields: T::default(),
            },
            OrderType::MarketToLimit {
                id,
                price,
                quantity,
                side,
                timestamp,
                time_in_force,
                ..
            } => OrderType::MarketToLimit {
                id: *id,
                price: *price,
                quantity: *quantity,
                side: *side,
                timestamp: *timestamp,
                time_in_force: *time_in_force,
                extra_fields: T::default(),
            },
            OrderType::ReserveOrder {
                id,
                price,
                visible_quantity,
                hidden_quantity,
                side,
                timestamp,
                time_in_force,
                replenish_threshold,
                replenish_amount,
                auto_replenish,
                ..
            } => OrderType::ReserveOrder {
                id: *id,
                price: *price,
                visible_quantity: *visible_quantity,
                hidden_quantity: *hidden_quantity,
                side: *side,
                timestamp: *timestamp,
                time_in_force: *time_in_force,
                replenish_threshold: *replenish_threshold,
                replenish_amount: *replenish_amount,
                auto_replenish: *auto_replenish,
                extra_fields: T::default(),
            },
        }
    }
    /// Create a new order book for the given symbol
    pub fn new(symbol: &str) -> Self {
        // Create a unique namespace for this order book's transaction IDs
        let namespace = Uuid::new_v4();

        Self {
            symbol: symbol.to_string(),
            bids: SkipMap::new(),
            asks: SkipMap::new(),
            order_locations: DashMap::new(),
            transaction_id_generator: UuidGenerator::new(namespace),
            last_trade_price: AtomicU64::new(0),
            has_traded: AtomicBool::new(false),
            market_close_timestamp: AtomicU64::new(0),
            has_market_close: AtomicBool::new(false),
            cache: PriceLevelCache::new(),
            trade_listener: None,
            _phantom: PhantomData,
        }
    }

    /// Create a new order book for the given symbol with a trade listener
    pub fn with_trade_listener(symbol: &str, trade_listener: TradeListener) -> Self {
        let namespace = Uuid::new_v4();

        Self {
            symbol: symbol.to_string(),
            bids: SkipMap::new(),
            asks: SkipMap::new(),
            order_locations: DashMap::new(),
            transaction_id_generator: UuidGenerator::new(namespace),
            last_trade_price: AtomicU64::new(0),
            has_traded: AtomicBool::new(false),
            market_close_timestamp: AtomicU64::new(0),
            has_market_close: AtomicBool::new(false),
            cache: PriceLevelCache::new(),
            trade_listener: Some(trade_listener),
            _phantom: PhantomData,
        }
    }

    /// Set a trade listener for this order book
    pub fn set_trade_listener(&mut self, trade_listener: TradeListener) {
        self.trade_listener = Some(trade_listener);
    }

    /// Remove the trade listener from this order book
    pub fn remove_trade_listener(&mut self) {
        self.trade_listener = None;
    }

    /// Get the symbol of this order book
    pub fn symbol(&self) -> &str {
        &self.symbol
    }

    /// Set the market close timestamp for DAY orders
    pub fn set_market_close_timestamp(&self, timestamp: u64) {
        self.market_close_timestamp
            .store(timestamp, Ordering::SeqCst);
        self.has_market_close.store(true, Ordering::SeqCst);
        trace!(
            "Order book {}: Set market close timestamp to {}",
            self.symbol, timestamp
        );
    }

    /// Clear the market close timestamp
    pub fn clear_market_close_timestamp(&self) {
        self.has_market_close.store(false, Ordering::SeqCst);
    }

    /// Get the best bid price, if any
    ///
    /// # Performance
    /// O(1) operation using SkipMap's ordered structure (highest price is last)
    pub fn best_bid(&self) -> Option<u64> {
        if let Some(cached_bid) = self.cache.get_cached_best_bid() {
            return Some(cached_bid);
        }

        // SkipMap maintains sorted order, best bid (highest price) is last
        let best_price = self.bids.iter().next_back().map(|entry| *entry.key());

        self.cache.update_best_prices(best_price, None);

        best_price
    }

    /// Get the best ask price, if any
    ///
    /// # Performance
    /// O(1) operation using SkipMap's ordered structure (lowest price is first)
    pub fn best_ask(&self) -> Option<u64> {
        if let Some(cached_ask) = self.cache.get_cached_best_ask() {
            return Some(cached_ask);
        }

        // SkipMap maintains sorted order, best ask (lowest price) is first
        let best_price = self.asks.iter().next().map(|entry| *entry.key());

        self.cache.update_best_prices(None, best_price);

        best_price
    }

    /// Get the mid price (average of best bid and best ask)
    pub fn mid_price(&self) -> Option<f64> {
        match (
            OrderBook::<T>::best_bid(self),
            OrderBook::<T>::best_ask(self),
        ) {
            (Some(bid), Some(ask)) => Some((bid as f64 + ask as f64) / 2.0),
            _ => None,
        }
    }

    /// Get the last trade price, if any
    pub fn last_trade_price(&self) -> Option<u64> {
        if self.has_traded.load(Ordering::Relaxed) {
            Some(self.last_trade_price.load(Ordering::Relaxed))
        } else {
            None
        }
    }

    /// Get the spread (best ask - best bid)
    pub fn spread(&self) -> Option<u64> {
        match (
            OrderBook::<T>::best_bid(self),
            OrderBook::<T>::best_ask(self),
        ) {
            (Some(bid), Some(ask)) => Some(ask.saturating_sub(bid)),
            _ => None,
        }
    }

    /// Get all orders at a specific price level
    pub fn get_orders_at_price(&self, price: u64, side: Side) -> Vec<Arc<OrderType<T>>>
    where
        T: Default,
    {
        trace!(
            "Order book {}: Getting orders at price {} for side {:?}",
            self.symbol, price, side
        );
        let price_levels = match side {
            Side::Buy => &self.bids,
            Side::Sell => &self.asks,
        };

        if let Some(entry) = price_levels.get(&price) {
            entry
                .value()
                .iter_orders()
                .into_iter()
                .map(|order| Arc::new(self.convert_from_unit_type(&order)))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Get all orders in the book
    pub fn get_all_orders(&self) -> Vec<Arc<OrderType<T>>>
    where
        T: Default,
    {
        trace!("Order book {}: Getting all orders", self.symbol);
        let mut result = Vec::new();

        // Get all bid orders
        for item in self.bids.iter() {
            let price_level = item.value();
            let converted_orders: Vec<Arc<OrderType<T>>> = price_level
                .iter_orders()
                .into_iter()
                .map(|order| Arc::new(self.convert_from_unit_type(&order)))
                .collect();
            result.extend(converted_orders);
        }

        // Get all ask orders
        for item in self.asks.iter() {
            let price_level = item.value();
            let converted_orders: Vec<Arc<OrderType<T>>> = price_level
                .iter_orders()
                .into_iter()
                .map(|order| Arc::new(self.convert_from_unit_type(&order)))
                .collect();
            result.extend(converted_orders);
        }

        result
    }

    /// Get an order by its ID
    pub fn get_order(&self, order_id: OrderId) -> Option<Arc<OrderType<T>>>
    where
        T: Default,
    {
        // Get the order location without locking
        if let Some(location) = self.order_locations.get(&order_id) {
            let (price, side) = *location;

            let price_levels = match side {
                Side::Buy => &self.bids,
                Side::Sell => &self.asks,
            };

            // Get the price level
            if let Some(entry) = price_levels.get(&price) {
                let price_level = entry.value();
                // Iterate through the orders at this level to find the one with the matching ID
                for order in price_level.iter_orders() {
                    if order.id() == order_id {
                        return Some(Arc::new(self.convert_from_unit_type(&order)));
                    }
                }
            }
        }

        None
    }

    /// Match a market order against the book
    pub fn match_market_order(
        &self,
        order_id: OrderId,
        quantity: u64,
        side: Side,
    ) -> Result<MatchResult, OrderBookError> {
        trace!(
            "Order book {}: Matching market order {} for {} at side {:?}",
            self.symbol, order_id, quantity, side
        );
        let match_result = OrderBook::<T>::match_order(self, order_id, side, quantity, None)?;

        // Trigger trade listener if there are transactions
        if !match_result.transactions.transactions.is_empty()
            && let Some(ref listener) = self.trade_listener
        {
            let trade_result = TradeResult::new(self.symbol.clone(), match_result.clone());
            listener(&trade_result);
        }

        Ok(match_result)
    }

    /// Attempts to match a limit order in the order book.
    ///
    /// # Parameters
    /// - `order_id`: The unique identifier of the order to be matched.
    /// - `quantity`: The quantity of the order to be matched.
    /// - `side`: The side of the order book (e.g., Buy or Sell) on which the order resides.
    /// - `limit_price`: The maximum (for Buy) or minimum (for Sell) acceptable price
    ///   for the order.
    ///
    /// # Returns
    /// - `Ok(MatchResult)`: If the order is successfully matched, returning information
    ///   about the match, including possibly filled quantities and pricing details.
    /// - `Err(OrderBookError)`: If the order cannot be matched due to an error, such as
    ///   invalid parameters or an existing order book issue.
    ///
    /// # Behavior
    /// - Logs a trace message with details about the order and its intended match parameters.
    /// - Internally delegates to the `match_order` function, passing the provided parameters,
    ///   including the optional `limit_price` which specifies the price constraint.
    ///
    /// # Errors
    /// This function returns an error in cases such as:
    /// - The specified `order_id` is not found in the order book.
    /// - The provided parameters are invalid (e.g., negative quantity).
    /// - The attempted match is not feasible within the order book's current state.
    ///
    /// # Notes
    /// - The `limit_price` parameter sets a constraint on the match price:
    ///   - For Buy orders, it specifies the maximum acceptable price.
    ///   - For Sell orders, it specifies the minimum acceptable price.
    /// - If `limit_price` is not met during the matching process, the order will not be executed.
    pub fn match_limit_order(
        &self,
        order_id: OrderId,
        quantity: u64,
        side: Side,
        limit_price: u64,
    ) -> Result<MatchResult, OrderBookError> {
        trace!(
            "Order book {}: Matching limit order {} for {} at side {:?} with limit price {}",
            self.symbol, order_id, quantity, side, limit_price
        );
        let match_result =
            OrderBook::<T>::match_order(self, order_id, side, quantity, Some(limit_price))?;

        // Trigger trade listener if there are transactions
        if !match_result.transactions.transactions.is_empty()
            && let Some(ref listener) = self.trade_listener
        {
            let trade_result = TradeResult::new(self.symbol.clone(), match_result.clone());
            listener(&trade_result);
        }

        Ok(match_result)
    }

    /// Create a snapshot of the current order book state
    pub fn create_snapshot(&self, depth: usize) -> OrderBookSnapshot {
        // Get all bid prices and sort them in descending order
        let mut bid_prices: Vec<u64> = self.bids.iter().map(|item| *item.key()).collect();
        bid_prices.sort_by(|a, b| b.cmp(a)); // Descending order
        bid_prices.truncate(depth);

        // Get all ask prices and sort them in ascending order
        let mut ask_prices: Vec<u64> = self.asks.iter().map(|item| *item.key()).collect();
        ask_prices.sort(); // Ascending order
        ask_prices.truncate(depth);

        let mut bid_levels = Vec::with_capacity(bid_prices.len());
        let mut ask_levels = Vec::with_capacity(ask_prices.len());

        // Create snapshots for each bid level
        for price in bid_prices {
            if let Some(entry) = self.bids.get(&price) {
                bid_levels.push(entry.value().snapshot());
            }
        }

        // Create snapshots for each ask level
        for price in ask_prices {
            if let Some(entry) = self.asks.get(&price) {
                ask_levels.push(entry.value().snapshot());
            }
        }

        OrderBookSnapshot {
            symbol: self.symbol.clone(),
            timestamp: current_time_millis(),
            bids: bid_levels,
            asks: ask_levels,
        }
    }

    /// Create a checksum-protected snapshot package of the entire book.
    pub fn create_snapshot_package(
        &self,
        depth: usize,
    ) -> Result<OrderBookSnapshotPackage, OrderBookError> {
        let snapshot = self.create_snapshot(depth);
        OrderBookSnapshotPackage::new(snapshot)
    }

    /// Serialize a checksum-protected snapshot package to JSON.
    pub fn snapshot_to_json(&self, depth: usize) -> Result<String, OrderBookError> {
        self.create_snapshot_package(depth)?.to_json()
    }

    /// Restore the book state from a checksum-validated snapshot package.
    pub fn restore_from_snapshot_package(
        &self,
        package: OrderBookSnapshotPackage,
    ) -> Result<(), OrderBookError> {
        self.restore_from_snapshot(package.into_snapshot()?)
    }

    /// Restore the book state from a JSON payload containing a checksum-protected snapshot package.
    pub fn restore_from_snapshot_json(&self, data: &str) -> Result<(), OrderBookError> {
        let package = OrderBookSnapshotPackage::from_json(data)?;
        self.restore_from_snapshot_package(package)
    }

    /// Restore the book state from a snapshot, without checksum validation.
    pub fn restore_from_snapshot(&self, snapshot: OrderBookSnapshot) -> Result<(), OrderBookError> {
        if snapshot.symbol != self.symbol {
            return Err(OrderBookError::InvalidOperation {
                message: format!(
                    "Snapshot symbol {} does not match order book symbol {}",
                    snapshot.symbol, self.symbol
                ),
            });
        }

        self.cache.invalidate();

        // Clear all existing data
        while let Some(entry) = self.bids.pop_front() {
            drop(entry);
        }
        while let Some(entry) = self.asks.pop_front() {
            drop(entry);
        }
        self.order_locations.clear();
        self.has_traded.store(false, Ordering::Relaxed);
        self.last_trade_price.store(0, Ordering::Relaxed);
        self.has_market_close.store(false, Ordering::Relaxed);
        self.market_close_timestamp.store(0, Ordering::Relaxed);

        for level_snapshot in snapshot.bids {
            let price = level_snapshot.price;
            let price_level: PriceLevel = PriceLevel::from(&level_snapshot);
            let arc_level = Arc::new(price_level);
            self.bids.insert(price, arc_level);
        }

        for level_snapshot in snapshot.asks {
            let price = level_snapshot.price;
            let price_level: PriceLevel = PriceLevel::from(&level_snapshot);
            let arc_level = Arc::new(price_level);
            self.asks.insert(price, arc_level);
        }

        // Rebuild order location map with generic order types
        for item in self.bids.iter() {
            let price = *item.key();
            let level = item.value();
            for order in level.iter_orders() {
                self.order_locations.insert(order.id(), (price, Side::Buy));
            }
        }

        for item in self.asks.iter() {
            let price = *item.key();
            let level = item.value();
            for order in level.iter_orders() {
                self.order_locations.insert(order.id(), (price, Side::Sell));
            }
        }

        Ok(())
    }

    /// Get the total volume at each price level
    pub fn get_volume_by_price(&self) -> (HashMap<u64, u64>, HashMap<u64, u64>) {
        let mut bid_volumes = HashMap::new();
        let mut ask_volumes = HashMap::new();

        // Calculate bid volumes
        for item in self.bids.iter() {
            let price = *item.key();
            let price_level = item.value();
            bid_volumes.insert(price, price_level.total_quantity());
        }

        // Calculate ask volumes
        for item in self.asks.iter() {
            let price = *item.key();
            let price_level = item.value();
            ask_volumes.insert(price, price_level.total_quantity());
        }

        (bid_volumes, ask_volumes)
    }

    /// Get an Arc reference to the bids as a DashMap
    ///
    /// # Note
    /// Creates a snapshot by collecting all entries into a DashMap
    pub fn get_bids(&self) -> Arc<DashMap<u64, Arc<PriceLevel>>> {
        let map = DashMap::new();
        for entry in self.bids.iter() {
            map.insert(*entry.key(), entry.value().clone());
        }
        Arc::new(map)
    }

    /// Get an Arc reference to the asks as a DashMap
    ///
    /// # Note
    /// Creates a snapshot by collecting all entries into a DashMap
    pub fn get_asks(&self) -> Arc<DashMap<u64, Arc<PriceLevel>>> {
        let map = DashMap::new();
        for entry in self.asks.iter() {
            map.insert(*entry.key(), entry.value().clone());
        }
        Arc::new(map)
    }

    /// Get a BTreeMap of bids with price as key and PriceLevel as value
    pub fn get_bt_bids(&self) -> BTreeMap<u64, PriceLevel> {
        self.bids
            .iter()
            .map(|entry| {
                let price = *entry.key();
                let snapshot = entry.value().snapshot();
                let price_level = PriceLevel::from(&snapshot);
                (price, price_level)
            })
            .collect()
    }

    /// Get a BTreeMap of asks with price as key and PriceLevel as value
    pub fn get_bt_asks(&self) -> BTreeMap<u64, PriceLevel> {
        self.asks
            .iter()
            .map(|entry| {
                let price = *entry.key();
                let snapshot = entry.value().snapshot();
                let price_level = PriceLevel::from(&snapshot);
                (price, price_level)
            })
            .collect()
    }

    /// Get an Arc reference to the order_locations DashMap
    pub fn get_order_locations_arc(&self) -> Arc<DashMap<OrderId, (u64, Side)>> {
        Arc::new(self.order_locations.clone())
    }
}
