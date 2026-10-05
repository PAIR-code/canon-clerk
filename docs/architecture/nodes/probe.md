# Diagnostic Health Check (`probe`)

**Status:** Authoritative Architectural Standard  
**Core Domain Engine:** `@canon-clerk/configuration` (with `@canon-clerk/core`)  
**Driving Adapters:** `@canon-clerk/cli` (`probe`, `check-health`, `ping`), `@canon-clerk/action`, `@canon-clerk/integration-tests-private`

---

## 1. Domain Concept & Role (`core` / `configuration`)

`probe` serves as the **Diagnostic Network Health Check** for configured model endpoints. In the court clerkship taxonomy, it represents testing the teleconferencing equipment and audio-visual communication links before a remote hearing begins.

- **Imperative Verb:** `probe`
- **Court Clerkship Role:** Live connectivity and latency verification of provider endpoints.
- **Metric Pair:** N/A (Live connectivity probe; reports `status` and `latencyMs`).

---

## 2. Dependencies & Prerequisites (`core`)

- **Direct Prerequisites:** `configure` (requires resolved provider credentials and model specifiers in `caseload.config`).
- **Transitive Prerequisites:** None.
- **Pruned from Cascades:** `probe` is **never executed during review cascades** (`canon-clerk audit`). This ensures that evaluation runs do not incur redundant health-check latency prior to screening.

---

## 3. Core Functional Contract (`packages/configuration`)

```ts
export interface ProbeOptions {
  readonly timeoutMs?: number | undefined;
}

export function executeProbe(
  options: ProbeOptions,
  caseload: Caseload
): Promise<Caseload>;
```

### Caseload Delta
Populates the `.probe` field on the cumulative `Caseload`:

```ts
export interface ProbeEndpointResult {
  /** Connection outcome */
  readonly status: 'ok' | 'error';

  /** Roundtrip response latency in milliseconds */
  readonly latencyMs: number;

  /** Resolved model identifier probed */
  readonly model: string;

  /** Error message if connectivity failed */
  readonly error?: string | undefined;
}

export interface CaseloadProbe {
  /** Screener model endpoint probe result */
  readonly screener: ProbeEndpointResult;

  /** Auditor model endpoint probe result */
  readonly auditor: ProbeEndpointResult;
}
```

---

## 4. Process & Domain Logic (`configuration`)

1. **Endpoint Resolution:** Reads configured `screenerModel` and `auditorModel` from `caseload.config`.
2. **Ping Request Assembly:** Constructs lightweight probe requests (0 reasoning tokens, minimum prompt payload) to verify authentication and reachability.
3. **Concurrent Probe:** Concurrently sends probe requests to both endpoints using `Promise.all`:
   - Measures roundtrip response latency in milliseconds (`latencyMs`).
   - Verifies model availability and provider credential validity.
4. **Diagnostic Record:** Attaches endpoint latency and reachability metadata to `Caseload.probe`.

---

## 5. Driving Adapter: CLI (`packages/cli`)

The CLI exposes `probe` (aliased as `check-health` and `ping`):

```bash
# Human-readable connectivity and latency check:
canon-clerk probe

# Emit structured JSON probe record:
canon-clerk probe --json
```

### CLI Flags & Options
- `--timeout <ms>`: Request timeout in milliseconds (default: `5000ms`).
- `--caseload <path|->`: Ingests upstream Caseload.
- `--json`: Emits enriched Caseload JSON.

### CLI Exit Codes
- **0:** All configured endpoints are reachable, authenticated, and responsive.
- **2:** Provider credentials rejected (401/403) or endpoints unreachable (timeout / 5xx).

---

## 6. Driving Adapter: GitHub Action (`packages/action`)

1. **Diagnostic Action Step:** Invoked in dedicated connectivity workflows or self-hosted runner validation actions.
2. **Summary Emission:** Emits reachability tables and latency metrics to `GITHUB_STEP_SUMMARY`.
3. **Fail-Fast Gating:** Fails CI pipelines early if remote AI provider endpoints are down or firewall rules block egress.

---

## 7. Driving Adapter: Integration Tests (`packages/integration-tests-private`)

Integration tests invoke `executeProbe` against live Gemini endpoints to assert real-world authentication, network latency bounds, and endpoint stability before running deep adjudication test suites.
