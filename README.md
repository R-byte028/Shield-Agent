# PactChain — On-chain Event Escrow

PactChain is a prototype for community events where hosts fund a prize pot or participants contribute stakes, complete a challenge, and claim payouts through a NEAR smart contract.

> **Safety:** This repository is an unaudited prototype. Use NEAR testnet only and test tokens only. Do not deploy this version to mainnet or use it to hold real funds.

## Current prototype

- Host-funded events: the host supplies the prize pot; guests join without depositing.
- Guest-pays events: guests deposit at least the configured minimum.
- Code challenges: eligible guests submit the secret code.
- Link submissions: guests submit a URL for host review.
- Pull-based payouts: each recipient claims their payout; failed transfers can return to Pending.
- Refunds: after the finalization deadline, anyone can trigger the refund flow.

## Important contract rules and review status

The contract enforces registration limits, event timelines, unique guest registration, and a maximum of 100 guests. The current review branch also records security findings that must be resolved before any public deployment.

**Known security work still required:**
- Ensure every code-challenge winner is derived only from successful on-chain code submissions, regardless of who funded the event or whether a winner cap is set.
- Ensure link-submission winners have actually submitted a proof link. On-chain presence does not establish that the proof is valid or high quality.
- Add regression tests for winner selection, settlement, refunds, payout callbacks, retry behavior, and duplicate-claim prevention.
- Complete independent security review and end-to-end testnet validation.

## Build and test

Requirements: a current stable Rust toolchain and the `wasm32-unknown-unknown` target.

```sh
cargo test
rustup target add wasm32-unknown-unknown
cargo build --target wasm32-unknown-unknown --release
```

The GitHub Actions workflow runs the unit tests and WebAssembly build on pushes and pull requests targeting `main`.

## Frontend

The prototype is a static `index.html` page. Serve it locally, open it in a browser, enter a deployed **testnet** contract account, and connect a testnet wallet. The frontend is configured to use NEAR testnet RPC and wallet endpoints.

Do not change the frontend to mainnet or enter a mainnet contract account. Mainnet deployment is intentionally out of scope until the security findings are fixed, tests pass, and an independent review is complete.

## Demo flow

1. Create a host-funded or guest-pays event on testnet.
2. Register guest test accounts.
3. Submit the code or a proof link.
4. Review the event and settle it according to the challenge type.
5. Have each payout recipient claim their pending payout.
6. Test deadline refunds and failed-claim retry behavior.

## Project status

This is a hackathon prototype, not an audited financial product. The UI and contract should be treated as experimental until all security work and testnet checks are complete.
