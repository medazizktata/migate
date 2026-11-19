#!/usr/bin/env bash
# One head branch per fixture on top of the `base` tag of the Rails sample. Re-runnable: each branch is reset to base.
set -euo pipefail
S=${SPIKE_DIR:?set SPIKE_DIR to the spike work folder}
cd "$S/rails-sample"
g() { git -c user.name=spike -c user.email=spike@localhost "$@"; }
mig() { printf 'class %s < ActiveRecord::Migration[8.1]\n%s\nend\n' "$2" "$3" > "db/migrate/20260201000001_$1.rb"; }
rep() { python3 -c 'import sys;p,a,b=sys.argv[1:];s=open(p).read();assert a in s,a;open(p,"w").write(s.replace(a,b,1))' "$@"; }

# R1 GitHub Aug 2025: drop a column "no longer in direct use"; the base code never names it, but Rails cached it at boot.
g checkout -qfB f1-drop-used base
mig drop_legacy_code DropLegacyCode '  def change = remove_column(:orders, :legacy_code, :string, null: false, default: "")'
g add -A; g commit -qm "head: drop orders.legacy_code (unused by the code, not in ignored_columns)"

# R2 new NOT NULL column without a default (the default is only used to backfill existing rows, then removed)
g checkout -qfB f2-notnull-nodefault base
mig order_priority OrderPriority '  def change
    add_column :orders, :priority, :integer, null: false, default: 0
    change_column_default :orders, :priority, from: 0, to: nil
  end'
rep lib/tasks/seed_mv.rake 'amount_cents: 4200, status: 3)' 'amount_cents: 4200, status: 3, priority: 5)'
g add -A; g commit -qm "head: add orders.priority NOT NULL without a default"

# R3 GitLab #5171 class: add a nullable column to BOTH tables (keeping the archive in step); safe for a schema diff.
g checkout -qfB f3-union-add base
mig add_note AddNote '  def change
    add_column :orders, :note, :text
    add_column :archived_orders, :note, :text
  end'
g add -A; g commit -qm "head: add a nullable note to orders and archived_orders"

# R4 CircleCI 2021 class: a type change; the head code writes words the base code cannot read back.
g checkout -qfB f4-type-change base
mig order_status_text OrderStatusText '  def up
    change_column_default :orders, :status, nil
    change_column :orders, :status, :string
    { "1" => "new", "2" => "paid", "3" => "shipped" }.each { |n, w| execute("UPDATE orders SET status = '"'"'#{w}'"'"' WHERE status = '"'"'#{n}'"'"'") }
    change_column_default :orders, :status, "new"
  end
  def down = raise(ActiveRecord::IrreversibleMigration)'
rep app/models/order.rb '  def status_label = STATUS_LABELS.fetch(status)' '  def status_label = status'
rep lib/tasks/seed_mv.rake 'amount_cents: 4200, status: 3)' 'amount_cents: 4200, status: "shipped")'
g add -A; g commit -qm "head: orders.status becomes text"

# R5 safe: a nullable column on customers
g checkout -qfB f5-safe-add base
mig customer_gift_message CustomerGiftMessage '  def change = add_column(:customers, :gift_message, :text)'
g add -A; g commit -qm "head: add a nullable customers.gift_message"

# R6 safe contract: the base code already lists old_note in ignored_columns; head drops the column
g checkout -qfB f6-safe-contract base
mig drop_old_note DropOldNote '  def change = remove_column(:customers, :old_note, :text)'
g add -A; g commit -qm "head: drop customers.old_note (ignored by the base code)"

g checkout -qf main
for b in f1-drop-used f2-notnull-nodefault f3-union-add f4-type-change f5-safe-add f6-safe-contract; do
  echo "== $b"; git diff --stat base "$b"
done
