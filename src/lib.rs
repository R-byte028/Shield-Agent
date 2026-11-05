use near_sdk::json_types::Base64VecU8;
use near_sdk::store::IterableMap;
use near_sdk::{env, near, require, AccountId, Gas, NearToken, PanicOnDefault, Promise, PromiseResult};

 const MAX_GUESTS: usize = 100; // keeps gas bounded when paying out

#[near(serializers = [borsh, json])]
#[derive(Clone)]
pub enum Challenge {
    /// No challenge: host picks winners (host-funded events only)
    None,
    /// Secret code / puzzle answer (sha256 of the code is stored)
    Code { hash: Vec<u8> },
    /// Guests submit links; host picks winners (host-funded events only)
    Submission,
}

#[near(serializers = [borsh, json])]
#[derive(Clone, Debug, PartialEq)]
pub enum PayoutStatus {
    Pending,
    InFlight,
    Paid,
}

#[near(serializers = [borsh, json])]
#[derive(Clone)]
pub struct Payout {
    pub recipient: AccountId,
    pub amount: NearToken,
    pub status: PayoutStatus,
}

#[near(serializers = [borsh, json])]
#[derive(Clone)]
pub struct Event {
    pub host: AccountId,
    pub title: String,
    /// Retained as a zero-valued compatibility field; participant deposits are disabled.
    pub min_deposit: NearToken,
    pub pot: NearToken,            // sponsor/host-funded prize pool only
    pub host_funded: NearToken,
    pub challenge: Challenge,
    /// How many winners the host wants (None = no limit). Code events: the first N
    /// correct codes win. Host-picked events: host may name at most N winners.
    pub num_winners: Option<u32>,
    // Nanosecond timestamps. Order: register_by <= submit_by < finalize_by
    pub register_by: u64,
    pub submit_by: u64,            // codes/links accepted until here
    pub finalize_by: u64,          // host must settle before this; after it anyone can refund
    pub guests: Vec<AccountId>,
    /// Retained for serialized-state compatibility; all entries are zero.
    pub stakes: Vec<NearToken>,
    pub completed: Vec<AccountId>,
    pub submissions: Vec<(AccountId, String)>,
    /// Pull-based payouts. Failed transfers return to Pending through the callback.
    pub payouts: Vec<Payout>,
    pub done: bool,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Contract {
    events: IterableMap<u64, Event>,
    next_id: u64,
}

#[near]
impl Contract {
    #[init]
    pub fn new() -> Self {
        Self { events: IterableMap::new(b"e"), next_id: 0 }
    }

    /// Host creates a sponsor-funded event. Attached NEAR is the only prize funding.
    /// Participant deposits are disabled; min_deposit must be zero.
    #[payable]
    pub fn create_event(
        &mut self,
        title: String,
        min_deposit: NearToken,
        code_hash: Option<Base64VecU8>,
        submissions_mode: bool,
        num_winners: Option<u32>,
        register_by: u64,
        submit_by: u64,
        finalize_by: u64,
    ) -> u64 {
        require!(!title.trim().is_empty(), "title is required");
        require!(title.len() <= 100, "title too long");
        if let Some(hash) = &code_hash {
            require!(hash.0.len() == 32, "code hash must be 32 bytes");
        }
        require!(
            env::block_timestamp() < register_by && register_by <= submit_by && submit_by < finalize_by,
            "bad timeline: now < register_by <= submit_by < finalize_by"
        );
        require!(
            min_deposit.as_yoctonear() == 0,
            "participant deposits are disabled; sponsor-funded events only"
        );
        if let Some(n) = num_winners {
            require!(n > 0 && (n as usize) <= MAX_GUESTS, "num_winners must be 1..=100");
        }
        let challenge = match (code_hash, submissions_mode) {
            (Some(h), _) => Challenge::Code { hash: h.into() },
            (None, true) => Challenge::Submission,
            (None, false) => Challenge::None,
        };
        let funded = env::attached_deposit();
        let id = self.next_id;
        self.next_id += 1;
        self.events.insert(id, Event {
            host: env::predecessor_account_id(),
            title,
            min_deposit,
            pot: funded,
            host_funded: funded,
            challenge,
            num_winners,
            register_by,
            submit_by,
            finalize_by,
            guests: vec![],
            stakes: vec![],
            completed: vec![],
            submissions: vec![],
            payouts: vec![],
            done: false,
        });
        id
    }

