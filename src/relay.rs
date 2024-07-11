use crate::nostr::{Condition, Event, EventId, Filter, PubKey, SingleLetterTags};
use crate::priority_queue::PriorityQueue;
use rustc_hash::{FxHashMap, FxHashSet};
use std::borrow::Cow;
use std::collections::BTreeSet;
use std::fmt::Debug;
use std::str::FromStr;
use std::sync::Arc;

type UnixTime = u64;

#[derive(Debug)]
pub struct Db {
    pub next_n: u64,
    pub n_to_event: FxHashMap<u64, Arc<Event>>,
    pub conditions: BTreeSet<(Cow<'static, Condition>, Time)>,
    pub time: BTreeSet<Time>,
    pub deleted: FxHashSet<EventId>,
    pub blocked_pubkeys: FxHashSet<PubKey>,
}

impl Default for Db {
    fn default() -> Self {
        Self {
            next_n: 1,
            n_to_event: Default::default(),
            conditions: Default::default(),
            time: Default::default(),
            deleted: Default::default(),
            blocked_pubkeys: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum AddEventError {
    Duplicated,
    HaveNewer,
}

impl Db {
    pub fn id_to_n(&self, id: EventId) -> Option<u64> {
        let id = Condition::Id(id);
        Some(
            self.conditions
                .range((Cow::Borrowed(&id), Time::ZERO)..(Cow::Borrowed(&id), Time::MAX))
                .next()?
                .1
                 .1,
        )
    }

    pub fn add_event(&mut self, event: Arc<Event>) -> Result<u64, AddEventError> {
        if self.id_to_n(event.id).is_some() {
            return Err(AddEventError::Duplicated);
        }
        match event.kind {
            0 | 3 | 10000..=19999 => {
                let author = Condition::Author(event.pubkey);
                let kind = Condition::Kind(event.kind);
                let mut i = GetEvents {
                    until: Time(u64::MAX, u64::MAX),
                    and_conditions: PriorityQueue::from([
                        (u64::MAX, ConditionsWithLatest::new(vec![&author])),
                        (u64::MAX - 1, ConditionsWithLatest::new(vec![&kind])),
                    ]),
                };
                if let Some(Time(t, n)) = i.next(self) {
                    let have_newer = match t.cmp(&event.created_at) {
                        std::cmp::Ordering::Less => false,
                        std::cmp::Ordering::Equal => {
                            let e = &self.n_to_event[&n];
                            e.id > event.id
                        }
                        std::cmp::Ordering::Greater => true,
                    };
                    if have_newer {
                        return Err(AddEventError::HaveNewer);
                    } else {
                        self.remove_event(n);
                    }
                }
            }
            30000..=39999 => {
                fn first_d_value(event: &Event) -> Option<&str> {
                    event.tags.iter().find_map(|t| {
                        if t.first()? == "d" {
                            Some(t.get(1).map(|a| a.as_str()).unwrap_or(""))
                        } else {
                            None
                        }
                    })
                }
                if let Some(d_value) = first_d_value(&event) {
                    let author = Condition::Author(event.pubkey);
                    let kind = Condition::Kind(event.kind);
                    let d_tag = Condition::Tag('d', d_value.to_string());
                    let mut i = GetEvents {
                        until: Time(u64::MAX, u64::MAX),
                        and_conditions: PriorityQueue::from([
                            (u64::MAX, ConditionsWithLatest::new(vec![&author])),
                            (u64::MAX - 1, ConditionsWithLatest::new(vec![&kind])),
                            (u64::MAX - 1, ConditionsWithLatest::new(vec![&d_tag])),
                        ]),
                    };
                    while let Some(Time(t, n)) = i.next(self) {
                        if first_d_value(&self.n_to_event[&n]).map_or(false, |d| d == d_value) {
                            let have_newer = match t.cmp(&event.created_at) {
                                std::cmp::Ordering::Less => false,
                                std::cmp::Ordering::Equal => {
                                    let e = &self.n_to_event[&n];
                                    e.id > event.id
                                }
                                std::cmp::Ordering::Greater => true,
                            };
                            if have_newer {
                                return Err(AddEventError::HaveNewer);
                            } else {
                                self.remove_event(n);
                                break;
                            }
                        }
                    }
                }
            }
            5 => {
                for t in &event.tags {
                    if let (Some(t), Some(v)) = (t.first(), t.get(1)) {
                        if let ("e", Ok(e)) = (t.as_ref(), EventId::from_str(v.as_ref())) {
                            self.deleted.insert(e);
                            if let Some(n) = self.id_to_n(e) {
                                self.remove_event(n);
                            }
                        }
                    }
                }
            }
            _ => (),
        }
        let n = self.next_n;
        self.next_n = n + 1;
        for (tag, value) in SingleLetterTags::new(&event.tags) {
            self.conditions.insert((
                Cow::Owned(Condition::Tag(tag, value)),
                Time(event.created_at, n),
            ));
        }
        self.conditions.insert((
            Cow::Owned(Condition::Author(event.pubkey)),
            Time(event.created_at, n),
        ));
        self.conditions.insert((
            Cow::Owned(Condition::Id(event.id)),
            Time(event.created_at, n),
        ));
        self.conditions.insert((
            Cow::Owned(Condition::Kind(event.kind)),
            Time(event.created_at, n),
        ));
        self.time.insert(Time(event.created_at, n));
        self.n_to_event.insert(n, event);
        Ok(n)
    }

    pub fn remove_event(&mut self, n: u64) -> bool {
        if let Some(event) = self.n_to_event.remove(&n) {
            for (tag, value) in SingleLetterTags::new(&event.tags) {
                self.conditions.remove(&(
                    Cow::Owned(Condition::Tag(tag, value)),
                    Time(event.created_at, n),
                ));
            }
            self.conditions.remove(&(
                Cow::Owned(Condition::Author(event.pubkey)),
                Time(event.created_at, n),
            ));
            self.conditions.remove(&(
                Cow::Owned(Condition::Id(event.id)),
                Time(event.created_at, n),
            ));
            self.conditions.remove(&(
                Cow::Owned(Condition::Kind(event.kind)),
                Time(event.created_at, n),
            ));
            self.time.remove(&Time(event.created_at, n));
            true
        } else {
            false
        }
    }
}

#[derive(Debug)]
struct ConditionsWithLatest<'a> {
    cs: PriorityQueue<Time, &'a Condition>,
    remained: Vec<&'a Condition>,
}

impl<'a> ConditionsWithLatest<'a> {
    fn new(conditions: Vec<&'a Condition>) -> Self {
        ConditionsWithLatest {
            cs: PriorityQueue::new(),
            remained: conditions,
        }
    }

    fn next(&mut self, db: &Db, until: Time) -> Option<Time> {
        while let Some(c) = self.remained.pop() {
            if let Some((_, t)) = db
                .conditions
                .range((Cow::Borrowed(c), Time::ZERO)..=(Cow::Borrowed(c), until))
                .next_back()
            {
                self.cs.push(*t, c);
            }
        }
        while let Some((t, c)) = self.cs.pop() {
            if t <= until {
                self.remained.push(c);
                return Some(t);
            }
            if let Some((_, t)) = db
                .conditions
                .range((Cow::Borrowed(c), Time::ZERO)..=(Cow::Borrowed(c), until))
                .next_back()
            {
                self.cs.push(*t, c);
            }
        }
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Time(pub UnixTime, pub u64);

impl Time {
    fn minus(self, other: Self) -> (u64, u64) {
        let time_diff = self.0 - other.0;
        if time_diff == 0 {
            (time_diff, self.1 - other.1)
        } else {
            (time_diff, 0)
        }
    }

    fn pred(self) -> Self {
        if self.1 == 0 {
            Self(self.0 - 1, u64::MAX)
        } else {
            Self(self.0, self.1 - 1)
        }
    }

    const MAX: Self = Self(u64::MAX, u64::MAX);
    const ZERO: Self = Self(0, 0);
}

#[derive(Debug)]
pub struct GetEvents<'a> {
    until: Time,
    and_conditions: PriorityQueue<u64, ConditionsWithLatest<'a>>,
}

impl<'a> GetEvents<'a> {
    pub fn new(filter: &'a Filter) -> Option<Self> {
        let mut and_conditions = PriorityQueue::with_capacity(filter.conditions.len());
        for cs in &filter.conditions {
            if cs.is_empty() {
                return None;
            }
            and_conditions.push(u64::MAX, ConditionsWithLatest::new(cs.iter().collect()));
        }
        Some(GetEvents {
            until: Time(filter.until, u64::MAX),
            and_conditions,
        })
    }

    pub fn next(&mut self, db: &Db) -> Option<Time> {
        if self.until == Time::ZERO {
            return None;
        }
        if self.and_conditions.is_empty() {
            return if let Some(t) = db.time.range(Time::ZERO..=self.until).next_back() {
                self.until = t.pred();
                Some(*t)
            } else {
                None
            };
        }
        loop {
            let mut next_and_conditions = PriorityQueue::with_capacity(self.and_conditions.len());
            let mut first = true;
            let mut all = true;
            while let Some((p_diff, mut cs)) = self.and_conditions.pop() {
                if let Some(t) = cs.next(db, self.until) {
                    let diff = self.until.minus(t);
                    self.until = t;
                    if first {
                        first = false;
                    } else if diff != (0, 0) {
                        all = false;
                    }
                    let c_len = (cs.cs.len() + cs.remained.len()) as u64;
                    if c_len == 0 {
                        return None;
                    }
                    next_and_conditions.push(diff.0 / (c_len * 2) + p_diff / 2, cs);
                } else {
                    return None;
                }
            }
            self.and_conditions = next_and_conditions;
            if all {
                let c = self.until;
                self.until = c.pred();
                return Some(c);
            }
        }
    }
}
