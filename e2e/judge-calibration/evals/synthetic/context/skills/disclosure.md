==> configuring-logging/SKILL.md <==
---
name: configuring-logging
description: Use when bootstrapping a service's logging pipeline — selecting a subscriber/registry, layering formatter/filter/enricher/exporter, attaching resource attributes, wiring W3C trace propagators, choosing head- or tail-based sampling, configuring redaction as defense-in-depth, and arranging graceful shutdown flush. Applies once at process start, never inside library code.
---

==> emitting-logs/SKILL.md <==
---
name: emitting-logs
description: Use when writing log call sites in domain or application code — choosing a severity, attaching structured fields with semantic-convention keys, scoping spans to units of work, recording errors with their full chain at a single boundary, and naming business events. Applies every time code emits a log record.
---

==> newtype/SKILL.md <==
---
name: newtype
description: Use when primitive types are part of a domain's API. These strings, numbers, or UUIDs represent distinct domain concepts that must not be mixed. Prevents accidental substitution of values that share the same underlying type but carry different meaning — such as passing a UserId where an OrderId is expected, or adding Kilometers to Miles without conversion.
---