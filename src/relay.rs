use crate::nostr::{Condition, Event, EventId, Filter, SingleLetterTags};
use crate::priority_queue::PriorityQueue;
use log::trace;
use parking_lot::RwLock;
use smallvec::SmallVec;
use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap};
use std::fmt::Debug;
use std::str::FromStr;
use std::sync::Arc;

pub struct QueryIter<'a, 'b>(QueryIterInner<'a, 'b>);

enum QueryIterInner<'a, 'b> {
    All {
        n: u64,
        db: &'a Db,
        since: UnixTime,
        until: UnixTime,
        limit: u32,
    },
    Filter {
        or_conditions: PriorityQueue<Time, GetEvents<'b>>,
        db: &'a Db,
    },
}

impl<'a, 'b> QueryIter<'a, 'b> {
    pub fn new(db: &'a Db, filters: &'b SmallVec<[Filter; 2]>) -> Self {
        let mut or_conditions = PriorityQueue::with_capacity(filters.len());
        for f in filters {
            if f.conditions.is_empty() {
                return QueryIter(QueryIterInner::All {
                    n: u64::MAX,
                    db,
                    since: f.since,
                    until: f.until,
                    limit: f.limit,
                });
            }
            if let Some(mut s) = GetEvents::new(f) {
                if let Some(a) = s.next(db) {
                    or_conditions.push(a, s);
                }
            }
        }
        QueryIter(QueryIterInner::Filter { or_conditions, db })
    }
}

impl Iterator for QueryIter<'_, '_> {
    type Item = Time;

    fn next(&mut self) -> Option<Self::Item> {
        trace!("QueryIter::next");
        match &mut self.0 {
            QueryIterInner::All {
                n,
                db,
                since,
                until,
                limit,
            } => {
                if *limit == 0 {
                    None
                } else if let Some(a) = db
                    .time
                    .read()
                    .range(Time(*since, 0)..=Time(*until, *n))
                    .next_back()
                {
                    *limit -= 1;
                    *until = a.0;
                    *n = a.1 - 1;
                    Some(*a)
                } else {
                    None
                }
            }
            QueryIterInner::Filter { or_conditions, db } => {
                if let Some((t, mut s)) = or_conditions.pop() {
                    if let Some(a) = s.next(db) {
                        or_conditions.push(a, s);
                    }
                    Some(t)
                } else {
                    None
                }
            }
        }
    }
}

type UnixTime = u64;

#[derive(Debug, Default)]
pub struct Db {
    pub n_to_event: RwLock<HashMap<u64, Arc<Event>>>,
    pub conditions: RwLock<BTreeSet<(Cow<'static, Condition>, Time)>>,
    pub time: RwLock<BTreeSet<Time>>,
}

impl Db {
    pub fn id_to_n(&self, id: EventId) -> Option<u64> {
        let id = Condition::Id(id);
        Some(
            self.conditions
                .read()
                .range((Cow::Borrowed(&id), Time::ZERO)..(Cow::Borrowed(&id), Time::MAX))
                .next()?
                .1
                 .1,
        )
    }

    pub fn add_event(&self, event: Arc<Event>) -> (bool, u64) {
        let n = self.n_to_event.read().len() as u64 + 1;
        if let Some(n) = self.id_to_n(event.id) {
            return (true, n);
        }
        {
            let cs = &mut self.conditions.write();
            for (tag, value) in SingleLetterTags::new(&event.tags) {
                cs.insert((
                    Cow::Owned(Condition::Tag(tag, value)),
                    Time(event.created_at, n),
                ));
            }
            cs.insert((
                Cow::Owned(Condition::Author(event.pubkey)),
                Time(event.created_at, n),
            ));
            cs.insert((
                Cow::Owned(Condition::Id(event.id)),
                Time(event.created_at, n),
            ));
            cs.insert((
                Cow::Owned(Condition::Kind(event.kind)),
                Time(event.created_at, n),
            ));
        }
        self.time.write().insert(Time(event.created_at, n));
        if event.kind == 5 {
            for t in &event.tags {
                if let (Some(t), Some(v)) = (t.first(), t.get(1)) {
                    if let ("e", Ok(e)) = (t.as_ref(), EventId::from_str(v.as_ref())) {
                        if let Some(n) = self.id_to_n(e) {
                            self.remove_event(n);
                        }
                    }
                }
            }
        }
        self.n_to_event.write().insert(n, event);
        (false, n)
    }

    pub fn remove_event(&self, n: u64) -> bool {
        if let Some(event) = self.n_to_event.write().remove(&n) {
            {
                let cs = &mut self.conditions.write();
                for (tag, value) in SingleLetterTags::new(&event.tags) {
                    cs.remove(&(
                        Cow::Owned(Condition::Tag(tag, value)),
                        Time(event.created_at, n),
                    ));
                }
                cs.remove(&(
                    Cow::Owned(Condition::Author(event.pubkey)),
                    Time(event.created_at, n),
                ));
                cs.remove(&(
                    Cow::Owned(Condition::Id(event.id)),
                    Time(event.created_at, n),
                ));
                cs.remove(&(
                    Cow::Owned(Condition::Kind(event.kind)),
                    Time(event.created_at, n),
                ));
            }
            self.time.write().remove(&Time(event.created_at, n));
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

    fn next(&mut self, db: &Db, since: UnixTime, until: Time) -> Option<Time> {
        let conditions = db.conditions.read();
        while let Some(c) = self.remained.pop() {
            if let Some((_, t)) = conditions
                .range((Cow::Borrowed(c), Time(since, 0))..=(Cow::Borrowed(c), until))
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
            if let Some((_, t)) = conditions
                .range((Cow::Borrowed(c), Time(since, 0))..=(Cow::Borrowed(c), until))
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
struct GetEvents<'a> {
    since: UnixTime,
    until: Time,
    limit: u32,
    and_conditions: PriorityQueue<u64, ConditionsWithLatest<'a>>,
}

impl<'a> GetEvents<'a> {
    fn new(filter: &'a Filter) -> Option<Self> {
        let mut and_conditions = PriorityQueue::with_capacity(filter.conditions.len());
        for cs in &filter.conditions {
            if cs.is_empty() {
                return None;
            }
            and_conditions.push(u64::MAX, ConditionsWithLatest::new(cs.iter().collect()));
        }
        Some(GetEvents {
            since: filter.since,
            until: Time(filter.until, u64::MAX),
            and_conditions,
            limit: filter.limit,
        })
    }

    fn next(&mut self, db: &Db) -> Option<Time> {
        if self.limit == 0 || self.until == Time::ZERO || self.and_conditions.is_empty() {
            return None;
        }
        self.limit -= 1;
        loop {
            let mut next_conditions = PriorityQueue::with_capacity(self.and_conditions.len());
            let mut first = true;
            let mut all = true;
            while let Some((p_diff, mut cs)) = self.and_conditions.pop() {
                if let Some(t) = cs.next(db, self.since, self.until) {
                    let diff = self.until.minus(t);
                    self.until = t;
                    if first {
                        first = false;
                    } else if diff != (0, 0) {
                        all = false;
                    }
                    let c_len = (cs.cs.len() + cs.remained.len()) as u64;
                    if c_len != 0 {
                        next_conditions.push(diff.0 / (c_len * 2) + p_diff / 2, cs);
                    }
                } else {
                    return None;
                }
            }
            self.and_conditions = next_conditions;
            if all {
                let c = self.until;
                self.until = c.pred();
                return Some(c);
            }
            if self.and_conditions.is_empty() {
                break None;
            }
        }
    }
}
