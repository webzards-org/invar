#!/bin/sh
# Seeded branches for checking the reviewer by hand (M1-20, plugin/NOTES.md §16).
#
#   eval/review-seeds.sh <empty-directory>
#
# Outside plugin/ on purpose: the plugin holds no script but its launcher.
#
# Builds a small Laravel-shaped repository there, with the Plan of
# contracts/formats/plan-header.md example 9.1 (SHOP-214, cancel an unpaid order)
# approved, and branches with a known defect each:
#
#   seed-invariant   loads the order with withoutGlobalScopes() — breaks the
#                    BelongsToTeam invariant (so criterion 3, another team's order
#                    is a 404, is not met either) — and never refuses a paid order
#                    (criterion 2 is not met).
#   seed-scope       implements the Plan, and also changes InvoiceController,
#                    which the Plan's Scope does not cover.
#   seed-unnamed-invariant  (M1-26) implements SHOP-215, a second approved
#                    Plan whose Implementation context names no invariant;
#                    its export loads orders with withoutGlobalScopes(), so it
#                    breaks the BelongsToTeam invariant the Plan never named.
#   seed-weak-test   (AT-14) implements SHOP-216, approved with acceptance tests,
#                    whose locked test for AC2 (a pending order is refused and
#                    unchanged) asserts only the 422 status code.
#   seed-factory     (AT-14) implements SHOP-216 with strong locked tests, and
#                    changes OrderFactory::cancelled(), which the locked test of
#                    AC1 uses, so that AC1 passes without the archive action.
#
# The SHOP-216 tests are locked by the real `invar acceptance lock`, with
# tests/run-acceptance.sh standing in for Pest: it reports every test red, as Pest
# would before the code exists.
#
# Then, in that directory: `git checkout <branch>`, open Claude Code, run
# `/invar:review SHOP-214` (`SHOP-215` on seed-unnamed-invariant, `SHOP-216` on
# seed-weak-test and seed-factory), and compare the report with the expectations
# in §16.
# Uses `invar` from PATH, or the binary named by $INVAR.

set -eu

INVAR="${INVAR:-invar}"
here="$(cd "$(dirname "$0")" && pwd)"
contract="$here/../contracts/formats/plan-header.md"
target="${1:?usage: review-seeds.sh <empty-directory>}"

mkdir -p "$target"
cd "$target"
if [ -n "$(ls -A .)" ]; then
  echo "review-seeds: $target is not empty" >&2
  exit 1
fi

git init --quiet --initial-branch=main
# `plan approve` records who approved; give the seed repository a name of its own.
git config user.name seed
git config user.email seed@example.com
w() { mkdir -p "$(dirname "$1")"; cat > "$1"; }

# `required: false`: SHOP-214 and SHOP-215 are approved without acceptance tests, as
# before AT-14; SHOP-216 asks for them with `--acceptance`.
w invar.yaml <<'EOF'
scenarios: false
review:
  offer: true
acceptance:
  required: false
  dir: tests/Acceptance
  run: ["sh", "tests/run-acceptance.sh", "{report}", "{files}"]
EOF

# Stands in for `vendor/bin/pest --log-junit {report} {files}`: there is no PHP
# here. Every `it('…')` of the given files is reported failing, which is what Pest
# says before the code exists.
w tests/run-acceptance.sh <<'EOF'
report="$1"
shift
{
  echo '<testsuites><testsuite name="Acceptance">'
  for file in "$@"; do
    sed -n "s/^it('\([^']*\)'.*/\1/p" "$file" | while IFS= read -r name; do
      printf '<testcase name="%s" file="%s"><failure type="AssertionError">red: the code does not exist yet</failure></testcase>\n' "$name" "$file"
    done
  done
  echo '</testsuite></testsuites>'
} > "$report"
exit 1
EOF

w database/factories/OrderFactory.php <<'EOF'
<?php

namespace Database\Factories;

use Illuminate\Database\Eloquent\Factories\Factory;

class OrderFactory extends Factory
{
    public function definition(): array
    {
        return [
            'number' => 'ORD-'.fake()->unique()->numerify('#####'),
            'status' => 'pending',
            'paid_at' => null,
            'archived_at' => null,
        ];
    }

    public function cancelled(): static
    {
        return $this->state(['status' => 'cancelled']);
    }
}
EOF

w BUSINESS_CONTEXT.md <<'EOF'
<!-- invar-context: v1 -->
# BUSINESS_CONTEXT.md

## Invariants

- Every query on team data is scoped to the current team by the `BelongsToTeam`
  global scope. Request handling never calls `withoutGlobalScopes()`.
- A paid order is never cancelled; it goes through the refund flow.
EOF

w AI_CONTEXT.md <<'EOF'
<!-- invar-context: v1 -->
# AI_CONTEXT.md