    /// Guests register for free. Only the host/sponsor funds the prize pool.
    #[payable]
    pub fn register(&mut self, id: u64) {
        let e = self.events.get_mut(&id).expect("no event");
        let me = env::predecessor_account_id();
        let amount = env::attached_deposit();
        require!(!e.done, "event closed");
        require!(env::block_timestamp() <= e.register_by, "registration closed");
        require!(me != e.host, "host can't register in own event");
        require!(!e.guests.contains(&me), "already registered");
        require!(e.guests.len() < MAX_GUESTS, "event full");
        require!(
            amount.as_yoctonear() == 0,
            "registration is free; participant deposits are disabled"
        );
        e.guests.push(me);
        e.stakes.push(NearToken::from_yoctonear(0));
    }

    /// Code / puzzle answer.
    pub fn submit_code(&mut self, id: u64, code: String) {
        let e = self.events.get_mut(&id).expect("no event");
        let me = env::predecessor_account_id();
        require!(!e.done && e.guests.contains(&me), "not allowed");
        require!(env::block_timestamp() <= e.submit_by, "submissions closed");
        match &e.challenge {
            Challenge::Code { hash } => {
                require!(env::sha256(code.as_bytes()) == *hash, "wrong code");
            }
            _ => env::panic_str("event has no code challenge"),
        }
        if !e.completed.contains(&me) {
            if let Some(n) = e.num_winners {
                require!((e.completed.len() as u32) < n, "all prizes already claimed");
            }
            e.completed.push(me);
        }
    }

    /// Link submission (one per guest, resubmitting replaces it).
    pub fn submit(&mut self, id: u64, proof: String) {
        let e = self.events.get_mut(&id).expect("no event");
        let me = env::predecessor_account_id();
        require!(!e.done && e.guests.contains(&me), "not allowed");
        require!(env::block_timestamp() <= e.submit_by, "submissions closed");
        require!(
            proof.starts_with("https://") || proof.starts_with("http://"),
            "submit a link starting with http(s)://"
        );
        require!(proof.len() <= 500, "link too long");
        if let Some(s) = e.submissions.iter_mut().find(|(g, _)| *g == me) {
            s.1 = proof;
        } else {
            e.submissions.push((me, proof));
        }
    }

    pub fn get_submissions(&self, id: u64) -> Vec<(AccountId, String)> {
        self.events.get(&id).expect("no event").submissions.clone()
    }

    /// Host cancels before submissions close: everyone gets their own stake back.
    pub fn cancel(&mut self, id: u64) {
        let e = self.events.get_mut(&id).expect("no event");
        require!(env::predecessor_account_id() == e.host, "host only");
        require!(!e.done, "already settled");
        require!(env::block_timestamp() <= e.submit_by, "too late to cancel");
        // Host-funded: once anyone has submitted a code or link, the host can't cancel
        if e.min_deposit.as_yoctonear() == 0 {
            require!(
                e.completed.is_empty() && e.submissions.is_empty(),
                "can't cancel after a submission is made"
            );
        }
        e.done = true;
        Self::refund_all(e);
    }

    /// Host settles inside the allowed window. The sponsor-funded prize pool is
    /// split evenly among eligible winners. No winners means the sponsor is refunded.
    pub fn finalize(&mut self, id: u64, winners: Option<Vec<AccountId>>) {
        let e = self.events.get_mut(&id).expect("no event");
        require!(env::predecessor_account_id() == e.host, "host only");
        require!(!e.done, "already settled");
        let now = env::block_timestamp();
        // Code events with a winner limit: all prizes claimed = host can settle early
        let all_claimed = e.num_winners.map_or(false, |n| e.completed.len() as u32 >= n);
        let code_event = matches!(e.challenge, Challenge::Code { .. });
        require!(now > e.submit_by || all_claimed, "submissions still open");
        require!(now <= e.finalize_by, "deadline passed, use refund");
        // Code-challenge winners must always be determined by successful on-chain
        // submissions, regardless of who funded the event or whether a cap is set.
        if code_event {
            require!(winners.is_none(), "winners are set by the code challenge");
        }
        let winners = winners.unwrap_or_else(|| e.completed.clone());

        // A host may choose winners for link-submission events, but only from
        // accounts that actually submitted a proof link.
        if matches!(e.challenge, Challenge::Submission) {
            for winner in &winners {
                require!(
                    e.submissions.iter().any(|(guest, _)| guest == winner),
                    "winner did not submit proof"
                );
            }
        }
        if let Some(n) = e.num_winners {
            require!(winners.len() as u32 <= n, "more winners than the event allows");
        }
        for (i, w) in winners.iter().enumerate() {
            require!(e.guests.contains(w), "winner not registered");
            require!(!winners[..i].contains(w), "duplicate winner");
        }
        e.done = true;
        if winners.is_empty() {
            Self::refund_all(e);
            return;
        }
        let total = e.pot.as_yoctonear();
        let amounts = split_pot(total, &vec![0; winners.len()]);
        e.payouts.clear();
        for (recipient, amount) in winners.into_iter().zip(amounts) {
            if amount > 0 {
                e.payouts.push(Payout {
                    recipient,
                    amount: NearToken::from_yoctonear(amount),
                    status: PayoutStatus::Pending,
                });
            }
        }
    }

