#!/usr/bin/env bash
# Builds one head branch per fixture on top of the `base` tag of the Django sample. Re-runnable: each branch is reset to base.
set -euo pipefail
S=${SPIKE_DIR:?set SPIKE_DIR to the spike work folder}
P=$S/.venv/bin/python
cd "$S/django-sample"
g() { git -c user.name=spike -c user.email=spike@localhost "$@"; }
mk() { "$P" manage.py makemigrations shop -n "$1" >/dev/null; g add -A; g commit -qm "$2"; }
# replace the first occurrence of $1 by $2 in shop/models.py, failing loudly when $1 is absent
py() { "$P" -c 'import sys;p="shop/models.py";s=open(p).read();a,b=sys.argv[1],sys.argv[2];assert a in s,a;open(p,"w").write(s.replace(a,b,1))' "$1" "$2"; }
LC='    legacy_code = models.CharField(max_length=20, blank=True, default="")

    def status_label'

g checkout -qfB f1-drop-used base
py "$LC" '
    def status_label'
mk drop_legacy_code "head: drop Order.legacy_code (the base code still declares it)"

g checkout -qfB f2-notnull-nodefault base
py "$LC" '    legacy_code = models.CharField(max_length=20, blank=True, default="")
    priority = models.IntegerField(default=0)

    def status_label'
sed -i '' 's/amount_cents=4200, status=3)/amount_cents=4200, status=3, priority=5)/' shop/management/commands/seed.py
mk order_priority "head: add Order.priority NOT NULL (Django keeps no database default)"

g checkout -qfB f3-union-add base
py "$LC" '    legacy_code = models.CharField(max_length=20, blank=True, default="")
    note = models.TextField(null=True, blank=True)

    def status_label'
mk order_note "head: add a nullable Order.note (live orders only, not the archive)"

g checkout -qfB f4-type-change base
py 'STATUS_LABELS = {1: "new", 2: "paid", 3: "shipped"}' 'STATUS_LABELS = {1: "new", 2: "paid", 3: "shipped"}  # kept for the data migration'
py '    status = models.IntegerField(default=1)
    legacy_code = models.CharField(max_length=20, blank=True, default="")

    def status_label(self):
        return STATUS_LABELS[self.status]' '    status = models.CharField(max_length=20, default="new")
    legacy_code = models.CharField(max_length=20, blank=True, default="")

    def status_label(self):
        return self.status'
"$P" manage.py makemigrations shop -n order_status_text >/dev/null
cat > shop/migrations/0004_order_status_words.py <<'EOF'
from django.db import migrations

def to_words(apps, schema_editor):
    Order = apps.get_model("shop", "Order")
    for n, w in {"1": "new", "2": "paid", "3": "shipped"}.items():
        Order.objects.filter(status=n).update(status=w)

class Migration(migrations.Migration):
    dependencies = [("shop", "0003_order_status_text")]
    operations = [migrations.RunPython(to_words, migrations.RunPython.noop)]
EOF
sed -i '' 's/amount_cents=4200, status=3)/amount_cents=4200, status="shipped")/' shop/management/commands/seed.py
g add -A; g commit -qm "head: Order.status becomes text ('shipped'), existing rows converted"

g checkout -qfB f5-safe-add base
py '    name = models.CharField(max_length=100)' '    name = models.CharField(max_length=100)
    gift_message = models.TextField(null=True, blank=True)'
mk customer_gift_message "head: add a nullable Customer.gift_message"

g checkout -qfB f6-safe-contract base
cat > shop/migrations/0003_drop_old_note_column.py <<'EOF'
from django.db import migrations

# Expand/contract, step 2: old_note left the code in 0002 (a release ago); now drop the column itself.
class Migration(migrations.Migration):
    dependencies = [("shop", "0002_stop_declaring_old_note")]
    operations = [
        migrations.SeparateDatabaseAndState(
            database_operations=[migrations.RunSQL("ALTER TABLE shop_customer DROP COLUMN old_note")]
        ),
    ]
EOF
g add -A; g commit -qm "head: drop the old_note column the base code no longer declares"

g checkout -qf main
for b in f1-drop-used f2-notnull-nodefault f3-union-add f4-type-change f5-safe-add f6-safe-contract; do
  echo "== $b"; git diff --stat base "$b" | tail -n +1
done
