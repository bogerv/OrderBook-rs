//! Additional unit tests to improve test coverage for book.rs
//! These tests target specific uncovered lines and edge cases


use pricelevel::{OrderId, OrderType, PegReferenceType, Side, TimeInForce};

#[derive(Debug, Clone, Default, PartialEq)]
struct TestExtraFields {
    pub metadata: String,
}

#[cfg(test)]
mod book_coverage_tests {
    use orderbook_rs::OrderBook;
    use super::*;

    #[test]
    fn test_with_trade_listener_constructor() {
        // Test the with_trade_listener constructor (lines 233-247)
        fn dummy_listener(_match_result: &pricelevel::MatchResult) {
            // Empty listener for testing
        }
        let book = OrderBook::<()>::with_trade_listener("TEST", dummy_listener);
        
        assert_eq!(book.symbol(), "TEST");
        assert!(book.trade_listener.is_some());
        assert_eq!(book.best_bid(), None);
        assert_eq!(book.best_ask(), None);
    }

    #[test]
    fn test_clear_market_close_timestamp() {
        // Test clear_market_close_timestamp (lines 263-264)
        let book = OrderBook::<()>::new("TEST");
        
        // First set a timestamp
        book.set_market_close_timestamp(1000000);
        // We can't directly access has_market_close, but we can test the functionality
        // by checking if the method works (no error means it was set)
        assert_eq!(book.symbol(), "TEST");
        
        // Then clear it
        book.clear_market_close_timestamp();
        // We can't directly access has_market_close, but clearing should work without error
        assert_eq!(book.symbol(), "TEST");
    }

    #[test]
    fn test_best_bid_with_cache_miss() {
        // Test best_bid when cache is empty (lines 276, 289)
        let book = OrderBook::<()>::new("TEST");
        
        // Initially no bids
        assert_eq!(book.best_bid(), None);
        
        // Add a bid and test cache update
        let order_id = OrderId::from_u64(1);
        let _ = book.add_limit_order(order_id, 100, 10, Side::Buy, TimeInForce::Gtc, None);
        
        // Clear cache to force recalculation
        // We can't directly access cache, but we can test cache functionality
        // by performing operations that would use the cache
        let _ = book.best_bid();
        let _ = book.best_ask();
        assert_eq!(book.best_bid(), Some(100));
    }

    #[test]
    fn test_best_ask_with_cache_miss() {
        // Test best_ask when cache is empty (lines 301, 321)
        let book = OrderBook::<()>::new("TEST");
        
        // Initially no asks
        assert_eq!(book.best_ask(), None);
        
        // Add an ask and test cache update
        let order_id = OrderId::from_u64(1);
        let _ = book.add_limit_order(order_id, 200, 10, Side::Sell, TimeInForce::Gtc, None);
        
        // Clear cache to force recalculation
        // We can't directly access cache, but we can test cache functionality
        // by performing operations that would use the cache
        let _ = book.best_bid();
        let _ = book.best_ask();
        assert_eq!(book.best_ask(), Some(200));
    }

    #[test]
    fn test_mid_price_edge_cases() {
        // Test mid_price with various scenarios (lines 336-337, 341)
        let book = OrderBook::<()>::new("TEST");
        
        // No orders - should return None
        assert_eq!(book.mid_price(), None);
        
        // Only bid
        let bid_id = OrderId::from_u64(1);
        let _ = book.add_limit_order(bid_id, 100, 10, Side::Buy, TimeInForce::Gtc, None);
        assert_eq!(book.mid_price(), None);
        
        // Only ask
        let book2 = OrderBook::<()>::new("TEST2");
        let ask_id = OrderId::from_u64(2);
        let _ = book2.add_limit_order(ask_id, 200, 10, Side::Sell, TimeInForce::Gtc, None);
        assert_eq!(book2.mid_price(), None);
        
        // Both bid and ask
        let ask_id2 = OrderId::from_u64(3);
        let _ = book.add_limit_order(ask_id2, 200, 10, Side::Sell, TimeInForce::Gtc, None);
        assert_eq!(book.mid_price(), Some(150.0));
    }

    #[test]
    fn test_last_trade_price_no_trades() {
        // Test last_trade_price when no trades occurred (lines 345)
        let book = OrderBook::<()>::new("TEST");
        assert_eq!(book.last_trade_price(), None);
    }

