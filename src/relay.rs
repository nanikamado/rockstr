use crate::nostr::{Condition, Event, EventId, Filter};
use core::slice;
use parking_lot::RwLock;
use smallvec::SmallVec;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Debug;
use std::str::FromStr;

pub struct QueryIter<'a>(QueryIterInner<'a>);

enum QueryIterInner<'a> {
    All {
        n: u64,
        db: &'a Db,
        since: Time,
        until: Time,
        limit: u32,
    },
    Filter {
        m: BTreeMap<(u64, u64), GetEvents>,
        db: &'a Db,
    },
}

impl<'a> QueryIter<'a> {
    pub fn new(db: &'a Db, filters: SmallVec<[Filter; 2]>) -> Self {
        let mut m = BTreeMap::new();
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
                if let Some(a) = s.next(&db) {
                    m.insert(a, s);
                }
            }
        }
        QueryIter(QueryIterInner::Filter { m: m, db })
    }
}

impl Iterator for QueryIter<'_> {
    type Item = (Time, u64);

    fn next(&mut self) -> Option<Self::Item> {
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
                } else if let Some(a) = db.time.read().range((*since, 0)..=(*until, *n)).next_back()
                {
                    *limit -= 1;
                    *until = a.0;
                    *n = a.1 - 1;
                    Some(*a)
                } else {
                    None
                }
            }
            QueryIterInner::Filter { m, db } => {
                if let Some(((t, n), mut s)) = m.pop_last() {
                    if let Some(a) = s.next(&db) {
                        m.insert(a, s);
                    }
                    Some((t, n))
                } else {
                    None
                }
            }
        }
    }
}

type Time = u64;

#[derive(Debug, Default)]
pub struct Db {
    pub id_to_n: RwLock<HashMap<EventId, u64>>,
    pub n_to_event: RwLock<HashMap<u64, Event>>,
    pub conditions: RwLock<BTreeSet<(Condition, Time, u64)>>,
    pub time: RwLock<BTreeSet<(Time, u64)>>,
}

impl Db {
    pub fn add_event(&self, event: Event) -> Option<u64> {
        if !event.verify() {
            return None;
        }
        use std::collections::hash_map::Entry::*;
        let n = self.id_to_n.read().len() as u64 + 1;
        let n = match self.id_to_n.write().entry(event.id) {
            Occupied(e) => return Some(*e.get()),
            Vacant(e) => {
                e.insert(n);
                n
            }
        };
        {
            let cs = &mut self.conditions.write();
            for (tag, value) in SingleLetterTags(event.tags.iter()) {
                cs.insert((Condition::Tag(tag, value), event.created_at, n));
            }
            cs.insert((Condition::Author(event.pubkey), event.created_at, n));
            cs.insert((Condition::Kind(event.kind), event.created_at, n));
        }
        self.time.write().insert((event.created_at, n));
        if event.kind == 5 {
            for t in &event.tags {
                if let (Some(t), Some(v)) = (t.get(0), t.get(1)) {
                    if let ("e", Ok(e)) = (t.as_ref(), EventId::from_str(v.as_ref())) {
                        if let Some(n) = self.id_to_n.write().get(&e) {
                            self.remove_event(*n);
                        }
                    }
                }
            }
        }
        self.n_to_event.write().insert(n, event);
        Some(n)
    }

    pub fn remove_event(&self, n: u64) -> bool {
        if let Some(event) = self.n_to_event.write().remove(&n) {
            self.id_to_n.write().remove(&event.id);
            {
                let cs = &mut self.conditions.write();
                for (tag, value) in SingleLetterTags(event.tags.iter()) {
                    cs.remove(&(Condition::Tag(tag, value), event.created_at, n));
                }
                cs.remove(&(Condition::Author(event.pubkey), event.created_at, n));
                cs.remove(&(Condition::Kind(event.kind), event.created_at, n));
            }
            self.time.write().remove(&(event.created_at, n));
            true
        } else {
            false
        }
    }
}

struct SingleLetterTags<'a>(slice::Iter<'a, SmallVec<[String; 3]>>);

impl Iterator for SingleLetterTags<'_> {
    type Item = (char, String);

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(t) = self.0.next() {
            if let Some(tag) = t.get(0) {
                let mut cs = tag.chars();
                if let (Some(tag), None) = (cs.next(), cs.next()) {
                    return Some((tag, t.get(1).map(|a| a.to_string()).unwrap_or_default()));
                }
            }
        }
        None
    }
}

#[derive(Debug)]
struct ConditionsWithLatest {
    cs: BTreeMap<(Time, u64), Condition>,
    remained: Vec<Condition>,
}

impl ConditionsWithLatest {
    fn new(conditions: Vec<Condition>) -> Self {
        ConditionsWithLatest {
            cs: BTreeMap::new(),
            remained: conditions,
        }
    }

    fn next(&mut self, db: &Db, since: Time, until: Time, n: u64) -> Option<(Time, u64)> {
        let conditions = db.conditions.read();
        for c in &self.remained {
            if let Some((_, t, n)) = conditions
                .range((c.clone(), since, 0)..=(c.clone(), until, n))
                .next_back()
            {
                self.cs.insert((*t, *n), c.clone());
            }
        }
        while let Some(((t, cn), c)) = self.cs.pop_last() {
            if t <= until && cn <= n {
                self.remained.push(c);
                return Some((t, cn));
            }
            if let Some((_, t, n)) = conditions
                .range((c.clone(), since, 0)..=(c.clone(), until, n))
                .next_back()
            {
                self.cs.insert((*t, *n), c);
            }
        }
        None
    }
}

#[derive(Debug)]
struct GetEvents {
    since: Time,
    until: Time,
    n: u64,
    limit: u32,
    conditions: BTreeMap<(u64, u64), ConditionsWithLatest>,
}

impl GetEvents {
    fn new(filter: Filter) -> Option<Self> {
        let mut conditions = BTreeMap::new();
        for cs in filter.conditions {
            if cs.is_empty() {
                return None;
            }
            conditions.insert((u64::MAX, u64::MAX - 1), ConditionsWithLatest::new(cs));
        }
        Some(GetEvents {
            since: filter.since,
            until: filter.until,
            n: u64::MAX,
            conditions,
            limit: filter.limit,
        })
    }

    fn next(&mut self, db: &Db) -> Option<(Time, u64)> {
        if self.limit == 0 || self.n == 0 || self.conditions.is_empty() {
            return None;
        }
        self.limit -= 1;
        loop {
            let mut next_conditions = BTreeMap::new();
            let mut first = true;
            let mut all = true;
            while let Some(((p_diff, _), mut cs)) = self.conditions.pop_last() {
                if let Some((t, n)) = cs.next(db, self.since, self.until, self.n) {
                    let diff = (self.until - t, self.n - n);
                    self.until = t;
                    self.n = n;
                    if first {
                        first = false;
                    } else if diff != (0, 0) {
                        all = false;
                    }
                    let c_len = (cs.cs.len() + cs.remained.len()) as u64;
                    if c_len != 0 {
                        next_conditions.insert((diff.0 / (c_len * 2) + p_diff / 2, diff.1), cs);
                    }
                } else {
                    return None;
                }
            }
            self.conditions = next_conditions;
            if all {
                let n = self.n;
                self.n -= 1;
                return Some((self.until, n));
            }
            if self.conditions.is_empty() {
                break None;
            }
        }
    }
}
