use crate::nostr::{Condition, Event, EventId, Filter, FirstTagValue, PubKey, SingleLetterTags};
use crate::priority_queue::PriorityQueue;
use bitcoin_hashes::hex::DisplayHex;
use ordered_float::OrderedFloat;
use rocksdb::{DBIteratorWithThreadMode as RocksIter, IteratorMode, DB as Rocks};
use rustc_hash::FxHashSet;
use std::fmt::Debug;
use std::fs::create_dir_all;
use std::io;
use std::path::Path;
use std::sync::Arc;

type UnixTime = u64;

#[derive(Debug)]
pub struct Db {
    pub next_n: u64,
    // u64 -> event json
    pub n_to_event: Rocks,
    // condition time -> []
    pub conditions: Rocks,
    // time -> []
    pub time: Rocks,
    // event id -> []
    pub deleted: Rocks,
    pub blocked_pubkeys: FxHashSet<PubKey>,
}

impl Default for Db {
    fn default() -> Self {
        let config_dir = Path::new("rockstr");
        create_dir_all(config_dir).unwrap();
        let mut opts = rocksdb::Options::default();
        opts.create_if_missing(true);
        let conditions = Rocks::open(&opts, config_dir.join("conditions.rocksdb")).unwrap();
        let n_to_event = Rocks::open(&opts, config_dir.join("n_to_event.rocksdb")).unwrap();
        let time = Rocks::open(&opts, config_dir.join("time.rocksdb")).unwrap();
        let deleted = Rocks::open(&opts, config_dir.join("deleted.rocksdb")).unwrap();
        let next_n = if let Some(a) = n_to_event.iterator(rocksdb::IteratorMode::End).next() {
            u64::from_be_bytes(a.unwrap().0.as_ref().try_into().unwrap()) + 1
        } else {
            1
        };
        Self {
            next_n,
            n_to_event,
            conditions,
            time,
            deleted,
            blocked_pubkeys: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum AddEventError {
    Duplicated,
    HaveNewer,
}

fn key_to_vec(condition: &Condition, time: Time) -> Vec<u8> {
    let mut buff = Vec::with_capacity(16);
    condition.write_bytes(&mut buff).unwrap();
    time.write_bytes(&mut buff).unwrap();
    buff
}

fn prefix_iterator<'a>(
    db: &'a Rocks,
    prefix: &'a [u8],
) -> impl Iterator<Item = (Box<[u8]>, Box<[u8]>)> + 'a {
    db.iterator(IteratorMode::From(prefix, rocksdb::Direction::Forward))
        .map(|a| a.unwrap())
        .take_while(|(a, _)| a.starts_with(prefix))
}

impl Db {
    pub fn n_to_event_get(&self, n: u64) -> Option<Event> {
        let s = self.n_to_event.get(n.to_be_bytes()).unwrap()?;
        Some(serde_json::from_slice(&s).unwrap())
    }

    pub fn n_to_event_remove(&self, n: u64) {
        self.n_to_event.delete(n.to_be_bytes()).unwrap();
    }

    pub fn n_to_event_insert(&self, n: u64, event: &Event) {
        self.n_to_event
            .put(n.to_be_bytes(), serde_json::to_vec(event).unwrap())
            .unwrap();
    }

    pub fn time_get(&self, time: Time) -> Event {
        let s = self.time.get(time.to_vec()).unwrap().unwrap();
        serde_json::from_slice(&s).unwrap()
    }

    pub fn time_remove(&self, time: Time) {
        self.time.delete(time.to_vec()).unwrap();
    }

    pub fn time_insert(&self, time: Time) {
        self.time.put(time.to_vec(), []).unwrap();
    }

    pub fn is_deleted(&self, id: &EventId) -> bool {
        self.deleted.get(id.as_byte_array()).unwrap().is_some()
    }

    pub fn deleted_insert(&self, id: &EventId) {
        self.deleted.put(id.as_byte_array(), []).unwrap()
    }

    pub fn contains(&self, id: EventId) -> bool {
        let id = Condition::Id(id);
        let p = id.to_vec();
        let mut i = prefix_iterator(&self.conditions, &p);
        i.next().is_some()
    }

    pub fn id_to_n(&self, id: EventId) -> Option<u64> {
        let id = Condition::Id(id);
        let p = id.to_vec();
        let s = prefix_iterator(&self.conditions, &p).next()?.0;
        let s = s[p.len() + 8..].try_into().unwrap();
        Some(u64::from_be_bytes(s))
    }

