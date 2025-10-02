/******************************************************************************
   Author: Joaquín Béjar García
   Email: jb@taunais.com
   Date: 2/10/25
******************************************************************************/

//! Multi-book management with centralized trade event routing.
//!
//! This module provides the `BookManager` struct for managing multiple order books
//! with a unified trade event channel system.

use crate::orderbook::OrderBook;
use crate::orderbook::trade::{TradeEvent, TradeListener, TradeResult};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use tracing::{error, info};

/// Manages multiple order books with centralized trade event routing.
pub struct BookManager<T>
where
    T: Clone + Send + Sync + Default + 'static,
{
    /// Collection of order books indexed by symbol
    books: HashMap<String, OrderBook<T>>,
    /// Sender for trade events
    trade_sender: mpsc::Sender<TradeEvent>,
    /// Receiver for trade events (taken when processor starts)
    trade_receiver: Option<mpsc::Receiver<TradeEvent>>,
}

impl<T> BookManager<T>
where
    T: Clone + Send + Sync + Default + 'static,
{
    /// Create a new BookManager with a trade event channel.
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();

        Self {
            books: HashMap::new(),
            trade_sender: sender,
            trade_receiver: Some(receiver),
        }
    }

    /// Add a new order book for a symbol with an automatically configured trade listener.
    pub fn add_book(&mut self, symbol: &str) {
        let sender = self.trade_sender.clone();
        let symbol_clone = symbol.to_string();

        let trade_listener: TradeListener = Arc::new(move |trade_result: &TradeResult| {
            let trade_event = TradeEvent {
                symbol: trade_result.symbol.clone(),
                trade_result: trade_result.clone(),
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
            };

            if let Err(e) = sender.send(trade_event) {
                error!("Failed to send trade event for {}: {}", symbol_clone, e);
            }
        });

        let book = OrderBook::with_trade_listener(symbol, trade_listener);
        self.books.insert(symbol.to_string(), book);
        info!("Added order book for symbol: {}", symbol);
    }

    /// Get a reference to an order book by symbol.
    pub fn get_book(&self, symbol: &str) -> Option<&OrderBook<T>> {
        self.books.get(symbol)
    }

    /// Get a mutable reference to an order book by symbol.
    pub fn get_book_mut(&mut self, symbol: &str) -> Option<&mut OrderBook<T>> {
        self.books.get_mut(symbol)
    }

    /// Get the list of all symbols with order books in this manager.
    pub fn symbols(&self) -> Vec<String> {
        self.books.keys().cloned().collect()
    }

    /// Remove an order book for a specific symbol.
    pub fn remove_book(&mut self, symbol: &str) -> Option<OrderBook<T>> {
        let result = self.books.remove(symbol);
        if result.is_some() {
            info!("Removed order book for symbol: {}", symbol);
        }
        result
    }

    /// Check if a book exists for a specific symbol.
    pub fn has_book(&self, symbol: &str) -> bool {
        self.books.contains_key(symbol)
    }

    /// Start the trade event processor in a separate thread.
    pub fn start_trade_processor(&mut self) -> thread::JoinHandle<()> {
        let receiver = self
            .trade_receiver
            .take()
            .expect("Trade processor already started");

        thread::spawn(move || {
            info!("Trade processor started");

            while let Ok(trade_event) = receiver.recv() {
                Self::process_trade_event(trade_event);
            }

            info!("Trade processor stopped");
        })
    }

    /// Process a single trade event.
    fn process_trade_event(event: TradeEvent) {
        info!(
            "Processing trade for {}: {} transactions, executed quantity: {}",
            event.symbol,
            event
                .trade_result
                .match_result
                .transactions
                .transactions
                .len(),
            event.trade_result.match_result.executed_quantity()
        );

        for transaction in event.trade_result.match_result.transactions.as_vec() {
            info!(
                "  Transaction: {} units at price {} (ID: {})",
                transaction.quantity, transaction.price, transaction.transaction_id
            );
        }
    }

    /// Get the number of order books in this manager.
    pub fn book_count(&self) -> usize {
        self.books.len()
    }
}

impl<T> Default for BookManager<T>
where
    T: Clone + Send + Sync + Default + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}
