---
triggers:
  - "packages/configuration/**"
tags:
  - internal
  - configuration
  - api-design
---
Persistent configuration file formats MUST organize settings into orthogonal, typed namespaces separating provider authentication and transport parameters from consumer execution profiles or tiers, rather than employing flat, ambiguous root keys.

Rationale: Grounded in established cloud and container tooling conventions (such as Docker `~/.docker/config.json`, AWS CLI credential profiles, and Kubernetes `kubeconfig`), isolating upstream provider definitions from consumer task profiles prevents parameter collisions, eliminates multi-provider credential ambiguity, and affords extensible vendor options without bespoke key prefixes.
