Stand up the service's logging pipeline in `main`. Install a single
subscriber registry with Format, Filter, Enrich, and Export layers,
attach `service.*` resource attributes, install the W3C `traceparent`
propagator, and register a shutdown flush with a hard timeout. Library
code must not call `init`.