    pub fn add_event(&mut self, event: Arc<Event>) -> Result<u64, AddEventError> {
        if self.contains(event.id) {
            return Err(AddEventError::Duplicated);
        }
        match event.kind {
            0 | 3 | 10000..20000 => {
                let author = Condition::Author(event.pubkey);
                let kind = Condition::Kind(event.kind);
                let mut outdated = Vec::new();
                {
                    let mut i = GetEvents {
                        until: Time(u64::MAX, u64::MAX),
                        and_conditions: PriorityQueue::from([
                            (
                                OrderedFloat(100.),
                                ConditionsWithLatest::new(vec![&author], self, 0),
                            ),
                            (
                                OrderedFloat(0.),
                                ConditionsWithLatest::new(vec![&kind], self, 1),
                            ),
                        ]),
                    };
                    if let Some(Time(t, n)) = i.next(self) {
                        let have_newer = match t.cmp(&event.created_at) {
                            std::cmp::Ordering::Less => false,
                            std::cmp::Ordering::Equal => {
                                let e = &self.n_to_event_get(n).unwrap();
                                e.id > event.id
                            }
                            std::cmp::Ordering::Greater => true,
                        };
                        if have_newer {
                            return Err(AddEventError::HaveNewer);
                        } else {
                            outdated.push(n);
                        }
                    }
                }
                for n in outdated {
                    self.remove_event(n);
                }
            }
            30000..40000 => {
                fn first_d_value(event: &Event) -> Option<&FirstTagValue> {
                    event.tags.iter().find_map(|t| {
                        if t.0 == "d" {
                            t.1.as_ref().map(|(a, _)| a)
                        } else {
                            None
                        }
                    })
                }
                if let Some(d_value) = first_d_value(&event) {
                    let author = Condition::Author(event.pubkey);
                    let kind = Condition::Kind(event.kind);
                    let d_tag = Condition::Tag(b'd', d_value.clone());
                    let mut outdated = Vec::new();
                    {
                        let mut i = GetEvents {
                            until: Time(u64::MAX, u64::MAX),
                            and_conditions: PriorityQueue::from([
                                (
                                    OrderedFloat(200.),
                                    ConditionsWithLatest::new(vec![&d_tag], self, 2),
                                ),
                                (
                                    OrderedFloat(100.),
                                    ConditionsWithLatest::new(vec![&author], self, 0),
                                ),
                                (
                                    OrderedFloat(0.),
                                    ConditionsWithLatest::new(vec![&kind], self, 1),
                                ),
                            ]),
                        };
                        while let Some(Time(t, n)) = i.next(self) {
                            let e = self.n_to_event_get(n).unwrap();
                            if first_d_value(&e).map_or(false, |d| d == d_value) {
                                let have_newer = match t.cmp(&event.created_at) {
                                    std::cmp::Ordering::Less => false,
                                    std::cmp::Ordering::Equal => e.id > event.id,
                                    std::cmp::Ordering::Greater => true,
                                };
                                if have_newer {
                                    return Err(AddEventError::HaveNewer);
                                } else {
                                    outdated.push(n);
                                    break;
                                }
                            }
                        }
                    }
                    for n in outdated {
                        self.remove_event(n);
                    }
                }
            }
            5 => {
                for t in &event.tags {
                    if let ("e", Some((FirstTagValue::Hex32(v), _))) = (t.0.as_ref(), &t.1) {
                        let e = EventId::from(*v);
                        self.deleted_insert(&e);
                        if let Some(n) = self.id_to_n(e) {
                            self.remove_event(n);
                        }
                    }
                }
            }
            _ => (),
        }
        let n = self.next_n;
        self.next_n = n + 1;
        for (tag, value) in SingleLetterTags::new(&event.tags) {
            self.conditions
                .put(
                    key_to_vec(
                        &Condition::Tag(tag, value.clone()),
                        Time(event.created_at, n),
                    ),
                    [],
                )
                .unwrap();
        }
        self.conditions
            .put(
                key_to_vec(&Condition::Author(event.pubkey), Time(event.created_at, n)),
                [],
            )
            .unwrap();
        self.conditions
            .put(
                key_to_vec(&Condition::Id(event.id), Time(event.created_at, n)),
                [],
            )
            .unwrap();
        self.conditions
            .put(
                key_to_vec(&Condition::Kind(event.kind), Time(event.created_at, n)),
                [],
            )
            .unwrap();
        self.time_insert(Time(event.created_at, n));
        self.n_to_event_insert(n, &event);
        Ok(n)
    }

