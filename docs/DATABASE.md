# Database Design & Development Standards (Core Generic)

## Table of Contents

1. Design Standards (Modeling, Schema, Constraints, Indexes, Migrations, etc.)
2. Development Standards (Queries, Transactions, Performance, Data Access, etc.)
3. Security & Governance Standards (Access Control, Privacy, Audit, Backup, etc.)

------

## Part 1: Design Standards (Generic Principles)

### 1. Existing Conventions & Architecture

- Understand the project’s existing database type, schema organization, naming rules, migration strategy, data access pattern, and operational constraints.
- New database changes must follow existing conventions. Do not introduce parallel modeling styles or storage patterns without approval.
- Keep logical data models, persistence models, and external API contracts clearly separated when the project separates them.
- Prefer simple, explicit, and maintainable designs over clever or overly abstract schemas.

### 2. Data Modeling

- Model data around real business concepts, ownership, lifecycle, and access patterns.
- Define clear entity boundaries, relationships, cardinality, and invariants.
- Avoid storing the same source-of-truth data in multiple places unless denormalization is explicitly justified.
- Consider read/write frequency, data volume, retention, archival, and future evolution.
- Do not design tables, collections, or documents only for the current UI shape.

### 3. Schema Design

- Use clear, consistent, and meaningful names for tables, collections, columns, fields, indexes, and constraints.
- Define appropriate data types, lengths, precision, defaults, and nullability.
- Avoid ambiguous fields, overloaded columns, and unstructured blobs unless explicitly justified.
- Prefer explicit status, lifecycle, and timestamp fields where they reflect real business state.
- Keep schema changes compatible with existing data and running application versions whenever possible.

### 4. Keys, Identity & Relationships

- Define stable primary identifiers for persisted entities.
- Use foreign keys, references, or equivalent relationship constraints when supported and appropriate.
- Avoid relying on mutable business fields as primary identifiers.
- Consider uniqueness, natural keys, surrogate keys, tenant scope, and cross-system identifiers carefully.
- Relationship rules must be explicit for creation, update, deletion, archival, and restoration.

### 5. Constraints & Data Integrity

- Enforce critical invariants at the database level when possible.
- Use constraints for uniqueness, required fields, valid ranges, referential integrity, and state consistency.
- Do not rely only on application-level validation for data integrity that must always hold.
- Ensure constraints align with business rules and expected concurrency behavior.
- Handle legacy, partial, or invalid data before adding stricter constraints.

### 6. Index Design

- Indexes must be based on real query patterns, filtering, sorting, joining, uniqueness, and access frequency.
- Avoid adding indexes blindly; every index has write, storage, and maintenance cost.
- Review composite index order, selectivity, cardinality, and covering behavior where applicable.
- Remove unused or duplicate indexes when safe.
- Consider platform-specific limits and query planner behavior without relying on undocumented behavior.

### 7. Normalization & Denormalization

- Prefer normalized models for correctness, maintainability, and clear ownership.
- Use denormalization only when justified by performance, availability, reporting, or platform constraints.
- Denormalized data must have a clear source of truth, synchronization strategy, and repair path.
- Avoid hidden duplication that can silently diverge.
- Document derived, cached, aggregated, or materialized data.

### 8. Multi-Tenancy & Data Isolation

- Clearly define tenant, organization, user, region, or environment boundaries.
- All tenant-scoped data must include reliable isolation keys or equivalent partitioning mechanisms.
- Queries, indexes, constraints, and migrations must preserve data isolation.
- Do not rely only on application filters for tenant isolation when stronger guarantees are required.
- Consider backup, restore, export, deletion, and audit behavior per isolation boundary.

### 9. Time, Ordering & Lifecycle

- Define consistent rules for timestamps, time zones, ordering, expiration, soft deletion, archival, and restoration.
- Store time in a consistent canonical form and localize only at the presentation boundary.
- Avoid relying on client-provided time for authoritative decisions unless explicitly required.
- Lifecycle states should be explicit and valid transitions should be controlled.
- Historical records must remain interpretable after schema and business rule changes.

### 10. Schema Evolution

- Design schemas to evolve safely over time.
- Avoid destructive changes unless migration, compatibility, and rollback are planned.
- Prefer additive changes before switching readers and writers.
- Support rolling deployments where old and new application versions may run simultaneously.
- Maintain compatibility with existing jobs, consumers, reports, exports, and integrations.

------

## Part 2: Development Standards (Generic Principles)

### 1. Migration Discipline

- All schema changes must be represented through the project’s migration mechanism.
- Migrations must be deterministic, reviewable, repeatable, and safe to run in target environments.
- Avoid long locks, full-table rewrites, and blocking operations on large datasets unless explicitly planned.
- Separate schema changes, data backfills, and application behavior changes when needed.
- Every high-risk migration must include rollback or recovery considerations.

### 2. Data Backfills & Repairs

- Backfills must be idempotent, resumable, observable, and safe under partial failure.
- Process large datasets in batches with controlled resource usage.
- Avoid disrupting production traffic, replication, backups, or maintenance windows.
- Validate results before and after backfills or repairs.
- Keep repair scripts traceable and remove or archive one-off scripts according to project policy.

### 3. Query Design

- Write queries according to existing data access patterns and abstractions.
- Queries must be bounded, intentional, and aligned with indexes where performance matters.
- Avoid N+1 access patterns, unbounded scans, unnecessary joins, excessive projections, and large result sets.
- Use pagination, filtering, projection, batching, streaming, or aggregation where appropriate.
- Validate generated or dynamic queries to avoid unsafe or inefficient behavior.

### 4. Transactions

