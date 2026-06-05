# Reporting query: revenue per customer

This query aggregates order revenue per customer. The load-bearing structure is
the `customer_id` grouping key, the `SUM(amount)` aggregation aliased as
`total_amount`, and the join from `orders` to `customers` on
`orders.customer_id = customers.id`.

```sql
SELECT customer_id, SUM(amount) AS total_amount
FROM orders
JOIN customers ON orders.customer_id = customers.id
GROUP BY customer_id;
```
