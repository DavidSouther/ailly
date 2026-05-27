Emit an `order.placed` log record from inside the order-placement
handler. Use a stable message body (no interpolation) and attach
structured fields under OpenTelemetry semantic-convention keys:
`order.id`, `user.id`, and `http.response.status_code`. Set the event
name to `order.placed`.