- Use transactions for operations that must be atomic.
- Keep transaction scope small and deterministic.
- Avoid user interaction, remote calls, long computation, or external side effects inside transactions.
- Choose isolation levels intentionally according to consistency requirements.
- Handle deadlocks, retries, timeouts, and partial failures safely.

### 5. Concurrency & Consistency

- Protect critical writes against lost updates, duplicate writes, race conditions, and inconsistent reads.
- Use unique constraints, locks, version fields, compare-and-set, idempotency keys, or equivalent mechanisms where appropriate.
- Do not rely only on pre-checks when concurrent writes can violate invariants.
- Design retry behavior so it does not create duplicate or inconsistent data.
- Document consistency expectations for distributed, cached, replicated, or eventually consistent data.

### 6. Data Access Boundaries

- Use existing repositories, query builders, ORMs, data mappers, or database clients.
- Do not scatter raw database access across unrelated business code.
- Keep database-specific syntax and vendor-specific behavior isolated when portability or testability matters.
- Do not bypass existing data access layers, authorization filters, auditing, or soft-delete behavior.
- Ensure read and write paths apply the same business and security constraints.

### 7. Performance & Scalability

- Design for expected and future data volume, query frequency, write rate, and growth patterns.
- Monitor and optimize slow queries, hot indexes, lock contention, connection usage, and storage growth.
- Use batching, indexing, partitioning, caching, read replicas, or materialized views only when justified.
- Avoid premature optimization that complicates correctness or maintainability.
- Performance-sensitive changes must be verified with realistic data volume or query plans.

### 8. Pagination, Sorting & Filtering

- Pagination must be stable, deterministic, and safe for large datasets.
- Sorting must be explicit and backed by appropriate indexes when needed.
- Filtering behavior must be predictable and validated.
- Avoid exposing arbitrary unbounded filtering or sorting that can cause expensive queries.
- Cursor-based or keyset pagination should be considered for large or frequently changing datasets.

### 9. Data Deletion, Retention & Archival

- Define clear behavior for hard deletion, soft deletion, archival, restoration, and retention.
- Deletion must respect ownership, legal, compliance, audit, and dependency requirements.
- Soft-deleted or archived data must not appear in active queries unless explicitly intended.
- Cascading deletion rules must be explicit and safe.
- Retention and cleanup jobs must be observable, reversible where possible, and safe under retries.

### 10. Reporting, Analytics & Derived Data

- Separate transactional data paths from reporting or analytical workloads when necessary.
- Derived, aggregated, indexed, or materialized data must have clear freshness and rebuild rules.
- Avoid adding reporting requirements that degrade core transactional performance.
- Data exported for analytics must respect privacy, authorization, and retention rules.
- Metrics and reports should define source of truth and calculation semantics.

------

## Part 3: Security & Governance Standards (Generic Principles)

### 1. Access Control

- Database access must follow least privilege.
- Application, migration, reporting, admin, and background job credentials should have appropriate scopes.
- Do not use privileged accounts for normal application traffic.
- Direct production data access must be controlled, logged, and auditable.
- Authorization-sensitive queries must enforce tenant, ownership, and resource boundaries.

### 2. Sensitive Data Protection

- Classify sensitive fields and apply appropriate protection.
- Do not store secrets, credentials, tokens, private keys, or highly sensitive data unless explicitly required.
- Use approved encryption, hashing, masking, or tokenization mechanisms where appropriate.
- Passwords and security credentials must never be stored in reversible or weak formats.
- Sensitive data must not appear in query logs, error messages, exports, backups, or test fixtures without controls.

### 3. Privacy & Data Minimization

- Store only data that is required for the product, business, compliance, or operational purpose.
- Minimize personal, tenant-sensitive, and business-sensitive fields.
- Define retention, deletion, anonymization, and export behavior where applicable.
- Avoid exposing internal identifiers or unrelated fields to downstream consumers.
- Respect consent, regional, and regulatory requirements when applicable.

### 4. Auditability

- Security-sensitive and business-critical changes must be traceable.
- Audit records should capture actor, action, target, time, source, and relevant context.
- Audit logs must be tamper-resistant according to project requirements.
- Do not store excessive sensitive payloads in audit records.
- Administrative and data repair operations must be auditable.

### 5. Backup, Restore & Disaster Recovery

- Data changes must consider backup, restore, replication, and disaster recovery behavior.
- Backups must protect sensitive data and follow retention rules.
- Restore procedures must be tested or documented according to project requirements.
- Schema migrations must not break backup or restore assumptions.
- Critical data must have a defined recovery objective and recovery path.

### 6. Environment & Test Data

- Keep development, test, staging, and production data isolated.
- Do not use production data in lower environments unless it is approved, minimized, masked, and controlled.
- Test data must not contain real secrets or uncontrolled personal data.
- Seed data should be deterministic, minimal, and safe.
- Environment-specific data handling must not leak into production behavior.

### 7. Observability & Operations

- Database errors, slow queries, migration status, replication health, connection pool usage, storage growth, and backup status should be observable.
- Alerts should focus on user impact, data integrity, availability, and capacity risk.
- Operational dashboards and logs must not expose sensitive data.
- Database-related incidents should leave enough context for diagnosis without compromising privacy or security.

------

## Enforcement

- All schema changes must go through the project’s approved migration and review process.
- Database changes must include tests, validation queries, or a clear verification checklist.
- High-risk migrations, destructive changes, backfills, and data repairs must include deployment, rollback, and recovery considerations.
- New indexes, constraints, denormalized fields, and derived data must be justified by access patterns or integrity requirements.
- Changes affecting sensitive data, retention, access control, or audit behavior must receive explicit review.