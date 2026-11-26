#!/bin/sh
# A seeded draft Plan for checking the plan auditor by hand (#167, plugin/NOTES.md §23).
#
#   eval/plan-audit-seeds.sh <empty-directory>
#
# Outside plugin/ on purpose: the plugin holds no script but its launcher.
#
# Builds a small Laravel-shaped repository there, on a branch `seed-plan-audit`, with
# a draft Plan for SHOP-214 (customers cancel their own unpaid orders) that passes
# `invar plan validate` and has four known defects:
#
#   a wrong column     Implementation context says the time is recorded in
#                      `canceled_at`; the migration and the model say `cancelled_at`.
#                      Expected: an `edit` of Implementation context, cited at the
#                      migration or the model.
#   a misspelled path  Scope lists app/Http/Controllers/OrdersController.php; the
#                      controller is OrderController.php. Expected: an `edit` of
#                      Scope, cited at the controller.
#   an impossible      AC3 says a cancelled order's status is `voided`; the status
#   criterion          column is an enum of pending, paid and cancelled, and the
#                      migration is outside Scope. Expected: `criterion-cannot-pass`,
#                      blocking, on Acceptance criteria. AC3 itself is unchanged.
#   an unnamed         INV-3 (cancelling releases reserved stock through StockLedger)
#   invariant          is plainly touched by cancelling, and Implementation context
#                      names only INV-1 and INV-2. Expected: `invariant-not-named`,
#                      blocking, naming INV-3.
#
# So the report ends `plan audit SHOP-214: NOT READY — 2 blocking finding(s)` or more,
# and refuses nothing: the auditor changed only the two sections.
#
# Then, in that directory: open Claude Code at its root, run
# `/invar:plan-audit SHOP-214`, and compare the report with the expectations above.
# Uses `invar` from PATH, or the binary named by $INVAR.

set -eu

INVAR="${INVAR:-invar}"
target="${1:?usage: plan-audit-seeds.sh <empty-directory>}"

mkdir -p "$target"
cd "$target"
if [ -n "$(ls -A .)" ]; then
  echo "plan-audit-seeds: $target is not empty" >&2
  exit 1
fi

git init --quiet --initial-branch=main
git config user.name seed
git config user.email seed@example.com
w() { mkdir -p "$(dirname "$1")"; cat > "$1"; }

w invar.yaml <<'EOF'
scenarios: false
EOF

# What `invar init` ignores: the audit's baseline, findings and report go there.
w .gitignore <<'EOF'
.invar/out/
EOF

w BUSINESS_CONTEXT.md <<'EOF'
<!-- invar-context: v1 -->
# BUSINESS_CONTEXT.md

## Invariants

- INV-1: Every query on team data is scoped to the current team by the
  `BelongsToTeam` global scope. Request handling never calls `withoutGlobalScopes()`.
- INV-2: A paid order is never cancelled; it goes through the refund flow.
- INV-3: Cancelling an order releases the stock reserved for it, through
  `StockLedger::release()`, in the same transaction.
EOF

w AI_CONTEXT.md <<'EOF'
<!-- invar-context: v1 -->
# AI_CONTEXT.md

## Layers

- Controllers stay thin: logic lives in `App\Actions`, authorization in policies.
- Stock changes go through `StockLedger`, inside the action's transaction.
EOF

w database/migrations/2026_01_10_000000_create_orders_table.php <<'EOF'
<?php

use Illuminate\Database\Migrations\Migration;
use Illuminate\Database\Schema\Blueprint;
use Illuminate\Support\Facades\Schema;

return new class extends Migration
{
    public function up(): void
    {
        Schema::create('orders', function (Blueprint $table) {
            $table->id();
            $table->foreignId('team_id')->constrained();
            $table->foreignId('customer_id')->constrained('users');
            $table->string('number')->unique();
            $table->enum('status', ['pending', 'paid', 'cancelled'])->default('pending');
            $table->timestamp('paid_at')->nullable();
            $table->timestamp('cancelled_at')->nullable();
            $table->timestamps();
        });
    }
};
EOF

w app/Models/Order.php <<'EOF'
<?php

namespace App\Models;

