use near_sdk::json_types::Base64VecU8;
use near_sdk::store::IterableMap;
use near_sdk::{env, near, require, AccountId, NearToken, PanicOnDefault, Promise};

uint::construct_uint! {
    /// 256-bit int so stake * pot can't overflow u128
    pub struct U256(4);
}

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
#[derive(Clone)]
pub struct Event {
    pub host: AccountId,
    pub title: String,
    /// 0 = host-funded (guests join free). >0 = guest-pays: each guest chooses
    /// their own amount, at least this minimum.
    pub min_deposit: NearToken,
    pub pot: NearToken,            // host seed + all guest deposits
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
    pub stakes: Vec<NearToken>,    // stakes[i] = amount guests[i] deposited
    pub completed: Vec<AccountId>,
    pub submissions: Vec<(AccountId, String)>,
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

    /// Host creates an event. Any attached NEAR becomes the host-funded pot.
    /// min_deposit > 0 makes it guest-pays (guests pick their own amount >= min).
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
        require!(
            env::block_timestamp() < register_by && register_by <= submit_by && submit_by < finalize_by,
            "bad timeline: now < register_by <= submit_by < finalize_by"
        );
        let guest_pays = min_deposit.as_yoctonear() > 0;
        // Guest-pays events must use the code challenge so winners aren't host-picked
        if guest_pays {
            require!(code_hash.is_some(), "guest-pays events need a code challenge");
        }
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
            done: false,
        });
        id
    }

    /// Guest registers. Guest-pays: attach any amount >= min_deposit (your stake).
    /// Host-funded: attach nothing.
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
        if e.min_deposit.as_yoctonear() > 0 {
            require!(amount >= e.min_deposit, "deposit below the minimum");
        } else {
            require!(amount.as_yoctonear() == 0, "this event is free to join");
        }
        e.pot = e.pot.saturating_add(amount);
        e.guests.push(me);
        e.stakes.push(amount);
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

    /// Host settles, only inside the window (submit_by, finalize_by]
    /// (or earlier if all first-come-first-served prizes are claimed).
    /// Guest-pays: winners = code completers; the whole pot is shared in
    /// proportion to each winner's own stake. Host-funded: even split among
    /// winners (host-picked, or code completers / first N). No winners =
    /// everyone is refunded their own stake.
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
        // Guest-pays, or code event with a winner limit: the contract decides (first N)
        if e.min_deposit.as_yoctonear() > 0 || (code_event && e.num_winners.is_some()) {
            require!(winners.is_none(), "winners are set by the code challenge");
        }
        let winners = winners.unwrap_or_else(|| e.completed.clone());
        if let Some(n) = e.num_winners {
            require!(winners.len() as u32 <= n, "more winners than the event allows");
        }
        let mut stakes: Vec<u128> = vec![];
        for (i, w) in winners.iter().enumerate() {
            let idx = e.guests.iter().position(|g| g == w).expect("winner not registered");
            require!(!winners[..i].contains(w), "duplicate winner");
            stakes.push(e.stakes[idx].as_yoctonear());
        }
        e.done = true;
        if winners.is_empty() {
            Self::refund_all(e);
            return;
        }
        let total = e.pot.as_yoctonear();
        let amounts = split_pot(total, &stakes);
        for (w, amt) in winners.into_iter().zip(amounts) {
            if amt > 0 {
                Promise::new(w).transfer(NearToken::from_yoctonear(amt));
            }
        }
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
    fn refund_all(e: &Event) {
        for (g, s) in e.guests.iter().zip(e.stakes.iter()) {
            if s.as_yoctonear() > 0 {
                Promise::new(g.clone()).transfer(*s);
            }
        }
        if e.host_funded.as_yoctonear() > 0 {
            Promise::new(e.host.clone()).transfer(e.host_funded);
        }
    }
}

/// Splits `total` among winners in proportion to their stakes (even
/// split if all stakes are 0). The first winner gets the remainder, so nothing
/// is left behind.
pub fn split_pot(total: u128, stakes: &[u128]) -> Vec<u128> {
    let sum: u128 = stakes.iter().sum();
    let n = stakes.len() as u128;
    let mut out: Vec<u128> = stakes
        .iter()
        .map(|s| {
            if sum == 0 { total / n } else { (U256::from(total) * U256::from(*s) / U256::from(sum)).as_u128() }
        })
        .collect();
    let others: u128 = out[1..].iter().sum();
    out[0] = total - others;
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
    fn split_is_proportional_and_exact() {
        let out = split_pot(20 * N, &[N, 3 * N, 6 * N]);
        assert_eq!(out.iter().sum::<u128>(), 20 * N);
        assert_eq!(out[1], 6 * N);
        assert_eq!(out[2], 12 * N);
        assert_eq!(split_pot(10, &[0, 0, 0]).iter().sum::<u128>(), 10);
    }

    #[test]
    fn host_funded_first_two_win_and_early_settle() {
        let mut c = setup(0, 10 * N, Some(2));
        for g in 1..=2 { ctx(accounts(g), 0, 5); c.register(0); }
        for g in 1..=2 { ctx(accounts(g), 0, 6); c.submit_code(0, "secret".into()); }
        ctx(accounts(0), 0, 7);
        c.finalize(0, None);
        assert!(c.get_event(0).unwrap().done);
        assert_eq!(get_created_receipts().len(), 2);
    }

    #[test]
    #[should_panic(expected = "all prizes already claimed")]
    fn third_solver_is_rejected() {
        let mut c = setup(0, 10 * N, Some(2));
        for g in 1..=3 { ctx(accounts(g), 0, 5); c.register(0); }
        for g in 1..=3 { ctx(accounts(g), 0, 6); c.submit_code(0, "secret".into()); }
    }

    #[test]
    fn guest_pays_settles_after_window() {
        let mut c = setup(N, 0, None);
        for (g, stake) in [(1, N), (2, 3 * N), (3, 6 * N)] { ctx(accounts(g), stake, 5); c.register(0); }
        for g in 1..=2 { ctx(accounts(g), 0, 15); c.submit_code(0, "secret".into()); }
        ctx(accounts(0), 0, 25);
        c.finalize(0, None);
        assert_eq!(get_created_receipts().len(), 2);
    }

    #[test]
    #[should_panic(expected = "submissions still open")]
    fn host_cannot_settle_early() {
        let mut c = setup(N, 0, None);
        ctx(accounts(1), N, 5); c.register(0);
        ctx(accounts(0), 0, 15); c.finalize(0, None);
    }

    #[test]
    fn anyone_can_refund_after_deadline() {
        let mut c = setup(N, 5 * N, None);
        for g in 1..=2 { ctx(accounts(g), N, 5); c.register(0); }
        ctx(accounts(4), 0, 31);
        c.refund(0);
        assert_eq!(get_created_receipts().len(), 3); // 2 guests + host seed
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
        let mut c = setup(N, 0, None);
        ctx(accounts(0), N, 5); c.register(0);
    }
}
