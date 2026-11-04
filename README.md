# PactChain — On-chain Event Escrow

PactChain is a prototype for community events where a host or sponsor funds a prize pool, participants join for free, complete a challenge, and claim eligible rewards through a NEAR smart contract.

> **Safety:** This repository is an unaudited prototype. Use NEAR testnet only and test tokens only. Do not deploy this version to mainnet or use it to hold real funds.

## Current prototype

- Host-funded events: the host supplies the prize pot; guests join without depositing.
- Participant deposits and stake-based prize pools are disabled; registration is free.
- Code challenges: eligible guests submit the secret code.
- Link submissions: guests submit a URL for host review.
- Pull-based payouts: each recipient claims their payout; failed transfers can return to Pending.
- Refunds: after the finalization deadline, anyone can trigger the refund flow.

## Important contract rules and review status

The contract enforces registration limits, event timelines, unique guest registration, and a maximum of 100 guests. The sponsor-funded pool is divided evenly among eligible winners, with rounding remainder assigned to the first winner.

**Known security work still required:**
- Continue regression coverage for winner selection, settlement, refunds, payout callbacks, retry behavior, and duplicate-claim prevention.
- Validate that only the sponsor funds the pool and that participants cannot attach deposits.
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

1. Create a sponsor-funded event on testnet; participants register for free.
2. Register guest test accounts.
3. Submit the code or a proof link.
4. Review the submissions and settle the sponsor-funded pool according to the challenge type.
5. Have each payout recipient claim their pending payout.
6. Test sponsor refunds after cancellation/deadline and failed-claim retry behavior.

## Project status

This is a hackathon prototype, not an audited financial product. The UI and contract should be treated as experimental until all security work and testnet checks are complete.