    /// Claim one pending payout. A failed transfer is restored to Pending by callback.
    pub fn claim_payout(&mut self, id: u64) -> Promise {
        let e = self.events.get_mut(&id).expect("no event");
        let caller = env::predecessor_account_id();
        let (index, recipient, amount) = e.payouts.iter_mut().enumerate()
            .find(|(_, payout)| payout.recipient == caller && payout.status == PayoutStatus::Pending)
            .map(|(index, payout)| {
                payout.status = PayoutStatus::InFlight;
                (index as u64, payout.recipient.clone(), payout.amount)
            })
            .expect("no pending payout for caller");

        Promise::new(recipient)
            .transfer(amount)
            .then(
                Promise::new(env::current_account_id()).function_call(
                    "on_payout_complete".to_string(),
                    near_sdk::serde_json::to_vec(&(id, index)).expect("serialize callback args"),
                    NearToken::from_yoctonear(0),
                    Gas::from_tgas(20),
                )
            )
    }

    /// Private callback: mark successful transfers paid, and failed transfers retryable.
    #[private]
    pub fn on_payout_complete(&mut self, id: u64, payout_index: u64) -> bool {
        let succeeded = matches!(env::promise_result(0), PromiseResult::Successful(_));
        let e = self.events.get_mut(&id).expect("no event");
        let payout = e.payouts.get_mut(payout_index as usize).expect("no payout");
        require!(payout.status == PayoutStatus::InFlight, "payout is not in flight");
        payout.status = if succeeded { PayoutStatus::Paid } else { PayoutStatus::Pending };
        succeeded
    }

    /// If the host never settles, anyone can trigger refunds after the deadline.
    pub fn refund(&mut self, id: u64) {
        let e = self.events.get_mut(&id).expect("no event");
        require!(!e.done, "already settled");
        require!(env::block_timestamp() > e.finalize_by, "too early");
        e.done = true;
        Self::refund_all(e);
    }

