# SHOP-214

Approved by rade on 2026-10-05.

## Intent

Let customers cancel their own unpaid orders from the order page. Cancelling
releases the reserved stock and keeps the order in the history as
`cancelled`.

## Scope

Backend: `App\Actions\Orders\CancelOrder`, `OrderPolicy::cancel`, the route
`POST /orders/{order}/cancel` and its controller method. Frontend: a Cancel
button on `resources/js/Pages/Orders/Show.vue`, shown only for cancellable
orders. Out of scope: refunds, cancellation by staff, e-mail notifications.

## Open questions / answers

- Q: Can an order be cancelled once it has been paid? A: No. Only `pending`,
  unpaid orders; paid orders go through the refund flow.
- Q: Is a cancelled order deleted? A: No. It stays, with status `cancelled`,
  for the customer's history and for reporting.

## Rejected alternatives

- Soft-deleting the order: removes it from the customer's history and from
  reporting.
- Releasing the stock in a queued job: leaves a window in which the stock is
  neither reserved nor sellable.

## Acceptance criteria

- AC1: Given a pending, unpaid order of mine, when I cancel it, then its status
  is `cancelled` and its reserved stock is available for sale again.
- AC2: Given a paid order of mine, when I try to cancel it, then the request is
  refused with 403 and the order is unchanged.
- AC3: Given an order that belongs to another team, when I send a cancel request
  with its id, then the response is 404 and the order is unchanged.
- AC4: Given an order I cancelled, when I open my order history, then it is
  listed with status `cancelled`.

## Acceptance tests to change

- tests/Acceptance/Shop190PaidTest.php — change — its 403 no longer holds within 24 hours
- tests/Acceptance/Shop177RefundTest.php — delete — refunds moved to SHOP-301

## Implementation context

- AI_CONTEXT: controllers stay thin — the change lives in
  `App\Actions\Orders\CancelOrder`, and authorization goes through
  `OrderPolicy`, never through inline checks
- AI_CONTEXT: stock changes go through `StockLedger`, inside the action's
  database transaction
- BUSINESS_CONTEXT invariant: every query on team data is scoped to the
  current team by the `BelongsToTeam` global scope; request handling never
  calls `withoutGlobalScopes()`
