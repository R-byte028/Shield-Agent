# Show-Up Pot

Escrowed event pots on NEAR. Guests put money in (or the host funds a prize), show up and solve a challenge, and winners split the pot. No-shows fund the people who came.

Tracks: Social Money, Real-World Money.

## Two modes
- **Host-funded:** host attaches the prize pot, guests join free. Winners are the first N to enter the code, or host-picked from submitted links.
- **Guest-pays:** each guest picks their own stake (host sets a minimum). Winners are decided by a code or puzzle answer, so the host cannot choose them. The pot is shared in proportion to winners' stakes.

## Fairness rules enforced by the contract
- Funds are held by the contract, not the host.
- Timeline: register_by <= submit_by < finalize_by. Host cannot settle before submissions close, and cannot settle after the deadline.
- After the deadline anyone can trigger refunds; each guest gets their own stake back.
- Host cannot join own event; one registration per account; max 100 guests.
- Host-funded events cannot be cancelled once any code or link is submitted.
- Winner limit (`num_winners`) is enforced on-chain, first-come-first-served for code events.
- Payout leaves no dust (first winner gets the remainder).

## Known limits (say these in the pitch)
- Host knows the code, so in guest-pays events they could join with fake accounts. Best for trusted communities. Next step: per-guest QR codes or allowlists.
- Guests can share the code with other registered guests.
- Payouts are pull-based: recipients claim individually. Failed transfers return to Pending through a private callback and can be retried.
- Not audited. Testnet only.

## MAINNET: read before deploying
This package is configured for NEAR mainnet and uses real money. The contract is **unaudited and has not been compiled or tested by its author**. Do not deploy with real funds until:
1. `cargo test` passes and you have tested the full flow on the testnet version first.
2. Someone other than the author has reviewed the contract.
3. You start with small amounts only.

```
cargo test
cargo near build non-reproducible-wasm
near contract deploy YOURNAME-showup.near use-file target/near/event_escrow.wasm with-init-call new json-args '{}' prepaid-gas '100 Tgas' attached-deposit '0 NEAR' network-config mainnet sign-with-keychain send
```
You need a funded mainnet account first. Command syntax changes between CLI versions; check `near --help`. The RPC URL in index.html may rate-limit or change; swap in a provider of your choice if requests fail.

Frontend: open `index.html` through a local server (`python3 -m http.server`), enter the contract account, connect a mainnet wallet.

## 3-minute demo
1. Host creates a guest-pays event, min 1 NEAR, code `blue-falcon`, 2 winners.
2. Three guest accounts register with different stakes.
3. Two guests enter the code; a third tries after the winners are full and is rejected.
4. Host settles: pot splits by stake. Show the failed cheat attempts (host picking winners, settling early).
5. Create a host-funded event with links, host picks winners. Each recipient then claims their payout.

## Pitch
Event no-shows waste hosts' money and time. Show-Up Pot makes commitment real: deposits are escrowed on-chain, the contract decides who earned the pot, and everyone is refunded if the host disappears. Next: pay in any asset from any chain with NEAR Intents, per-guest QR codes, reputation for hosts.