    pub fn remove_event(&mut self, n: u64) {
        let Some(event) = self.n_to_event_get(n) else {
            return;
        };
        self.n_to_event_remove(n);
        for (tag, value) in SingleLetterTags::new(&event.tags) {
            self.conditions
                .delete(key_to_vec(
                    &Condition::Tag(tag, value.clone()),
                    Time(event.created_at, n),
                ))
                .unwrap();
        }
        self.conditions
            .delete(key_to_vec(
                &Condition::Author(event.pubkey),
                Time(event.created_at, n),
            ))
            .unwrap();
        self.conditions
            .delete(key_to_vec(
                &Condition::Id(event.id),
                Time(event.created_at, n),
            ))
            .unwrap();
        self.conditions
            .delete(key_to_vec(
                &Condition::Kind(event.kind),
                Time(event.created_at, n),
            ))
            .unwrap();
        self.time_remove(Time(event.created_at, n));
    }
}

type ConditionVec = Vec<u8>;

struct ConditionAndIter<'a> {
    condition: Vec<u8>,
    db: RocksIter<'a, Rocks>,
}

impl Debug for ConditionAndIter<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.condition.as_hex())
    }
}

#[derive(Debug)]
pub struct ConditionsWithLatest<'a> {
    or_conditions: PriorityQueue<Time, ConditionAndIter<'a>>,
    remained: Vec<ConditionAndIter<'a>>,
    id: u32,
}

#[derive(Debug)]
pub struct ConditionsWithLatestStopped {
    or_conditions: Vec<(Time, ConditionVec)>,
    remained: Vec<ConditionVec>,
    id: u32,
}

impl<'a> ConditionsWithLatest<'a> {
    fn new(conditions: Vec<&Condition>, db: &'a Db, id: u32) -> Self {
        ConditionsWithLatest {
            or_conditions: PriorityQueue::new(),
            remained: conditions
                .into_iter()
                .map(|condition| ConditionAndIter {
                    condition: condition.to_vec(),
                    db: db.conditions.iterator(IteratorMode::End),
                })
                .collect(),
            id,
        }
    }

    fn get_latest(&mut self, until: Time) -> Option<(u32, Time)> {
        while let Some((t, _)) = self.or_conditions.peek() {
            if *t > until {
                let c = self.or_conditions.pop().unwrap().1;
                self.remained.push(c);
            } else {
                break;
            }
        }
        let number_of_db_access = self.remained.len() as u32;
        while let Some(ConditionAndIter {
            condition: mut c,
            db: mut i,
        }) = self.remained.pop()
        {
            let c_len = c.len();
            until.write_bytes(&mut c).unwrap();
            i.set_mode(rocksdb::IteratorMode::From(&c, rocksdb::Direction::Reverse));
            c.truncate(c_len);
            if let Some(a) = i.next() {
                let s = a.unwrap().0;
                if s.starts_with(&c) {
                    let t = Time::from_slice(&s[c_len..]);
                    self.or_conditions.push(
                        t,
                        ConditionAndIter {
                            condition: c,
                            db: i,
                        },
                    );
                }
            }
        }
        if let Some((t, _)) = self.or_conditions.peek() {
            Some((number_of_db_access, *t))
        } else {
            None
        }
    }

    pub fn stop(self) -> ConditionsWithLatestStopped {
        ConditionsWithLatestStopped {
            or_conditions: self
                .or_conditions
                .into_iter()
                .map(|(p, c)| (p, c.condition))
                .collect(),
            remained: self.remained.into_iter().map(|c| c.condition).collect(),
            id: self.id,
        }
    }
}

