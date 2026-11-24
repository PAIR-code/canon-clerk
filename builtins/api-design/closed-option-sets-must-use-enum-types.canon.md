---
triggers:
  - "**/src/**"
inspect:
  - diff
tags:
  - api-design
---
Following "Make Illegal States Unrepresentable" (Minsky) and the Rust API Guidelines (`C-CUSTOM-TYPE`), public function parameters and return values representing a closed set of discrete modes or states MUST use an enumeration or tagged union rather than raw booleans, integers, or strings.

Exception: Wire-protocol, FFI, or CLI serialization boundary types MAY use raw primitive discriminants IFF required by the external schema or ABI.
