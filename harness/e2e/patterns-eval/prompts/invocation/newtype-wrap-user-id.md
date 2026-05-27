Introduce a `UserId` type that wraps a `String`. The inner primitive
must be private, construction must be the only sanctioned entry point,
and a plain string must not be assignable where a `UserId` is required.
No `as` casts at call sites. Show the type definition and one example
call site.
