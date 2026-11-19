"""Rollback-compatibility probe, run from the BASE checkout: load the old version's models and read every row (written by the
head version's seed) through them, validating each with full_clean(). Team-agnostic: needs no test written by the team.
    cd <base checkout> && python readback_django.py <app_label>
"""
import os
import sys

import django

sys.path.insert(0, os.getcwd())
os.environ.setdefault("DJANGO_SETTINGS_MODULE", "mysite.settings")
django.setup()
from django.apps import apps  # noqa: E402

bad, rows = [], 0
for model in apps.get_app_config(sys.argv[1]).get_models():
    try:
        for obj in model.objects.all():
            rows += 1
            try:
                obj.full_clean()
                str(obj)
            except Exception as e:  # noqa: BLE001 - every failure is a finding
                bad.append(f"{model.__name__} pk={obj.pk}: {type(e).__name__}: {e}")
    except Exception as e:  # noqa: BLE001 - the query itself failed
        bad.append(f"{model.__name__} (query): {type(e).__name__}: {str(e).splitlines()[0]}")
print(f"read {rows} rows")
for b in bad:
    print("READBACK", b)
sys.exit(1 if bad else (2 if rows == 0 else 0))  # 2: nothing was read, so nothing was checked
