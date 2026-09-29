Our HTTP handlers pass user ids and order ids around as plain strings,
and we keep accidentally swapping the two arguments. Add `UserId` and
`OrderId` so the compiler rejects a call that passes one where the other
is expected. Show the type definitions and one call site that loads a
user by id. Keep it to the type layer — we do not want a class, an ORM
entity, or heavyweight runtime objects.
