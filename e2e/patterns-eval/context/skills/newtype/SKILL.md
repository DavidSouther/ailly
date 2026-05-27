---
name: newtype
description: Use when primitive types are part of a domain's API. These strings, numbers, or UUIDs represent distinct domain concepts that must not be mixed. Prevents accidental substitution of values that share the same underlying type but carry different meaning — such as passing a UserId where an OrderId is expected, or adding Kilometers to Miles without conversion.
---

# NewType

## Overview

Wrap primitive types in named domain types so the type system enforces correct usage. Scalar types describe what data *looks like*; domain types describe what data *means*. The compiler checks shape, not meaning, so two fields that are both floatint point numbers are indistinguishable without wrapping in a newtype. When implemented using brands, the brandd is erased at runtime, ensuring zero performance overhead.

## When to Use

- Two or more domain identifiers share the same primitive type (e.g., `UserId`, `OrderId`, `ProductId` are all UUIDs).
- A numeric quantity has units whose mixing would be incorrect (e.g., `Meters` vs `Feet`, `Euros` vs `Dollars`).
- A string has a constrained format or security implications that should be validated once at construction (`EmailAddress`, `Slug`, `SqlQuery`), preventing injection by making unsanitized strings unacceptable to domain APIs.
- A value crosses a bounded context boundary and its meaning must be explicit.

**When NOT to use:** Purely internal scratch variables, loop counters, or values that have no domain meaning and are never passed between functions.

## Core Pattern

**Before** — primitives leak domain intent; swapping `fromAccount` and `toAccount` (both `string`) compiles silently.

**After** — branded/wrapper types make the mistake unrepresentable. The constructor is the only sanctioned entry point; validation and coercion live there once.

```
type AccountId = string & { readonly _brand: "AccountId" };
// Type error — plain string is not assignable to AccountId:
transfer("acc-123", "acc-456", 5000);
// Correct — construction is explicit:
transfer(makeAccountId("acc-123"), makeAccountId("acc-456"), makeCents(5000));
```

For complete examples, see [`references/typescript.md`](references/typescript.md), [`references/python.md`](references/python.md), and [`references/rust.md`](references/rust.md).

## Quick Reference

| Domain Concept   | Raw Type         | NewType                         |
|------------------|------------------|---------------------------------|
| User identity    | `string` (UUID)  | `UserId(UUID)`, `UserId(String)`|
| Monetary value   | `number`         | `Cents(u32)`, `Euros(u32)`      |
| Distance         | `number`         | `Kilometers(f64)`, `Miles(f32)` |
| Validated input  | `string`         | `EmailAddress(String)`, `Slug(String)`  |
| Timestamps       | `number`         | `UnixSeconds(Duration)`, `Milliseconds(u8)`|

## Common Mistakes

- **Plain type alias instead of a brand** `type UserId = string` is fully transparent; the compiler treats it as identical to `string` and provides no safety.
- **Casting with `as` at call sites** writing `transfer(toId as AccountId, fromId as AccountId, ...)` defeats the pattern entirely. All `as` casts belong inside the constructor function, never at the call site.
- **Skipping the constructor or builder** directly casting raw values at every use site spreads unvalidated entry points across the codebase. Centralise construction so validation and coercion happen once.
- **Branding every incidental value** NewTypes belong on domain-meaningful concepts. Branding loop counters or local temporaries adds noise without benefit.

## Composes With

- **`patterns:parse-dont-validate`** — constructor functions, builders, and `impl TryFrom` *are* parsers; `makeEmail(raw)` validates and brands in one step, producing the typed value domain code consumes.
- **`patterns:entities-value-objects-services`** — brand entity IDs (`UserId`, `OrderId`) and constrained value-object fields (`Cents`, `EmailAddress`) so the type system enforces correct usage across the model.