impl ConditionsWithLatestStopped {
    pub fn restart(self, db: &Db) -> ConditionsWithLatest {
        ConditionsWithLatest {
            or_conditions: self
                .or_conditions
                .into_iter()
                .map(|(p, condition)| {
                    (
                        p,
                        ConditionAndIter {
                            condition,
                            db: db.conditions.iterator(IteratorMode::End),
                        },
                    )
                })
                .collect(),
            remained: self
                .remained
                .into_iter()
                .map(|condition| ConditionAndIter {
                    condition,
                    db: db.conditions.iterator(IteratorMode::End),
                })
                .collect(),
            id: self.id,
        }
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

    const ZERO: Self = Self(0, 0);

    fn write_bytes(self, buff: &mut Vec<u8>) -> io::Result<()> {
        use std::io::Write;
        buff.write_all(&self.0.to_be_bytes())?;
        buff.write_all(&self.1.to_be_bytes())?;
        Ok(())
    }

    pub fn from_slice(s: &[u8]) -> Self {
        debug_assert_eq!(s.len(), 16);
        let t = u64::from_be_bytes(s[0..8].try_into().unwrap());
        let n = u64::from_be_bytes(s[8..16].try_into().unwrap());
        Time(t, n)
    }

    pub fn to_vec(self) -> Vec<u8> {
        let mut buff = Vec::with_capacity(16);
        self.write_bytes(&mut buff).unwrap();
        buff
    }
}

#[derive(Debug)]
pub struct GetEvents<'a> {
    pub until: Time,
    pub and_conditions: PriorityQueue<OrderedFloat<f32>, ConditionsWithLatest<'a>>,
}

impl<'a> GetEvents<'a> {
    pub fn new(filter: &Filter, db: &'a Db) -> Option<Self> {
        if filter.search.is_some() {
            return None;
        }
        let mut and_conditions = PriorityQueue::with_capacity(filter.conditions.len());
        for (i, (priority, cs)) in filter.conditions.iter().enumerate() {
            if cs.is_empty() {
                return None;
            }
            and_conditions.push(
                OrderedFloat(*priority),
                ConditionsWithLatest::new(cs.iter().collect(), db, i as u32),
            );
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
            let mut i = db.time.iterator(IteratorMode::From(
                &self.until.to_vec(),
                rocksdb::Direction::Reverse,
            ));
            return if let Some(t) = i.next() {
                let t = Time::from_slice(&t.unwrap().0);
                self.until = t.pred();
                Some(t)
            } else {
                None
            };
        }
        let mut head_conditions = Vec::new();
        let mut next_and_conditions = PriorityQueue::with_capacity(self.and_conditions.len());
        let mut count = 0.;
        loop {
            let mut hs = vec![(
                100,
                self.until.0,
                100,
                100,
                OrderedFloat(100.0),
                OrderedFloat(100.0),
            )];
            while let Some((p_diff, mut cs)) = self.and_conditions.pop() {
                if head_conditions.contains(&cs.id) {
                    next_and_conditions.push(p_diff, cs);
                } else if let Some((number_of_db_access, t)) = cs.get_latest(self.until) {
                    let diff = self.until.minus(t);
                    self.until = t;
                    if diff != (0, 0) {
                        head_conditions.clear();
                    }
                    head_conditions.push(cs.id);
                    if count == 0. {
                        next_and_conditions.push(p_diff, cs);
                    } else {
                        let priority =
                            (OrderedFloat(diff.0 as f32 / number_of_db_access.min(1) as f32)
                                + (p_diff * count))
                                / (count + 1.);
                        hs.push((cs.id, t.0, diff.0, number_of_db_access, p_diff, priority));
                        next_and_conditions.push(priority, cs);
                    }
                } else {
                    return None;
                }
            }
            std::mem::swap(&mut self.and_conditions, &mut next_and_conditions);
            next_and_conditions.clear();
            if head_conditions.len() == self.and_conditions.len() {
                let c = self.until;
                self.until = c.pred();
                return Some(c);
            }
            count += 1.0;
        }
    }

    pub fn stop(self) -> GetEventsStopped {
        GetEventsStopped {
            until: self.until,
            and_conditions: self
                .and_conditions
                .into_iter()
                .map(|(p, c)| (p, c.stop()))
                .collect(),
        }
    }
}

pub struct GetEventsStopped {
    pub until: Time,
    pub and_conditions: Vec<(OrderedFloat<f32>, ConditionsWithLatestStopped)>,
}

impl GetEventsStopped {
    pub fn restart(self, db: &Db) -> GetEvents {
        GetEvents {
            until: self.until,
            and_conditions: self
                .and_conditions
                .into_iter()
                .map(|(p, c)| (p, c.restart(db)))
                .collect(),
        }
    }
}
