# Backend Development Standards (Core Generic)

## Table of Contents

1. Development Standards (Architecture, APIs, Data, Async Jobs, Observability, etc.)
2. Security Standards (Generic Principles)
3. Dependency Management Standards (Generic Principles)

------

## Part 1: Development Standards (Generic Behavior Principles)

### 1. Project Structure & Existing Conventions

- Understand the project’s directory organization, module boundaries, naming rules, layering model, configuration style, dependency injection pattern, and testing strategy.
- New code must follow the existing structure. Do not introduce parallel architectures or arbitrarily move/refactor existing files.
- If structural changes are necessary, propose the plan and obtain confirmation first.
- Keep business logic, transport logic, persistence logic, and infrastructure concerns clearly separated.

### 2. API Design & Contracts

- Follow the project’s existing API style, naming conventions, response format, error format, pagination model, and versioning strategy.
- APIs must have clear input/output contracts and stable semantics.
- Do not introduce breaking changes unless explicitly required.
- Validate request parameters, path variables, query strings, headers, and request bodies.
- Return appropriate status codes or equivalent protocol-level results.
- Avoid leaking internal implementation details through API responses.
- For public or cross-team APIs, keep backward compatibility and document behavior changes.

### 3. Business Logic Boundaries

- Keep core business rules centralized and reusable.
- Avoid duplicating business logic across controllers, handlers, jobs, scripts, or scheduled tasks.
- Do not place complex business rules directly inside transport-layer code.
- Use clear domain/service boundaries so behavior remains testable and maintainable.
- Preserve existing behavior unless the task explicitly requires changing it.

### 4. Data Modeling & Persistence

- Follow the project’s existing data modeling, schema, migration, repository, and transaction patterns.
- Schema changes must be backward-compatible when possible.
- Data migrations must be safe, repeatable, and recoverable.
- Do not remove or repurpose persisted fields without considering existing data and consumers.
- Handle nullable, missing, legacy, and malformed data defensively.
- Avoid coupling persistence models directly to external API contracts when the project separates them.

### 5. Transactions & Consistency

- Use transactions for operations that must succeed or fail atomically.
- Keep transaction scopes as small as possible.
- Avoid long-running network calls, blocking operations, or external side effects inside transactions.
- Consider idempotency, retries, race conditions, and partial failure.
- For distributed workflows, design for eventual consistency and compensating actions where needed.

### 6. Error Handling

- Use the project’s existing error hierarchy, error codes, and error response format.
- Distinguish validation errors, authentication errors, authorization errors, not-found errors, conflict errors, rate-limit errors, dependency failures, and internal errors.
- Do not swallow exceptions silently.
- Do not expose stack traces, internal paths, SQL, secrets, or infrastructure details to clients.
- Preserve enough internal context for debugging through logs, traces, or structured metadata.

### 7. Logging, Metrics & Tracing

- Use structured logging and the project’s existing observability utilities.
- Logs must include useful context such as request ID, correlation ID, user/account scope, operation name, and failure reason where applicable.
- Do not log secrets, tokens, passwords, raw credentials, sensitive PII, or full request/response bodies by default.
- Add metrics for important business operations, latency, errors, retries, queue depth, and dependency failures.
- Preserve trace context across services, jobs, queues, and external calls when supported.

### 8. Configuration & Environment

- Use the project’s existing configuration mechanism.
- Do not hard-code environment-specific values, secrets, URLs, credentials, feature flags, timeouts, or resource limits.
- Configuration must be explicit, typed or validated where possible, and safe by default.
- Separate development, test, staging, and production configuration.
- Missing required configuration should fail fast with clear diagnostics.

### 9. External Integrations

- Use existing clients, SDK wrappers, gateways, or adapters for external systems.
- Do not scatter raw integration calls across business logic.
- Define timeouts, retries, backoff, circuit breaking, and fallback behavior according to project patterns.
- Treat external systems as unreliable and validate their responses.
- Handle API version differences, rate limits, quotas, and partial outages.
- Keep integration-specific mapping isolated from core business logic.

### 10. Asynchronous Jobs & Background Processing

- Follow existing queue, scheduler, worker, and job patterns.
- Jobs must be idempotent or protected against duplicate execution.
- Define retry policies, backoff, dead-letter handling, and failure visibility.
- Avoid unbounded background work.
- Persist enough state to resume or diagnose interrupted jobs.
- Separate user-facing request latency from long-running background processing.

### 11. Concurrency & Idempotency

- Protect critical operations against duplicate requests, race conditions, and concurrent updates.
- Use idempotency keys, optimistic locking, pessimistic locking, unique constraints, or equivalent mechanisms where appropriate.
- Avoid relying only on application-level checks when data-level guarantees are required.
- Ensure retries do not create duplicate records, duplicate charges, duplicate notifications, or inconsistent state.

### 12. Performance & Scalability

