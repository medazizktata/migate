# Dogfood 2 and the PostgreSQL drive: 2026-11-19 to 20

Run 3 of migate's end-to-end runs; migate up to `9c6590e`. How runs are judged, and the others: `README.md`.

Two blind agents in parallel: one adopting migate on the origin project, one on Django, Rails, Prisma, Drizzle and a
schema-heavy sample app on PostgreSQL 18. No ground truth beyond their own judgement; their reports were not kept
apart from the tasks they produced: TASK-55 to TASK-65 (kinds the PostgreSQL reader cannot model, the server's
lifecycle, values lost without rows lost, calibration on real suites, coexisting with a project's own migration gate,
side effects under the home folder).