    pub fn get_event(&self, id: u64) -> Option<Event> {
        self.events.get(&id).cloned()
    }
}

impl Contract {
    fn refund_all(e: &mut Event) {
        e.payouts.clear();
        // Participants never deposit funds; only return the sponsor's seed.
        
        if e.host_funded.as_yoctonear() > 0 {
            e.payouts.push(Payout {
                recipient: e.host.clone(),
                amount: e.host_funded,
                status: PayoutStatus::Pending,
            });
        }
    }
}

/// Splits a sponsor-funded prize pool evenly. Any rounding remainder goes
/// to the first winner so the full pool is allocated without dust.
pub fn split_pot(total: u128, recipients: &[u128]) -> Vec<u128> {
    if recipients.is_empty() {
        return vec![];
    }
    let count = recipients.len() as u128;
    let share = total / count;
    let mut out = vec![share; recipients.len()];
    out[0] += total % count;
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::{accounts, get_created_receipts, VMContextBuilder};
    use near_sdk::testing_env;

    const N: u128 = 1_000_000_000_000_000_000_000_000;
    fn ctx(who: AccountId, dep: u128, ts: u64) {
        testing_env!(VMContextBuilder::new()
            .predecessor_account_id(who)
            .attached_deposit(NearToken::from_yoctonear(dep))
            .block_timestamp(ts)
            .build());
    }
    fn code_hash() -> Base64VecU8 {
        Base64VecU8::from(env::sha256(b"secret"))
    }
    // host = accounts(0); guests = accounts(1..)
    fn setup(min: u128, seed: u128, winners: Option<u32>) -> Contract {
        let mut c = Contract::new();
        ctx(accounts(0), seed, 1);
        c.create_event("t".into(), NearToken::from_yoctonear(min), Some(code_hash()), false, winners, 10, 20, 30);
        c
    }

    #[test]
    fn sponsor_pool_is_split_evenly_and_exactly() {
        let out = split_pot(20 * N, &[0, 0, 0]);
        assert_eq!(out, vec![20 * N / 3 + 2, 20 * N / 3, 20 * N / 3]);
        assert_eq!(out.iter().sum::<u128>(), 20 * N);
        assert_eq!(split_pot(10, &[0, 0, 0]), vec![4, 3, 3]);
    }

    #[test]
    fn split_handles_empty_and_maximum_pool() {
        assert!(split_pot(100, &[]).is_empty());
        let out = split_pot(u128::MAX, &[0, 0, 0]);
        assert_eq!(out.iter().try_fold(0u128, |sum, amount| sum.checked_add(*amount)), Some(u128::MAX));
        assert_eq!(out[1], u128::MAX / 3);
        assert_eq!(out[2], u128::MAX / 3);
    }

    #[test]
    fn host_funded_first_two_win_and_early_settle() {
        let mut c = setup(0, 10 * N, Some(2));
        for g in 1..=2 { ctx(accounts(g), 0, 5); c.register(0); }
        for g in 1..=2 { ctx(accounts(g), 0, 6); c.submit_code(0, "secret".into()); }
        ctx(accounts(0), 0, 7);
        c.finalize(0, None);
        assert!(c.get_event(0).unwrap().done);
        assert_eq!(c.get_event(0).unwrap().payouts.len(), 2);
    }

    #[test]
    #[should_panic(expected = "all prizes already claimed")]
    fn third_solver_is_rejected() {
        let mut c = setup(0, 10 * N, Some(2));
        for g in 1..=3 { ctx(accounts(g), 0, 5); c.register(0); }
        for g in 1..=3 { ctx(accounts(g), 0, 6); c.submit_code(0, "secret".into()); }
    }

    #[test]
    fn winner_can_claim_payout_and_it_enters_flight() {
        let mut c = setup(0, 10 * N, Some(1));
        ctx(accounts(1), 0, 5);
        c.register(0);
        ctx(accounts(1), 0, 6);
        c.submit_code(0, "secret".into());
        ctx(accounts(0), 0, 7);
        c.finalize(0, None);

        ctx(accounts(1), 0, 8);
        let _promise = c.claim_payout(0);
        let event = c.get_event(0).unwrap();
        assert_eq!(event.payouts[0].status, PayoutStatus::InFlight);
        // The mocked VM does not expose promise receipts consistently here;
        // the InFlight state verifies that the claim was accepted.
    }

    #[test]
    #[should_panic(expected = "winners are set by the code challenge")]
    fn host_funded_code_event_without_cap_rejects_host_selected_winners() {
        let mut c = setup(0, 10 * N, None);
        ctx(accounts(1), 0, 5);
        c.register(0);
        ctx(accounts(0), 0, 25);
        c.finalize(0, Some(vec![accounts(1)]));
    }

    #[test]
    fn uncapped_code_event_pays_only_successful_code_submitters() {
        let mut c = setup(0, 10 * N, None);
        ctx(accounts(1), 0, 5);
        c.register(0);
        ctx(accounts(1), 0, 6);
        c.submit_code(0, "secret".into());
        ctx(accounts(0), 0, 25);
        c.finalize(0, None);
        let event = c.get_event(0).unwrap();
        assert_eq!(event.payouts.len(), 1);
        assert_eq!(event.payouts[0].recipient, accounts(1));
    }

    #[test]
    #[should_panic(expected = "winner did not submit proof")]
    fn submission_event_rejects_winner_without_proof() {
        let mut c = Contract::new();
        ctx(accounts(0), 10 * N, 1);
        c.create_event("links".into(), NearToken::from_yoctonear(0), None, true, None, 10, 20, 30);
        ctx(accounts(1), 0, 5);
        c.register(0);
        ctx(accounts(0), 0, 25);
        c.finalize(0, Some(vec![accounts(1)]));
    }

    #[test]
    fn submission_event_accepts_registered_proof_submitter() {
        let mut c = Contract::new();
        ctx(accounts(0), 10 * N, 1);
        c.create_event("links".into(), NearToken::from_yoctonear(0), None, true, None, 10, 20, 30);
        ctx(accounts(1), 0, 5);
        c.register(0);
        ctx(accounts(1), 0, 6);
        c.submit(0, "https://example.com/proof".into());
        ctx(accounts(0), 0, 25);
        c.finalize(0, Some(vec![accounts(1)]));
        let event = c.get_event(0).unwrap();
        assert_eq!(event.payouts.len(), 1);
        assert_eq!(event.payouts[0].recipient, accounts(1));
    }

    #[test]
    #[should_panic(expected = "participant deposits are disabled")]
    fn event_creation_rejects_participant_stake_mode() {
        let mut c = Contract::new();
        ctx(accounts(0), 0, 1);
        c.create_event("no stakes".into(), NearToken::from_yoctonear(N), Some(code_hash()), false, None, 10, 20, 30);
    }

    #[test]
    #[should_panic(expected = "registration is free")]
    fn guest_cannot_attach_a_participant_deposit() {
        let mut c = setup(0, 10 * N, None);
        ctx(accounts(1), N, 5);
        c.register(0);
    }

    #[test]
    fn sponsor_funded_event_pays_equal_rewards() {
        let mut c = setup(0, 9 * N, None);
        for g in 1..=3 { ctx(accounts(g), 0, 5); c.register(0); }
        for g in 1..=3 { ctx(accounts(g), 0, 6); c.submit_code(0, "secret".into()); }
        ctx(accounts(0), 0, 25);
        c.finalize(0, None);
        let payouts = c.get_event(0).unwrap().payouts;
        assert_eq!(payouts.len(), 3);
        assert_eq!(payouts.iter().map(|p| p.amount.as_yoctonear()).sum::<u128>(), 9 * N);
        assert!(payouts.iter().all(|p| p.amount.as_yoctonear() == 3 * N));
    }

    #[test]
    #[should_panic(expected = "submissions still open")]
    fn host_cannot_settle_early() {
        let mut c = setup(0, 0, None);
        ctx(accounts(1), 0, 5); c.register(0);
        ctx(accounts(0), 0, 15); c.finalize(0, None);
    }

    #[test]
    fn anyone_can_refund_after_deadline() {
        let mut c = setup(0, 5 * N, None);
        for g in 1..=2 { ctx(accounts(g), 0, 5); c.register(0); }
        ctx(accounts(4), 0, 31);
        c.refund(0);
        let payouts = c.get_event(0).unwrap().payouts;
        assert_eq!(payouts.len(), 1); // sponsor seed only
        assert_eq!(payouts[0].recipient, accounts(0));
        assert_eq!(payouts[0].amount.as_yoctonear(), 5 * N);
    }

    #[test]
    #[should_panic(expected = "can't cancel after a submission is made")]
    fn host_funded_cancel_locks_after_submission() {
        let mut c = setup(0, 10 * N, None);
        ctx(accounts(1), 0, 5); c.register(0);
        ctx(accounts(1), 0, 6); c.submit_code(0, "secret".into());
        ctx(accounts(0), 0, 7); c.cancel(0);
    }

    #[test]
    #[should_panic(expected = "host can't register")]
    fn host_cannot_join_own_event() {
        let mut c = setup(0, 0, None);
        ctx(accounts(0), 0, 5); c.register(0);
    }


    #[test]
    #[should_panic(expected = "no pending payout for caller")]
    fn duplicate_claim_is_rejected_while_first_claim_is_in_flight() {
        let mut c = setup(0, 10 * N, Some(1));
        ctx(accounts(1), 0, 5);
        c.register(0);
        ctx(accounts(1), 0, 6);
        c.submit_code(0, "secret".into());
        ctx(accounts(0), 0, 7);
        c.finalize(0, None);

        ctx(accounts(1), 0, 8);
        let _first_claim = c.claim_payout(0);
        let _duplicate_claim = c.claim_payout(0);
    }
}