- Avoid unnecessary database queries, N+1 access patterns, large payloads, and unbounded scans.
- Use pagination, filtering, projection, batching, indexing, caching, and streaming where appropriate.
- Keep request latency, memory usage, CPU usage, and connection usage under control.
- Avoid blocking the event loop, worker pool, or request thread with heavy computation or long I/O.
- Design expensive operations to scale with data volume and traffic growth.

### 13. Caching

- Use caching only when consistency requirements and invalidation behavior are understood.
- Define cache keys, TTLs, invalidation rules, and fallback behavior clearly.
- Do not cache user-specific or permission-sensitive data without proper isolation.
- Avoid stale data causing incorrect business decisions.
- Ensure cache failures do not break critical flows unless the cache is the source of truth by design.

### 14. Files, Objects & Binary Data

- Validate file type, size, extension, metadata, and ownership before processing.
- Do not trust client-provided filenames, MIME types, paths, or metadata.
- Store files through approved storage abstractions.
- Avoid loading large files fully into memory unless necessary.
- Handle upload/download interruptions, cleanup of temporary files, and object lifecycle policies.
- Enforce authorization for file access, preview, download, and deletion.

### 15. Notifications & Side Effects

- Side effects such as emails, webhooks, payments, messages, and notifications must be explicit and traceable.
- Avoid triggering irreversible side effects before the main state change is safely persisted.
- Use outbox, event, or queue patterns when reliability matters.
- Ensure side effects are deduplicated and retry-safe.
- Do not send sensitive data to third-party channels unless explicitly required and permitted.

### 16. Compatibility & Versioning

- Preserve compatibility with existing clients, consumers, schemas, messages, and integrations.
- Version APIs, events, payloads, and migrations when behavior changes may affect consumers.
- Support rolling deployments where old and new code may run simultaneously.
- Avoid deployment steps that require perfect synchronization unless explicitly planned.
- Handle legacy data and legacy clients gracefully.

### 17. Testing & Verification

- Add or update tests for business logic, API behavior, validation, authorization, persistence, and failure paths.
- Cover edge cases, invalid inputs, empty states, concurrency-sensitive behavior, and permission boundaries.
- Use integration tests for database, queue, cache, or external integration behavior when project patterns support them.
- Do not rely only on happy-path testing.
- Run the project’s configured linting, type checking, tests, migrations, and build checks before completion.
- If automated coverage is not feasible, provide a concise manual verification checklist.

### 18. Documentation & Delivery Notes

- Document new APIs, configuration, migrations, jobs, permissions, operational requirements, and behavior changes.
- Include deployment or migration notes when changes affect data, infrastructure, background workers, or external systems.
- Provide clear verification steps and known limitations.
- Keep documentation concise and close to the code or project’s established documentation location.

------

## Part 2: Security Standards (Generic Principles)

### 1. Authentication & Session Security

- Use the project’s existing authentication mechanism.
- Do not invent custom authentication flows unless explicitly required.
- Store credentials and session material using the most secure mechanism available for the target platform.
- Tokens, secrets, credentials, and session identifiers must never appear in URLs, logs, error messages, or analytics.
- Enforce session expiry, revocation, rotation, and logout behavior according to project policy.

### 2. Authorization & Permission Boundaries

- Every sensitive operation must enforce authorization on the backend.
- Do not rely on frontend checks, hidden routes, client roles, or client-provided permissions.
- Validate ownership, tenant scope, organization scope, role, and resource-level permissions.
- Apply authorization consistently across APIs, jobs, file access, exports, webhooks, and internal tools.
- Default to deny when permission state is missing or ambiguous.

### 3. Input Validation & Output Safety

- Treat all client input, headers, cookies, files, webhooks, messages, and third-party responses as untrusted.
- Validate type, format, length, range, enum values, ownership, and business constraints.
- Normalize input where appropriate before processing.
- Encode or sanitize output when producing HTML, URLs, scripts, templates, emails, logs, or exported files.
- Avoid unsafe dynamic evaluation, template execution, reflection, or deserialization.

### 4. Injection Prevention

- Prevent SQL, NoSQL, command, template, LDAP, path, XML, SSRF, header, log, and expression injection.
- Use parameterized queries, safe builders, approved serializers, and project-level abstractions.
- Do not concatenate untrusted input into queries, commands, paths, URLs, templates, or headers.
- Restrict dynamic execution and dynamic loading to approved internal use cases.

### 5. Secrets & Configuration Security

- Never hard-code secrets, credentials, certificates, private keys, tokens, or internal endpoints.
- Use the project’s approved secret management and environment configuration.
- Secrets must be scoped, rotated, and excluded from source control, build artifacts, logs, and client-facing responses.
- Fail safely when required secrets are missing or invalid.

### 6. Data Privacy & Minimization

- Collect, store, process, and return only the data required for the feature.
- Minimize sensitive fields in logs, events, exports, caches, and third-party calls.
- Mask or redact sensitive data by default.
- Respect retention, deletion, consent, and compliance requirements where applicable.
- Avoid exposing internal identifiers unless they are part of the public contract.

### 7. Transport & Network Security

