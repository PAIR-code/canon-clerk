# Diagnostic Leaf: Connectivity & Health (`probe`)

**Status:** Authoritative Architectural Standard  
**Subcommand:** `canon-clerk probe`  
**Aliases:** `check-health`, `ping`  
**Pipeline Track:** Diagnostic Leaf (Branch B Termination)

---

## 1. Domain Concept & Role

`probe` serves as the **Diagnostic Network Health Check** for configured model endpoints. In the court clerkship taxonomy, it represents testing the teleconferencing equipment and audio-visual communication links before a remote hearing begins.

- **Imperative Verb:** `probe`
- **Court Clerkship Role:** Live connectivity and latency verification of provider endpoints.
- **Metric Pair:** N/A (Live connectivity probe; reports `status` and `latencyMs`).

---

## 2. Dependencies

- **Direct Prerequisites:** `configure` (requires resolved provider credentials and model specifiers).
- **Transitive Prerequisites:** None.
- **Pruned from Execution:** Branch A (`intake`, `discover`, `validate`) and the Heuristic Cascade (`docket`, `admit`, `audit`).
- **Cascade Isolation:** `probe` is **never executed during review cascades** (`canon-clerk audit`). This ensures that evaluation runs do not incur redundant health-check latency prior to screening.

---

## 3. Specific Inputs

### Standard Streams & CLI Options
- Incoming `Caseload` via `--caseload <path|->` (or auto-resolved via `configure`).
- `--timeout <ms>`: Network probe timeout (default: 5000ms).
- `--json`: Emits structured probe results or enriched Caseload.

---

## 4. Process & Logic

1. **Endpoint Resolution:** Reads configured `screenerModel` and `auditorModel` from `caseload.config`.
2. **Ping Request Assembly:** Constructs lightweight probe requests (0 reasoning tokens, minimum prompt payload) to verify authentication and reachability.
3. **Concurrent Probe:** Concurrently sends probe requests to both endpoints using `Promise.all`:
   - Measures roundtrip response latency in milliseconds (`latencyMs`).
   - Verifies model availability and provider credential validity.
4. **Result Recording:** Formats latency statistics and connection outcomes.

---

## 5. Outputs & Caseload Delta

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

### Caseload Delta
- `caseload.probe`: Attached with live connection outcomes and latencies for screener and auditor endpoints.

---

## 6. Gate & Error Semantics

- **Exit Code 0:** All configured endpoints are reachable, authenticated, and responsive.
- **Exit Code 2 (Connectivity / Authentication Error):**  
  **If provider credentials are rejected (401/403) or endpoints are unreachable (network timeout / 5xx), `probe` terminates with exit code `2`.**  
  *Provides rapid, isolated diagnostics without executing file parsing or canon discovery.*