## Layers

- Controllers stay thin: logic lives in `App\Actions`, authorization in policies.
- Stock changes go through `StockLedger`, inside the action's transaction.
EOF

w app/Models/Order.php <<'EOF'
<?php

namespace App\Models;

use App\Models\Concerns\BelongsToTeam;
use Illuminate\Database\Eloquent\Model;

class Order extends Model
{
    use BelongsToTeam;

    public function isPaid(): bool
    {
        return $this->paid_at !== null;
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
        return $user->team_id === $order->team_id;
    }
}
EOF

w app/Http/Controllers/InvoiceController.php <<'EOF'
<?php

namespace App\Http\Controllers;

class InvoiceController extends Controller
{
    public function index()
    {
        return view('invoices.index');
    }
}
EOF

# The order history already lists every order with its status, so criterion 4
# (a cancelled order is listed as `cancelled`) is met by existing code.
w app/Http/Controllers/OrderHistoryController.php <<'EOF'
<?php

namespace App\Http\Controllers;

use App\Models\Order;

class OrderHistoryController extends Controller
{
    public function index()
    {
        return view('orders.history', [
            'orders' => Order::query()->where('user_id', auth()->id())->latest()->get(),
        ]);
    }
}
EOF

w resources/views/orders/history.blade.php <<'EOF'
@foreach ($orders as $order)
    <li>{{ $order->number }} — {{ $order->status }}</li>
@endforeach
EOF

w routes/web.php <<'EOF'
<?php

use Illuminate\Support\Facades\Route;

Route::get('/invoices', [\App\Http\Controllers\InvoiceController::class, 'index']);
Route::get('/orders/history', [\App\Http\Controllers\OrderHistoryController::class, 'index']);
EOF

add() { git add --all -- . ':(exclude).invar'; }
add
git -c user.name=seed -c user.email=seed@example.com commit --quiet -m "The application before SHOP-214"

# The approved Plan: example 9.1, approved here. Not committed: `.invar/` is not
# part of what is reviewed.
mkdir -p .invar/plans/SHOP-214-cancel-unpaid-order .invar/decisions
awk '/^### 9.1 /{found=1} found && /^```markdown$/{inside=1; next} inside && /^```$/{exit} inside{print}' \
  "$contract" > .invar/plans/SHOP-214-cancel-unpaid-order/spec.md
"$INVAR" plan approve SHOP-214 >/dev/null

# SHOP-215: a Plan whose Implementation context names no invariant — the planner
# missed the team scope. Valid, and approved, like any other.
w .invar/plans/SHOP-215-export-cancelled-orders/spec.md <<'EOF'
---
Ticket: SHOP-215
Module: orders
Status: draft
Approved-by:
Approved-at:
Approved-checksum:
---

## Ticket
SHOP-215 — As a support agent I want to download the cancelled orders as a CSV
file, so I can follow up with the customers.

## Intent
A CSV export of cancelled orders, one row per order.

## Scope
Backend: `App\Actions\Orders\ExportCancelledOrders`, its controller
`CancelledOrdersExportController`, and the route `GET /orders/cancelled/export` in
`routes/web.php`. Out of scope: other statuses, scheduling, e-mail.

## Open questions / answers
None were needed.

## Rejected alternatives
None considered.

## Acceptance criteria
- AC1: Given three cancelled orders and one pending order, when I export, then the
  CSV has three rows with the order number, status and date, and the pending order
  is not in it.
- AC2: Given no cancelled orders, when I export, then the CSV has only its header
  row.

## Implementation context
- AI_CONTEXT: the logic lives in `App\Actions\Orders\ExportCancelledOrders`; the
  controller only calls the action and streams the result.
EOF
"$INVAR" plan approve SHOP-215 >/dev/null

# SHOP-216: archiving a cancelled order, approved with acceptance tests.
w .invar/plans/SHOP-216-archive-cancelled-order/spec.md <<'EOF'
---
Ticket: SHOP-216
Module: orders
Status: draft
Approved-by:
Approved-at:
Approved-checksum:
---

## Ticket
SHOP-216 — As a customer I want to archive a cancelled order, so my order history
shows only what is still going on.

## Intent
A cancelled order can be archived by its owner; archived orders leave the order
history. Only cancelled orders can be archived.

## Scope
Backend: `App\Actions\Orders\ArchiveOrder`, `OrderPolicy::archive`, the route
`POST /orders/{order}/archive` and `OrderArchiveController`, and the history query in
`OrderHistoryController`. Tests under `tests/Acceptance/`. Out of scope: unarchiving,
archiving by staff.

## Open questions / answers
None were needed.

## Rejected alternatives
None considered.

## Acceptance criteria
- AC1: Given a cancelled order of mine, when I archive it, then it is archived and
  my order history no longer lists it.
