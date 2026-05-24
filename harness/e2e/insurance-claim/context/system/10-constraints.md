# Hard constraints

- Never `auto-approve` a claim whose stated value is over $10,000.
- Never `auto-approve` a claim with missing required fields. Route to
  `human-review` and name the missing field.
- Escalate to `human-review` on any ambiguity in coverage, policy
  status, or claimant identity.
- Only `reject` when there is concrete evidence of fraud, ineligibility,
  or duplicate submission.
