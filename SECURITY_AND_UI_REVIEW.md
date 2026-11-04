# PactChain / Show-Up Pot — Review Notes

## Scope
This is an initial source review of the Rust contract and the static HTML frontend. It is not a formal security audit and does not establish that the application is safe for real funds.

## Contract findings requiring fixes and regression tests

### 1. Code-challenge winner selection can be bypassed
In `finalize`, the current guard rejects a caller-supplied winner list for guest-funded code events and for code events with a winner cap. A host-funded code event with no winner cap does not meet that guard. In that configuration, the host can pass an arbitrary list of registered accounts and bypass the code challenge.

**Required fix:** For every `Challenge::Code` event, require `winners == None` and derive winners exclusively from `completed`, regardless of funding mode or winner cap.

### 2. Link-submission winners are not checked against submissions
For `Challenge::Submission`, `finalize` accepts host-selected registered accounts but does not verify that each selected account submitted a proof link.

**Required fix:** Require every selected winner to appear in `submissions`. The contract can verify that a proof was submitted, but cannot judge its quality.

### 3. Refund and payout lifecycle needs broader coverage
The current tests cover a successful claim entering `InFlight`, but do not demonstrate callback behavior for both successful and failed transfers, repeated claims, or a refund/settlement race.

**Required tests:** callback success marks `Paid`; callback failure restores `Pending`; only the recipient can claim; a second claim while `InFlight` is rejected; settlement/refund cannot create duplicate payouts.

## Frontend review

The current app is a single static HTML page. It has the essential contract actions, but lacks a full dashboard, event discovery experience, clear transaction history, and structured mobile navigation. This branch refreshes its visual foundation and responsive form styling while preserving the existing DOM IDs and contract calls.

## Release gate
- Keep the app on testnet during development.
- Do not use the current mainnet configuration with real funds.
- Fix the winner-selection issues and add regression tests before any testnet release candidate.
- Verify wallet connection, event creation, deposits, submissions, settlement, claims, failed-transfer retries, and refunds on testnet.
- Obtain an independent review before any mainnet consideration.
