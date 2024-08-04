use crate::nostr::{
    ConditionCompact, Event, EventId, FilterCompact, FirstTagValue, FirstTagValueCompact, PubKey,
    SingleLetterTags,
};
use crate::priority_queue::PriorityQueue;
use bitcoin_hashes::hex::DisplayHex;
use ordered_float::OrderedFloat;
use rocksdb::{DBIteratorWithThreadMode as RocksIter, IteratorMode, DB as Rocks};
use std::borrow::Borrow;
use std::fmt::Debug;
use std::fs::create_dir_all;
use std::io::{self, Write};
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

type UnixTime = u64;

const HASH_TO_N_UNKNOWN: u8 = 2;
const HASH_TO_N_HAVE: u8 = 1;
const HASH_TO_N_DELETED: u8 = 3;

#[derive(Debug, PartialEq, Eq)]
pub enum HashToNEntry {
    Unknown { deletion_requested_by: Vec<u64> },
    Have,
    Deleted,
}

impl HashToNEntry {
    fn from_bytes(bytes: &[u8]) -> Self {
        match bytes[8] {
            HASH_TO_N_HAVE => HashToNEntry::Have,
            HASH_TO_N_UNKNOWN => {
                let deletion_requested_by = bytes[9..]
                    .chunks(8)
                    .map(|a| u64::from_be_bytes(a.try_into().unwrap()))
                    .collect();
                HashToNEntry::Unknown {
                    deletion_requested_by,
                }
            }
            HASH_TO_N_DELETED => HashToNEntry::Deleted,
            _ => {
                panic!()
            }
        }
    }
}

#[derive(Debug)]
pub struct Db {
    pub n_count: u64,
    /// [u64] -> event json
    pub n_to_event: Rocks,
    /// one of the following format:
    /// - hash -> n as u64, 1 (if the hash is an id of event that we have)
    /// - hash -> n as u64, 2 (if the hash is pubkey)
    /// - hash -> n as u64, 2, \<n of the pubkey who requested the deletion as u64\>*
    /// - hash -> n as u64, 3 (if the hash is requested to be deleted by its author)
    hash_to_n: Rocks,
    /// one of the following format:
    /// - [ConditionCompact] [Time] -> []
    /// - [ConditionCompact::Deleted] -> [PubKey]
    pub conditions: Rocks,
    /// [Time] -> []
    pub time: Rocks,
    // pub blocked_pubkeys: FxHashSet<PubKey>,
}