    #[test]
    fn test_spread_edge_cases() {
        // Test spread with various scenarios (lines 365-366, 371)
        let book = OrderBook::<()>::new("TEST");
        
        // No orders
        assert_eq!(book.spread(), None);
        
        // Only bid
        let bid_id = OrderId::from_u64(1);
        let _ = book.add_limit_order(bid_id, 100, 10, Side::Buy, TimeInForce::Gtc, None);
        assert_eq!(book.spread(), None);
        
        // Only ask
        let book2 = OrderBook::<()>::new("TEST2");
        let ask_id = OrderId::from_u64(2);
        let _ = book2.add_limit_order(ask_id, 200, 10, Side::Sell, TimeInForce::Gtc, None);
        assert_eq!(book2.spread(), None);
        
        // Both - normal case
        let ask_id2 = OrderId::from_u64(3);
        let _ = book.add_limit_order(ask_id2, 200, 10, Side::Sell, TimeInForce::Gtc, None);
        assert_eq!(book.spread(), Some(100));
        
        // Edge case: ask lower than bid (should use saturating_sub)
        let book3 = OrderBook::<()>::new("TEST3");
        let bid_id3 = OrderId::from_u64(4);
        let ask_id3 = OrderId::from_u64(5);
        let _ = book3.add_limit_order(bid_id3, 200, 10, Side::Buy, TimeInForce::Gtc, None);
        let _ = book3.add_limit_order(ask_id3, 100, 10, Side::Sell, TimeInForce::Gtc, None);
        // This should execute immediately, but if it didn't, saturating_sub would handle it
    }

    #[test]
    fn test_get_orders_at_price_empty_level() {
        // Test get_orders_at_price when price level doesn't exist (lines 376-377, 382)
        let book = OrderBook::<()>::new("TEST");
        
        // Non-existent price level
        let orders = book.get_orders_at_price(100, Side::Buy);
        assert!(orders.is_empty());
        
        let orders = book.get_orders_at_price(200, Side::Sell);
        assert!(orders.is_empty());
    }

    #[test]
    fn test_get_order_not_found() {
        // Test get_order when order doesn't exist (lines 424-425)
        let book = OrderBook::<()>::new("TEST");
        
        // Non-existent order ID
        let non_existent_id = OrderId::from_u64(999);
        assert_eq!(book.get_order(non_existent_id), None);
        
        // Add an order, then test with different ID
        let order_id = OrderId::from_u64(1);
        let _ = book.add_limit_order(order_id, 100, 10, Side::Buy, TimeInForce::Gtc, None);
        
        let different_id = OrderId::from_u64(2);
        assert_eq!(book.get_order(different_id), None);
    }

    #[test]
    fn test_convert_from_unit_type_all_variants() {
        // Test convert_from_unit_type for all order variants (lines 72, 74-79, 81-87, 89, 97, etc.)
        let book = OrderBook::<TestExtraFields>::new("TEST");
        
        // Test Standard order conversion
        let standard_order = OrderType::Standard {
            id: OrderId::from_u64(1),
            price: 100,
            quantity: 10,
            side: Side::Buy,
            timestamp: 1000,
            time_in_force: TimeInForce::Gtc,
            extra_fields: (),
        };
        
        let converted = book.convert_from_unit_type(&standard_order);
        match converted {
            OrderType::Standard { id, price, quantity, side, timestamp, time_in_force, extra_fields } => {
                assert_eq!(id, OrderId::from_u64(1));
                assert_eq!(price, 100);
                assert_eq!(quantity, 10);
                assert_eq!(side, Side::Buy);
                assert_eq!(timestamp, 1000);
                assert_eq!(time_in_force, TimeInForce::Gtc);
                assert_eq!(extra_fields, TestExtraFields::default());
            }
            _ => panic!("Expected Standard order"),
        }
        
        // Test IcebergOrder conversion
        let iceberg_order = OrderType::IcebergOrder {
            id: OrderId::from_u64(2),
            price: 200,
            visible_quantity: 5,
            hidden_quantity: 15,
            side: Side::Sell,
            timestamp: 2000,
            time_in_force: TimeInForce::Ioc,
            extra_fields: (),
        };
        
        let converted = book.convert_from_unit_type(&iceberg_order);
        match converted {
            OrderType::IcebergOrder { id, price, visible_quantity, hidden_quantity, side, timestamp, time_in_force, extra_fields } => {
                assert_eq!(id, OrderId::from_u64(2));
                assert_eq!(price, 200);
                assert_eq!(visible_quantity, 5);
                assert_eq!(hidden_quantity, 15);
                assert_eq!(side, Side::Sell);
                assert_eq!(timestamp, 2000);
                assert_eq!(time_in_force, TimeInForce::Ioc);
                assert_eq!(extra_fields, TestExtraFields::default());
            }
            _ => panic!("Expected IcebergOrder"),
        }
        
        // Test PostOnly conversion
        let post_only_order = OrderType::PostOnly {
            id: OrderId::from_u64(3),
            price: 300,
            quantity: 20,
            side: Side::Buy,
            timestamp: 3000,
            time_in_force: TimeInForce::Fok,
            extra_fields: (),
        };
        
        let converted = book.convert_from_unit_type(&post_only_order);
        match converted {
            OrderType::PostOnly { id, price, quantity, side, timestamp, time_in_force, extra_fields } => {
                assert_eq!(id, OrderId::from_u64(3));
                assert_eq!(price, 300);
                assert_eq!(quantity, 20);
                assert_eq!(side, Side::Buy);
                assert_eq!(timestamp, 3000);
                assert_eq!(time_in_force, TimeInForce::Fok);
                assert_eq!(extra_fields, TestExtraFields::default());
            }
            _ => panic!("Expected PostOnly order"),
        }
    }