- AC2: Given a pending order of mine, when I try to archive it, then the request is
  refused with 422 and the order is unchanged: still `pending`, and not archived.

## Implementation context
- AI_CONTEXT: the logic lives in `App\Actions\Orders\ArchiveOrder`; the controller
  only authorizes and calls it.
- BUSINESS_CONTEXT invariant: every query on team data is scoped to the current
  team by the `BelongsToTeam` global scope.
EOF
"$INVAR" plan approve SHOP-216 --acceptance >/dev/null

# --- seed-invariant ---------------------------------------------------------------
git checkout --quiet -b seed-invariant
w app/Actions/Orders/CancelOrder.php <<'EOF'
<?php

namespace App\Actions\Orders;

use App\Models\Order;
use App\Support\StockLedger;
use Illuminate\Support\Facades\DB;

class CancelOrder
{
    public function __construct(private StockLedger $stock) {}

    public function handle(int $orderId): Order
    {
        return DB::transaction(function () use ($orderId) {
            $order = Order::withoutGlobalScopes()->findOrFail($orderId);
            $order->status = 'cancelled';
            $order->save();
            $this->stock->release($order);

            return $order;
        });
    }
}
EOF
w app/Http/Controllers/OrderController.php <<'EOF'
<?php

namespace App\Http\Controllers;

use App\Actions\Orders\CancelOrder;

class OrderController extends Controller
{
    public function cancel(int $order, CancelOrder $cancel)
    {
        $cancel->handle($order);

        return back();
    }
}
EOF
printf "Route::post('/orders/{order}/cancel', [\\\\App\\\\Http\\\\Controllers\\\\OrderController::class, 'cancel']);\n" >> routes/web.php
add
git -c user.name=seed -c user.email=seed@example.com commit --quiet -m "SHOP-214: cancel an order"

# --- seed-scope ------------------------------------------------------------------
git checkout --quiet main
git checkout --quiet -b seed-scope
w app/Actions/Orders/CancelOrder.php <<'EOF'
<?php

namespace App\Actions\Orders;

use App\Models\Order;
use App\Support\StockLedger;
use Illuminate\Support\Facades\DB;

class CancelOrder
{
    public function __construct(private StockLedger $stock) {}

    public function handle(Order $order): Order
    {
        return DB::transaction(function () use ($order) {
            $order->status = 'cancelled';
            $order->save();
            $this->stock->release($order);

            return $order;
        });
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
        return $user->team_id === $order->team_id;
    }

    public function cancel(User $user, Order $order): bool
    {
        return $user->id === $order->user_id && ! $order->isPaid();
    }
}
EOF
w app/Http/Controllers/OrderController.php <<'EOF'
<?php

namespace App\Http\Controllers;

use App\Actions\Orders\CancelOrder;
use App\Models\Order;

class OrderController extends Controller
{
    public function cancel(Order $order, CancelOrder $cancel)
    {
        $this->authorize('cancel', $order);
        $cancel->handle($order);

        return back();
    }
}
EOF
w app/Http/Controllers/InvoiceController.php <<'EOF'
<?php

namespace App\Http\Controllers;

class InvoiceController extends Controller
{
    public function index()
    {
        return view('invoices.index', ['perPage' => 50]);
    }
}
EOF
printf "Route::post('/orders/{order}/cancel', [\\\\App\\\\Http\\\\Controllers\\\\OrderController::class, 'cancel']);\n" >> routes/web.php
add
git -c user.name=seed -c user.email=seed@example.com commit --quiet -m "SHOP-214: cancel an order, and invoice paging"

# --- seed-unnamed-invariant (M1-26) ------------------------------------------------
git checkout --quiet main
git checkout --quiet -b seed-unnamed-invariant
w app/Actions/Orders/ExportCancelledOrders.php <<'EOF'
<?php

namespace App\Actions\Orders;

use App\Models\Order;

class ExportCancelledOrders
{
    public function handle(): string
    {
        $rows = ["number,status,date"];
        $orders = Order::withoutGlobalScopes()
            ->where('status', 'cancelled')
            ->orderBy('created_at')
            ->get();
        foreach ($orders as $order) {
            $rows[] = "{$order->number},{$order->status},{$order->created_at->toDateString()}";
        }

        return implode("\n", $rows)."\n";
    }
}
EOF
w app/Http/Controllers/CancelledOrdersExportController.php <<'EOF'
<?php

namespace App\Http\Controllers;

use App\Actions\Orders\ExportCancelledOrders;

class CancelledOrdersExportController extends Controller
{
    public function __invoke(ExportCancelledOrders $export)
    {
        return response($export->handle(), 200, ['Content-Type' => 'text/csv']);
    }
}
EOF
printf "Route::get('/orders/cancelled/export', \\\\App\\\\Http\\\\Controllers\\\\CancelledOrdersExportController::class);\n" >> routes/web.php
add
git -c user.name=seed -c user.email=seed@example.com commit --quiet -m "SHOP-215: export cancelled orders"

