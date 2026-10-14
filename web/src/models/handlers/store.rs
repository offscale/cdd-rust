#![deny(missing_docs)]

//! Store endpoint parameter models.

use serde::{Deserialize, Serialize};

/// Path parameter model for store order operations (`/store/order/{orderId}`).
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub struct OrderIdParam {
    /// Numeric ID of the order.
    pub order_id: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_order_id_param() {
        let p = OrderIdParam { order_id: 42 };
        let s = serde_json::to_string(&p).expect("must serialize");
        let d: OrderIdParam = serde_json::from_str(&s).expect("must deserialize");
        assert_eq!(d, p);
    }
}
