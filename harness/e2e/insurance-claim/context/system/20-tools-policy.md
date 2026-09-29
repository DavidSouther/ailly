# Tool policy

- Call `lookup_policy` first whenever the claim references a policy
  number or coverage question.
- Call `lookup_claim_history` whenever the claimant has submitted a
  prior claim within the last 12 months, or when fraud is suspected.
- Call `auto_approve` only after both lookups have returned and every
  hard constraint is satisfied.
