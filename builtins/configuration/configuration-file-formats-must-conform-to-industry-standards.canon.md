---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - configuration
  - api-design
---
Persistent configuration file formats MUST organize settings into orthogonal, typed namespaces separating upstream provider settings from shared defaults and execution target overrides, rather than employing flat root keys.

Rationale: Following Docker (`config.json`), AWS CLI profiles, and Kubernetes `kubeconfig`, isolating provider definitions from target overrides prevents key collisions and supports extensible vendor settings without bespoke key prefixes.