# --- SHOP-216: the locked tests, and the archive itself ------------------------------

# The test-writer's file for SHOP-216, locked and committed. The first argument is
# what the AC2 test checks after its request: where the two seeds differ.
shop216_tests() {
  w tests/Acceptance/Shop216ArchiveOrderTest.php <<'EOF'
<?php
// invar-lock: SHOP-216

use App\Models\Order;
use App\Models\User;

it('AC1: archives a cancelled order and drops it from the history', function () {
    $user = User::factory()->create();
    $order = Order::factory()->for($user)->cancelled()->create();

    $this->actingAs($user)->post("/orders/{$order->id}/archive")->assertRedirect();

    expect($order->fresh()->archived_at)->not->toBeNull();
    $this->actingAs($user)->get('/orders/history')->assertDontSee($order->number);
});

it('AC2: refuses to archive a pending order', function () {
    $user = User::factory()->create();
    $order = Order::factory()->for($user)->create(['status' => 'pending']);

    $this->actingAs($user)->postJson("/orders/{$order->id}/archive")->assertStatus(422);
EOF
  printf '%s});\n' "$1" >> tests/Acceptance/Shop216ArchiveOrderTest.php
  "$INVAR" acceptance lock SHOP-216 >/dev/null
  add
  git -c user.name=seed -c user.email=seed@example.com commit --quiet -m "SHOP-216: acceptance tests"
}

# The archive, done as the Plan says: these seeds are about the tests.
shop216_archive() {
  w app/Actions/Orders/ArchiveOrder.php <<'EOF'
<?php

namespace App\Actions\Orders;

use App\Models\Order;
use Illuminate\Validation\ValidationException;

class ArchiveOrder
{
    public function handle(Order $order): Order
    {
        if ($order->status !== 'cancelled') {
            throw ValidationException::withMessages(['order' => 'Only a cancelled order can be archived.']);
        }
        $order->archived_at = now();
        $order->save();

        return $order;
    }
}
EOF
  w app/Http/Controllers/OrderArchiveController.php <<'EOF'
<?php

namespace App\Http\Controllers;

use App\Actions\Orders\ArchiveOrder;
use App\Models\Order;

class OrderArchiveController extends Controller
{
    public function __invoke(Order $order, ArchiveOrder $archive)
    {
        $this->authorize('archive', $order);
        $archive->handle($order);

        return back();
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
        return $user->team_id === $order->team_id;
    }

    public function archive(User $user, Order $order): bool
    {
        return $user->id === $order->user_id;
    }
}
EOF
  w app/Http/Controllers/OrderHistoryController.php <<'EOF'
<?php

namespace App\Http\Controllers;

use App\Models\Order;

class OrderHistoryController extends Controller
{
    public function index()
    {
        return view('orders.history', [
            'orders' => Order::query()
                ->where('user_id', auth()->id())
                ->whereNull('archived_at')
                ->latest()
                ->get(),
        ]);
    }
}
EOF
  printf "Route::post('/orders/{order}/archive', \\\\App\\\\Http\\\\Controllers\\\\OrderArchiveController::class);\n" >> routes/web.php
}

# --- seed-weak-test (AT-14) -------------------------------------------------------
git checkout --quiet main
git checkout --quiet -b seed-weak-test
shop216_tests ""
shop216_archive
add
git -c user.name=seed -c user.email=seed@example.com commit --quiet -m "SHOP-216: archive a cancelled order"

# --- seed-factory (AT-14) ---------------------------------------------------------
git checkout --quiet main
git checkout --quiet -b seed-factory
shop216_tests "
    expect(\$order->fresh()->status)->toBe('pending');
    expect(\$order->fresh()->archived_at)->toBeNull();
"
shop216_archive
# The coder "fixes" a red AC1 in the factory: every cancelled order is born archived.
w database/factories/OrderFactory.php <<'EOF'
<?php

namespace Database\Factories;

use Illuminate\Database\Eloquent\Factories\Factory;

class OrderFactory extends Factory
{
    public function definition(): array
    {
        return [
            'number' => 'ORD-'.fake()->unique()->numerify('#####'),
            'status' => 'pending',
            'paid_at' => null,
            'archived_at' => null,
        ];
    }

    public function cancelled(): static
    {
        return $this->state(['status' => 'cancelled', 'archived_at' => now()]);
    }
}
EOF
add
git -c user.name=seed -c user.email=seed@example.com commit --quiet -m "SHOP-216: archive a cancelled order"

git checkout --quiet main
echo "review-seeds: ready in $target — branches seed-invariant, seed-scope, seed-unnamed-invariant, seed-weak-test and seed-factory"