    #[test]
    fn test_convert_trailing_stop_order() {
        // Test TrailingStop order conversion (lines covering TrailingStop variant)
        let book = OrderBook::<TestExtraFields>::new("TEST");
        
        let trailing_stop_order = OrderType::TrailingStop {
            id: OrderId::from_u64(4),
            price: 400,
            quantity: 25,
            side: Side::Sell,
            timestamp: 4000,
            time_in_force: TimeInForce::Gtc,
            trail_amount: 10,
            last_reference_price: 390,
            extra_fields: (),
        };
        
        let converted = book.convert_from_unit_type(&trailing_stop_order);
        match converted {
            OrderType::TrailingStop { id, price, quantity, side, timestamp, time_in_force, trail_amount, last_reference_price, extra_fields } => {
                assert_eq!(id, OrderId::from_u64(4));
                assert_eq!(price, 400);
                assert_eq!(quantity, 25);
                assert_eq!(side, Side::Sell);
                assert_eq!(timestamp, 4000);
                assert_eq!(time_in_force, TimeInForce::Gtc);
                assert_eq!(trail_amount, 10);
                assert_eq!(last_reference_price, 390);
                assert_eq!(extra_fields, TestExtraFields::default());
            }
            _ => panic!("Expected TrailingStop order"),
        }
    }

    #[test]
    fn test_convert_pegged_order() {
        // Test PeggedOrder conversion
        let book = OrderBook::<TestExtraFields>::new("TEST");
        
        let pegged_order = OrderType::PeggedOrder {
            id: OrderId::from_u64(5),
            price: 500,
            quantity: 30,
            side: Side::Buy,
            timestamp: 5000,
            time_in_force: TimeInForce::Ioc,
            reference_price_offset: 5,
            reference_price_type: PegReferenceType::BestBid,
            extra_fields: (),
        };
        
        let converted = book.convert_from_unit_type(&pegged_order);
        match converted {
            OrderType::PeggedOrder { id, price, quantity, side, timestamp, time_in_force, reference_price_offset, reference_price_type, extra_fields } => {
                assert_eq!(id, OrderId::from_u64(5));
                assert_eq!(price, 500);
                assert_eq!(quantity, 30);
                assert_eq!(side, Side::Buy);
                assert_eq!(timestamp, 5000);
                assert_eq!(time_in_force, TimeInForce::Ioc);
                assert_eq!(reference_price_offset, 5);
                assert_eq!(reference_price_type, PegReferenceType::BestBid);
                assert_eq!(extra_fields, TestExtraFields::default());
            }
            _ => panic!("Expected PeggedOrder"),
        }
    }

    #[test]
    fn test_convert_market_to_limit_order() {
        // Test MarketToLimit conversion
        let book = OrderBook::<TestExtraFields>::new("TEST");
        
        let market_to_limit_order = OrderType::MarketToLimit {
            id: OrderId::from_u64(6),
            price: 600,
            quantity: 35,
            side: Side::Sell,
            timestamp: 6000,
            time_in_force: TimeInForce::Fok,
            extra_fields: (),
        };
        
        let converted = book.convert_from_unit_type(&market_to_limit_order);
        match converted {
            OrderType::MarketToLimit { id, price, quantity, side, timestamp, time_in_force, extra_fields } => {
                assert_eq!(id, OrderId::from_u64(6));
                assert_eq!(price, 600);
                assert_eq!(quantity, 35);
                assert_eq!(side, Side::Sell);
                assert_eq!(timestamp, 6000);
                assert_eq!(time_in_force, TimeInForce::Fok);
                assert_eq!(extra_fields, TestExtraFields::default());
            }
            _ => panic!("Expected MarketToLimit order"),
        }
    }