use App\Models\Concerns\BelongsToTeam;
use Illuminate\Database\Eloquent\Model;

class Order extends Model
{
    use BelongsToTeam;

    public const STATUSES = ['pending', 'paid', 'cancelled'];

    protected $casts = [
        'paid_at' => 'datetime',
        'cancelled_at' => 'datetime',
    ];

    public function customer()
    {
        return $this->belongsTo(User::class, 'customer_id');
    }

    public function isPaid(): bool
    {
        return $this->paid_at !== null;
    }
}
EOF

w app/Models/Concerns/BelongsToTeam.php <<'EOF'
<?php

namespace App\Models\Concerns;

use Illuminate\Database\Eloquent\Builder;

trait BelongsToTeam
{
    protected static function bootBelongsToTeam(): void
    {
        static::addGlobalScope('team', function (Builder $query) {
            $query->where('team_id', auth()->user()?->current_team_id);
        });
    }
}
EOF

w app/Services/StockLedger.php <<'EOF'
<?php

namespace App\Services;

use App\Models\Order;

class StockLedger
{
    public function reserve(Order $order): void
    {
        // Moves each line's quantity from available to reserved.
    }

    public function release(Order $order): void
    {
        // Moves each line's reserved quantity back to available.
    }
}
EOF

w app/Http/Controllers/OrderController.php <<'EOF'
<?php

namespace App\Http\Controllers;

use App\Models\Order;

class OrderController extends Controller
{
    public function show(Order $order)
    {
        $this->authorize('view', $order);

        return view('orders.show', ['order' => $order]);
    }
}
EOF

w app/Policies/OrderPolicy.php <<'EOF'
<?php

namespace App\Policies;

use App\Models\Order;
use App\Models\User;

class OrderPolicy
{
    public function view(User $user, Order $order): bool
    {
        return $order->customer_id === $user->id;
    }
}
EOF

w routes/web.php <<'EOF'
<?php

use App\Http\Controllers\OrderController;
use Illuminate\Support\Facades\Route;

Route::middleware('auth')->group(function () {
    Route::get('/orders/{order}', [OrderController::class, 'show'])->name('orders.show');
});
EOF

git add -A
git commit --quiet -m "chore: the shop before SHOP-214"
git checkout --quiet -b seed-plan-audit

w .invar/plans/SHOP-214-cancel-unpaid-order/spec.md <<'EOF'
---
Ticket: SHOP-214
Module: orders
Status: draft
Approved-by:
Approved-at:
Approved-checksum:
---

## Ticket
SHOP-214 — Customers can cancel their own orders while they are unpaid. A paid
order cannot be cancelled this way. A cancelled order shows when it was cancelled.

## Intent
A customer cancels an order of theirs that is not paid yet, from the order's page.

## Scope
- app/Http/Controllers/OrdersController.php
- app/Actions/CancelOrder.php
- app/Policies/OrderPolicy.php
- routes/web.php
- tests/Feature/CancelOrderTest.php

## Open questions / answers
- Q: Can a customer cancel an order of another customer in the same team?
  A: No; only their own.

## Rejected alternatives
- Cancelling from the order list as well: the ticket names the order's page only.

## Acceptance criteria

- AC1 (INV-1): Given an unpaid order of the current customer, when they cancel it,
  then they are sent back to the order's page with a confirmation.
- AC2 (INV-2): Given a paid order, when its customer tries to cancel it, then the
  request is refused and the order is unchanged.
- AC3: Given an unpaid order, when its customer cancels it, then its status is
  `voided` and the time it was cancelled is recorded.

## Implementation context
- AI_CONTEXT: controllers stay thin; the cancellation is `App\Actions\CancelOrder`,
  authorized by `OrderPolicy::cancel`.
- BUSINESS_CONTEXT INV-1: the order is loaded through the `BelongsToTeam` scope.
- BUSINESS_CONTEXT INV-2: a paid order is refused before anything changes.
- The order records the time of cancellation in `canceled_at`.
EOF

"$INVAR" plan validate SHOP-214 >/dev/null
git add -A
git commit --quiet -m "docs: the draft Plan of SHOP-214"
echo "seeded: $target, branch seed-plan-audit; run /invar:plan-audit SHOP-214 there"
