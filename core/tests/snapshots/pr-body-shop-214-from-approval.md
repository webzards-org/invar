# SHOP-214

Approved by dev on 2026-10-05.
Signed by dev@example.com (SHA256:<fingerprint>), approved at <commit> (chore: approve SHOP-214), verified against main:.invar/approvers.

**No acceptance tests:** this ticket was approved without them.

## Intent

Customers cancel their own unpaid orders.

## Scope

`orders` only.

## Open questions / answers

None were needed.

## Rejected alternatives

None considered.

## Acceptance criteria

- AC1: Given an unpaid order, when its customer cancels it, then it is cancelled.

## Implementation context

- AI_CONTEXT: controllers stay thin