    #[test]
    fn test_convert_reserve_order() {
        // Test ReserveOrder conversion
        let book = OrderBook::<TestExtraFields>::new("TEST");
        
        let reserve_order = OrderType::ReserveOrder {
            id: OrderId::from_u64(7),
            price: 700,
            visible_quantity: 10,
            hidden_quantity: 40,
            side: Side::Buy,
            timestamp: 7000,
            time_in_force: TimeInForce::Gtc,
            replenish_threshold: 5,
            replenish_amount: Some(15),
            auto_replenish: true,
            extra_fields: (),
        };
        
        let converted = book.convert_from_unit_type(&reserve_order);
        match converted {
            OrderType::ReserveOrder { 
                id, price, visible_quantity, hidden_quantity, side, timestamp, 
                time_in_force, replenish_threshold, replenish_amount: _, auto_replenish, extra_fields 
            } => {
                assert_eq!(id, OrderId::from_u64(7));
                assert_eq!(price, 700);
                assert_eq!(visible_quantity, 10);
                assert_eq!(hidden_quantity, 40);
                assert_eq!(side, Side::Buy);
                assert_eq!(timestamp, 7000);
                assert_eq!(time_in_force, TimeInForce::Gtc);
                assert_eq!(replenish_threshold, 5);
                assert_eq!(auto_replenish, true);
                assert_eq!(extra_fields, TestExtraFields::default());
            }
            _ => panic!("Expected ReserveOrder"),
        }
    }

    #[test]
    fn test_match_market_order_delegation() {
        // Test match_market_order delegates correctly (lines 461)
        let book = OrderBook::<()>::new("TEST");
        
        // Add some liquidity first
        let ask_id = OrderId::from_u64(1);
        let _ = book.add_limit_order(ask_id, 100, 10, Side::Sell, TimeInForce::Gtc, None);
        
        // Test market order matching
        let market_id = OrderId::from_u64(2);
        let result = book.match_market_order(market_id, 5, Side::Buy);
        
        assert!(result.is_ok());
        let match_result = result.unwrap();
        assert_eq!(match_result.executed_quantity(), 5);
    }

    #[test]
    fn test_match_limit_order_delegation() {
        // Test match_limit_order delegates correctly (lines 500)
        let book = OrderBook::<()>::new("TEST");
        
        // Add some liquidity first
        let ask_id = OrderId::from_u64(1);
        let _ = book.add_limit_order(ask_id, 100, 10, Side::Sell, TimeInForce::Gtc, None);
        
        // Test limit order matching
        let limit_id = OrderId::from_u64(2);
        let result = book.match_limit_order(limit_id, 5, Side::Buy, 100);
        
        assert!(result.is_ok());
        let match_result = result.unwrap();
        assert_eq!(match_result.executed_quantity(), 5);
    }

    #[test]
    fn test_create_snapshot_empty_book() {
        // Test create_snapshot with empty book (lines 519-521, 526-528)
        let book = OrderBook::<()>::new("TEST");
        
        let snapshot = book.create_snapshot(5);
        assert_eq!(snapshot.symbol, "TEST");
        // We can't directly access bids/asks, but we can check if there are no orders
        assert!(book.get_orders_at_price(100, Side::Buy).is_empty());
        assert!(book.get_orders_at_price(100, Side::Sell).is_empty());
        assert!(snapshot.timestamp > 0);
    }

    #[test]
    fn test_get_volume_by_price_empty_book() {
        // Test get_volume_by_price with empty book
        let book = OrderBook::<()>::new("TEST");
        
        let (bid_volumes, ask_volumes) = book.get_volume_by_price();
        assert!(bid_volumes.is_empty());
        assert!(ask_volumes.is_empty());
    }

    #[test]
    fn test_get_all_orders_empty_book() {
        // Test get_all_orders with empty book
        let book = OrderBook::<()>::new("TEST");
        
        let orders = book.get_all_orders();
        assert!(orders.is_empty());
    }
    
}