# Repository-Local Canons (`.canons/`)

This directory contains **repo-local (`internal`) dogfood canons** specific to the `canon-clerk` repository itself (architecture doc discipline, `llms.txt` hygiene, `SPEC.md` exemplar alignment, GitHub repository metadata parity, and built-in corpus portability).

Looking for the reusable, domain-scoped standard rule packs embedded into the `canon-clerk` binary (`builtin:<domain>/<slug>`)? See [`builtins/`](../builtins/) (indexed in the root [`README.md`](../README.md#the-canon-corpus)).

## Conventions

- All canon files MUST use the `*.canon.md` extension (`SPEC.md` §2.1); non-canon Markdown documents like this `README.md` are ignored by `discover` and `validate`.
- Every canon in `.canons/` MUST remain flat (no subdirectories) and carry the `internal` tag.
- Portable standards applicable to any adopting repository belong in [`builtins/<domain>/`](../builtins/) instead.