- Use encrypted transport for service-to-service and client-to-service communication.
- Do not disable certificate validation or weaken TLS behavior in production.
- Restrict outbound network access where possible.
- Protect internal services from public exposure.
- Validate redirects, callback URLs, webhook URLs, and user-provided remote resource URLs.

### 8. Rate Limiting & Abuse Protection

- Protect authentication, write operations, expensive queries, search, file upload, export, and notification endpoints from abuse.
- Apply rate limits, quotas, throttling, request size limits, and timeout limits according to project patterns.
- Avoid unbounded resource consumption from a single user, tenant, IP, token, or job.
- Ensure abuse protections fail safely and produce safe error responses.

### 9. File & Deserialization Security

- Treat uploaded files, archives, serialized objects, and imported data as untrusted.
- Validate size, type, structure, ownership, and content before processing.
- Avoid unsafe deserialization and archive extraction.
- Protect against path traversal, decompression bombs, malicious metadata, and executable payloads.
- Process high-risk files in isolated or restricted environments when required.

### 10. Third-Party & Webhook Security

- Verify webhook signatures, timestamps, replay protection, source identity, and payload structure.
- Do not trust third-party callbacks solely because they reach the server.
- Limit third-party permissions and credentials to the minimum required scope.
- Isolate third-party failures from core business flows where possible.
- Do not expose sensitive internal data to external services unless explicitly required.

### 11. Operational Safety

- Production systems must not expose debug endpoints, admin consoles, stack traces, test accounts, seed data, or development-only behavior.
- Administrative and maintenance operations must be authenticated, authorized, logged, and auditable.
- Dangerous operations must require explicit intent and should be reversible where feasible.
- Prefer safe defaults for configuration, feature flags, migrations, and rollout behavior.

------

## Part 3: Dependency Management Standards (Generic Principles)

### 1. Decision Process for Adding a Dependency

Before adding any new dependency, answer sequentially:

- Does the project already have similar capability?
- Can the platform, standard library, or existing infrastructure satisfy the requirement?
- Is a lightweight internal implementation feasible and maintainable?
- Is the capability business-specific or likely to be frequently customized?
- Does the dependency introduce runtime, security, licensing, operational, or maintenance risk?

**Golden rule**: The closer to business logic, the more customization required, the more frequent changes → the less you should rely on third-party dependencies.

### 2. No Duplicate Capabilities

- Do not introduce multiple libraries or frameworks that solve the same category of problems.
- Reuse existing framework, validation, database, HTTP, logging, testing, queue, cache, and configuration abstractions.
- Avoid parallel infrastructure patterns unless explicitly approved.

### 3. Evaluate Dependency Health

- Evaluate maintenance activity, release history, security posture, license, ecosystem adoption, documentation, and compatibility.
- Prefer mature, actively maintained dependencies with clear ownership and predictable upgrade paths.
- Avoid abandoned, obscure, vulnerable, or overly complex dependencies.
- Consider transitive dependency depth and supply-chain risk.

### 4. Runtime & Operational Cost

- Evaluate startup time, memory usage, CPU cost, I/O behavior, connection usage, binary size, and deployment impact.
- Avoid heavy dependencies for small isolated tasks.
- Do not add dependencies that require new infrastructure, daemons, services, permissions, or runtime assumptions without explicit justification.
- Ensure dependencies work in the project’s target deployment environment.

### 5. Security & License Control

- New dependencies must be compatible with project security and license requirements.
- Do not introduce dependencies with known critical vulnerabilities unless there is an approved mitigation.
- Avoid packages with suspicious install scripts, excessive permissions, or unclear provenance.
- Keep dependency versions locked and auditable.

### 6. Encapsulation & Replaceability

- Do not scatter third-party APIs directly across business logic.
- Wrap external libraries, SDKs, protocols, and infrastructure clients behind project-level interfaces, adapters, or gateways.
- Keep vendor-specific mapping and error handling isolated.
- Design wrappers so dependencies can be upgraded or replaced with minimal business impact.

### 7. Boundaries of Self-Built Solutions

**Prefer self-built** for:

- Business-specific workflows and domain rules.
- Thin adapters around existing infrastructure.
- Small utilities with limited edge cases.
- Lightweight orchestration and composition logic.

**Do not self-build** for:

- Cryptography, authentication protocols, password hashing, random number generation, or security-critical primitives.
- Database engines, message brokers, distributed locks, consensus, or complex infrastructure primitives.
- Complex parsers, serializers, compression, file format processors, or networking protocols.
- Capabilities where correctness, interoperability, or long-term maintenance requires mature implementations.

**Judgment rule**: High complexity, high correctness requirements, high interoperability, or high security sensitivity → mature dependency; high business customization and low technical complexity → self-built.

------

## Enforcement

- All code must pass the project’s configured formatting, linting, type checking, tests, migration checks, and build checks before completion.
- Changes must include appropriate tests or a clear verification checklist.
- Pull Requests must include a self-check against Security and Dependency standards.
- Any new third-party dependency must be documented with purpose, version, rationale, alternatives considered, and operational impact.
- Any schema change, migration, background job, external integration, or security-sensitive change must include deployment and rollback considerations.