impl Default for Db {
    fn default() -> Self {
        let config_dir = Path::new("rockstr");
        create_dir_all(config_dir).unwrap();
        let mut opts = rocksdb::Options::default();
        opts.create_if_missing(true);
        opts.set_compression_type(rocksdb::DBCompressionType::Lz4);
        let conditions = Rocks::open(&opts, config_dir.join("conditions.rocksdb")).unwrap();
        let n_to_event = Rocks::open(&opts, config_dir.join("n_to_event.rocksdb")).unwrap();
        let hash_to_n = Rocks::open(&opts, config_dir.join("hash_to_n.rocksdb")).unwrap();
        let time = Rocks::open(&opts, config_dir.join("time.rocksdb")).unwrap();
        let next_n = if let Some(a) = n_to_event.iterator(rocksdb::IteratorMode::End).next() {
            u64::from_be_bytes(a.unwrap().0.as_ref().try_into().unwrap()) + 1
        } else {
            1
        };
        Self {
            n_count: next_n,
            n_to_event,
            hash_to_n,
            conditions,
            time,
            // blocked_pubkeys: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum AddEventError {
    HaveNewer,
    Duplicated,
    Deleted,
}

fn key_to_vec(condition: &ConditionCompact, time: Time) -> Vec<u8> {
    let mut buff = Vec::with_capacity(16);
    condition.write_bytes(&mut buff).unwrap();
    time.write_bytes(&mut buff).unwrap();
    buff
}

fn first_d_value(event: &Event) -> Option<&FirstTagValue> {
    event.tags.iter().find_map(|t| {
        if t.0 == "d" {
            t.1.as_ref().map(|(a, _)| a)
        } else {
            None
        }
    })
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

    pub fn hash_to_n_get(&self, hash: &[u8; 32]) -> Option<(u64, HashToNEntry)> {
        self.hash_to_n.get(hash).unwrap().map(|a| {
            (
                u64::from_be_bytes(a[..8].try_into().unwrap()),
                HashToNEntry::from_bytes(&a),
            )
        })
    }

    pub fn hash_to_n_set_unknown(&self, hash: &[u8; 32], n: u64) {
        let mut value = [0; 9];
        value[..8].copy_from_slice(&n.to_be_bytes());
        value[8] = HASH_TO_N_UNKNOWN;
        self.hash_to_n.put(hash, value).unwrap();
    }

    pub fn hash_to_n_set_have(&self, hash: &[u8; 32], n: u64) {
        let mut value = [0; 9];
        value[..8].copy_from_slice(&n.to_be_bytes());
        value[8] = HASH_TO_N_HAVE;
        self.hash_to_n.put(hash, value).unwrap();
    }

    pub fn hash_to_n_set_deleted(&self, hash: &[u8; 32], n: u64) {
        let mut value = [0; 9];
        value[..8].copy_from_slice(&n.to_be_bytes());
        value[8] = HASH_TO_N_DELETED;
        self.hash_to_n.put(hash, value).unwrap();
    }

    pub fn hash_to_n_set_deletion_requested(&self, hash: &[u8; 32], n: u64, requested_by: &[u64]) {
        let mut value = n.to_be_bytes().to_vec();
        value.push(HASH_TO_N_UNKNOWN);
        for r in requested_by {
            value.write_all(&r.to_be_bytes()).unwrap();
        }
        self.hash_to_n.put(hash, value).unwrap();
    }

    pub fn get_n_from_event_id(&mut self, author: u64, id: &EventId) -> Result<u64, AddEventError> {
        let id = id.as_byte_array();
        let n = if let Some(v) = self.hash_to_n.get(id).unwrap() {
            match HashToNEntry::from_bytes(&v) {
                HashToNEntry::Have => return Err(AddEventError::Duplicated),
                HashToNEntry::Unknown {
                    deletion_requested_by,
                } => {
                    let n = u64::from_be_bytes(v[..8].try_into().unwrap());
                    if deletion_requested_by.contains(&author) {
                        self.hash_to_n_set_deleted(id, n);
                        return Err(AddEventError::Deleted);
                    }
                    n
                }
                HashToNEntry::Deleted => return Err(AddEventError::Deleted),
            }
        } else {
            // we do not know this id
            self.n_count += 1;
            self.n_count
        };
        self.hash_to_n_set_have(id, n);
        Ok(n)
    }

    pub fn id_to_existing_n(&self, id: EventId) -> Option<u64> {
        self.hash_to_n_get(id.as_byte_array()).and_then(|(n, s)| {
            if s == HashToNEntry::Have {
                Some(n)
            } else {
                None
            }
        })
    }

    pub fn get_n_from_hash(&mut self, hash: &[u8; 32]) -> u64 {
        if let Some(n) = self.hash_to_n.get(hash).unwrap() {
            u64::from_be_bytes(n[..8].try_into().unwrap())
        } else {
            self.n_count += 1;
            self.hash_to_n_set_unknown(hash, self.n_count);
            self.n_count
        }
    }

    pub fn add_event(&mut self, event: Arc<Event>) -> Result<u64, AddEventError> {
        let author = self.get_n_from_hash(&event.pubkey.to_bytes());
        let n = self.get_n_from_event_id(author, &event.id)?;
        match event.kind {
            0 | 3 | 10000..20000 => {
                let author = ConditionCompact::Author(author);
                let kind = ConditionCompact::Kind(event.kind);
                let mut outdated = None;
                {
                    let mut i = GetEvents {
                        until: Time(u64::MAX, u64::MAX),
                        and_conditions: PriorityQueue::from([
                            (
                                OrderedFloat(100.),
                                ConditionsWithLatest::new(vec![author], self, 0),
                            ),
                            (
                                OrderedFloat(0.),
                                ConditionsWithLatest::new(vec![kind], self, 1),
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
                            outdated = Some(n);
                        }
                    }
                }
                if let Some(n) = outdated {
                    self.remove_event(n);
                }
            }
            30000..40000 => {
                if let Some(d_value) = first_d_value(&event) {
                    let pubkey = self.get_n_from_hash(&event.pubkey.to_bytes());
                    self.remove_d_tagged_event(
                        event.created_at,
                        d_value,
                        event.kind,
                        pubkey,
                        Some(&event.id),
                    )?;
                }
            }
            5 => {
                for t in &event.tags {
                    if let ("e", Some((FirstTagValue::Hex32(a), _))) = (t.0.as_ref(), &t.1) {
                        let (n, e) = if let Some(a) = self.hash_to_n_get(a) {
                            a
                        } else {
                            self.n_count += 1;
                            let n = self.n_count;
                            (
                                n,
                                HashToNEntry::Unknown {
                                    deletion_requested_by: Vec::new(),
                                },
                            )
                        };
                        match e {
                            HashToNEntry::Unknown {
                                mut deletion_requested_by,
                            } => {
                                if !deletion_requested_by.contains(&author) {
                                    deletion_requested_by.push(author);
                                }
                                self.hash_to_n_set_deletion_requested(a, n, &deletion_requested_by);
                            }
                            HashToNEntry::Have => {
                                let e = self.n_to_event_get(n).unwrap();
                                if event.pubkey == e.pubkey {
                                    self.remove_event(n);
                                } else {
                                    continue;
                                }
                            }
                            HashToNEntry::Deleted => continue,
                        }
                    }
                    if let ("a", Some((FirstTagValue::String(v), _))) = (t.0.as_ref(), &t.1) {
                        if let Some(c1) = v.find(':') {
                            if let Some(c2) = v[c1 + 1..].find(':') {
                                let c2 = c1 + c2 + 1;
                                let kind = &v[..c1];
                                let pubkey = &v[c1 + 1..c2];
                                let d_value = &FirstTagValue::parse(&v[c2 + 1..]);
                                if let (Ok(kind), Ok(pubkey)) =
                                    (kind.parse::<u32>(), PubKey::from_str(pubkey))
                                {
                                    let pubkey = self.get_n_from_hash(&pubkey.to_bytes());
                                    let _ = self.remove_d_tagged_event(
                                        event.created_at,
                                        d_value,
                                        kind,
                                        pubkey,
                                        None,
                                    );
                                }
                            }
                        }
                    }
                }
            }
            _ => (),
        }
        let time = Time(event.created_at, n);
        for (tag, value) in SingleLetterTags::new(&event.tags) {
            let value = FirstTagValueCompact::new(value, self);
            self.conditions
                .put(key_to_vec(&ConditionCompact::Tag(tag, value), time), [])
                .unwrap();
        }
        let pubkey = self.get_n_from_hash(&event.pubkey.to_bytes());
        self.conditions
            .put(key_to_vec(&ConditionCompact::Author(pubkey), time), [])
            .unwrap();
        self.conditions
            .put(key_to_vec(&ConditionCompact::Kind(event.kind), time), [])
            .unwrap();
        self.time_insert(time);
        self.n_to_event_insert(n, &event);
        Ok(n)
    }

    fn remove_d_tagged_event(
        &mut self,
        earlier_than: u64,
        d_value: &FirstTagValue,
        kind: u32,
        author: u64,
        id: Option<&EventId>,
    ) -> Result<(), AddEventError> {
        let d_value_compact = FirstTagValueCompact::new(d_value, self);
        let author = ConditionCompact::Author(author);
        let kind = ConditionCompact::Kind(kind);
        let d_tag = ConditionCompact::Tag(b'd', d_value_compact);
        let mut outdated = None;
        {
            let mut i = GetEvents {
                until: Time(u64::MAX, u64::MAX),
                and_conditions: PriorityQueue::from([
                    (
                        OrderedFloat(200.),
                        ConditionsWithLatest::new(vec![d_tag], self, 2),
                    ),
                    (
                        OrderedFloat(100.),
                        ConditionsWithLatest::new(vec![author], self, 0),
                    ),
                    (
                        OrderedFloat(0.),
                        ConditionsWithLatest::new(vec![kind], self, 1),
                    ),
                ]),
            };
            while let Some(Time(t, n)) = i.next(self) {
                let e = self.n_to_event_get(n).unwrap();
                if first_d_value(&e).map_or(false, |d| d == d_value) {
                    let have_newer = match t.cmp(&earlier_than) {
                        std::cmp::Ordering::Less => false,
                        std::cmp::Ordering::Equal => id.map_or(false, |id| &e.id > id),
                        std::cmp::Ordering::Greater => true,
                    };
                    if have_newer {
                        return Err(AddEventError::HaveNewer);
                    } else {
                        outdated = Some(n);
                        break;
                    }
                }
            }
        }
        if let Some(n) = outdated {
            self.remove_event(n);
        }
        Ok(())
    }

    pub fn remove_event(&mut self, n: u64) {
        let Some(event) = self.n_to_event_get(n) else {
            return;
        };
        self.n_to_event_remove(n);
        self.hash_to_n_set_deleted(event.id.as_byte_array(), n);
        for (tag, value) in SingleLetterTags::new(&event.tags) {
            let value = FirstTagValueCompact::new(value, self);
            self.conditions
                .delete(key_to_vec(
                    &ConditionCompact::Tag(tag, value),
                    Time(event.created_at, n),
                ))
                .unwrap();
        }
        let pubkey = self.get_n_from_hash(&event.pubkey.to_bytes());
        self.conditions
            .delete(key_to_vec(
                &ConditionCompact::Author(pubkey),
                Time(event.created_at, n),
            ))
            .unwrap();
        self.conditions
            .delete(key_to_vec(
                &ConditionCompact::Kind(event.kind),
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
    fn new<T: Borrow<ConditionCompact>>(
        conditions: impl IntoIterator<Item = T>,
        db: &'a Db,
        id: u32,
    ) -> Self {
        ConditionsWithLatest {
            or_conditions: PriorityQueue::new(),
            remained: conditions
                .into_iter()
                .map(|condition| ConditionAndIter {
                    condition: condition.borrow().to_vec(),
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
    pub fn new(filter: &FilterCompact, db: &'a Db) -> Option<Self> {
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
                ConditionsWithLatest::new(cs.iter(), db, i as u32),
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
