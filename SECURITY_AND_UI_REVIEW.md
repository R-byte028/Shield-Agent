# PactChain — Security and Product-Scope Review

## Scope
This is an initial source review, not a formal security audit. The prototype is restricted to sponsor-funded rewards: participants register for free and do not deposit or stake funds.

## Product-safety boundary
- Only the host/sponsor funds the prize pool.
- Participant registration must attach zero NEAR.
- The contract rejects nonzero `min_deposit` values and participant deposits.
- Eligible rewards are split evenly; participants' deposits or stakes are never used to calculate rewards.
- Cancellation, expiry, or a no-winner settlement returns the sponsor-funded amount through the normal claim flow.

## Contract safeguards added on this branch
- Code-challenge winners are derived only from successful on-chain submissions, regardless of winner cap.
- Link-submission winners must have an on-chain proof-link record. This does not establish proof quality; the host still reviews submissions.
- Regression tests cover host-selected code-winner rejection, proof-submission eligibility, free registration, rejection of participant-funded modes, and even sponsor-pool distribution.
- Payouts remain pull-based: only the named recipient can claim a Pending payout; the status changes to InFlight before the transfer is initiated; the private callback marks success as Paid or restores failure to Pending.

## Remaining verification
- Add explicit callback tests for successful transfer => Paid and failed transfer => Pending.
- Add explicit tests for non-recipient claims and duplicate claims while InFlight.
- Verify refund and settlement transitions cannot create duplicate sponsor payouts.
- Run all CI checks and perform end-to-end testnet validation of event creation, registration, submissions, settlement, claims, cancellation, and deadline refunds.
- Obtain independent security review before considering any deployment beyond testnet.

## Frontend
The static frontend is configured for NEAR testnet only. Its create/register forms now reflect sponsor-funded pools and free participant registration. Browser-based end-to-end validation has not yet been performed.

## Release gate
- Keep the app on testnet and use test tokens only.
- Do not use participant stakes, wagers, or deposits.
- Do not deploy to mainnet or hold real funds.
- Require passing CI, complete lifecycle tests, end-to-end testnet validation, and independent security review before reassessing release readiness.
