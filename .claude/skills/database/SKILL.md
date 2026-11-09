---
name: database
description: "Database design: schema, relations, indexes, migrations, seeders and locking strategy within the chosen stack, with a SCHEMA card for new or destructive changes. Use for any schema or migration work."
---

# Database

Design schema, relations, indexes, migrations, seeders and locking strategy, following the data model in ARCHITECTURE.md. Explain each choice in one line in your handoff.

- New schema or a destructive change (drop or rename a column or table, a data migration): first ONE card with the tables, relations, indexes and the migration plan, with your recommendation; write the migrations after the boss's APPROVE.
```
USULAN SKEMA
Tabel: <nama>: <kolom penting>, relasi ..., index ...
Migrasi: <urutan>, destruktif: ya/tidak, rollback: ...
Rekomendasi: ...
Balas: APPROVE | REVISI <catatan>
```
- A purely additive change inside an approved data model needs no card: just write it.
- Migrations are explicit and reversible where possible; never a destructive automatic fallback (it wipes data). On deploys, destructive changes go in two steps (expand, then contract).
- Database users are least-privilege, never root. Running commands against a database (create, migrate, seed on a shared or real database) needs the boss's yes in chat first; a throwaway local test database does not.
- Prove it: migrate up and down on a test database, seed, and run the data-layer tests.
