# SHOP-214

Approved by rade on 2026-10-05.

**No acceptance tests:** this ticket was approved before invar recorded that decision, which counts as without them.

**Invariants named in the Plan:** INV-3 (2 of 3 in BUSINESS_CONTEXT.md not named)

## Intent

Let customers cancel their own unpaid orders from the order page.

## Scope

`App\Actions\Orders\CancelOrder`, `OrderPolicy::cancel` and the route
`POST /orders/{order}/cancel`.

## Open questions / answers

- Q: Can a paid order be cancelled? A: No; it goes through the refund flow.

## Rejected alternatives

- Soft-deleting the order: removes it from the customer's history.

## Acceptance criteria

- AC1: Given a pending, unpaid order of mine, when I cancel it, then its status
  is `cancelled`.
- AC2: Given a paid order of mine, when I try to cancel it, then the request is
  refused with 403 and the order is unchanged.
- AC3 (INV-3): Given an order that belongs to another team, when I send a cancel
  request with its id, then the response is 404 and the order is unchanged.

## Implementation context

- AI_CONTEXT: controllers stay thin — the change lives in
  `App\Actions\Orders\CancelOrder`
- BUSINESS_CONTEXT INV-3: the cancel route loads the order through the
  `BelongsToTeam` scope and never calls `withoutGlobalScopes()`
