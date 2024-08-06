use crate::relay::Db;
pub use lnostr::{
    ClientMessage, Condition, Event, EventId, Filter, FirstTagValue, PubKey, SingleLetterTags, Tag,
};
use serde::de::IgnoredAny;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::io::{self, Write};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
#[serde(untagged)]
pub enum FirstTagValueCompact {
    Hex32(u64),
    String(String),
}

impl FirstTagValueCompact {
    pub fn new(v: &FirstTagValue, db: &mut Db) -> Self {
        match v {
            FirstTagValue::Hex32(a) => Self::Hex32(db.get_n_from_hash(a)),
            FirstTagValue::String(a) => Self::String(a.clone()),
        }
    }
}

type Priority = f32;

#[derive(Debug, Clone)]
pub struct FilterCompact {
    pub since: u64,
    pub until: u64,
    pub limit: u32,
    pub ids: Option<Vec<u64>>,
    pub conditions: Vec<(Priority, Vec<ConditionCompact>)>,
    pub search: Option<IgnoredAny>,
}

impl FilterCompact {
    pub fn new(f: &Filter, db: &mut Db) -> Self {
        let mut new_conditions = Vec::with_capacity(f.conditions.len());
        for (priority, cs) in &f.conditions {
            new_conditions.push((
                *priority,
                cs.iter().map(|c| ConditionCompact::new(c, db)).collect(),
            ));
        }
        Self {
            since: f.since,
            until: f.until,
            limit: f.limit,
            ids: f
                .ids
                .as_ref()
                .map(|ids| ids.iter().flat_map(|id| db.id_to_existing_n(*id)).collect()),
            conditions: new_conditions,
            search: f.search,
        }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum ConditionCompact {
    Tag(u8, FirstTagValueCompact),
    Author(u64),
    Kind(u32),
}

impl ConditionCompact {
    pub fn write_bytes(&self, buff: &mut Vec<u8>) -> io::Result<()> {
        match self {
            ConditionCompact::Author(a) => {
                buff.write_all(&a.to_be_bytes())?;
                buff.push(0);
            }
            ConditionCompact::Tag(c, FirstTagValueCompact::Hex32(a)) => {
                buff.write_all(&a.to_be_bytes())?;
                buff.push(*c);
            }
            ConditionCompact::Tag(c, FirstTagValueCompact::String(a)) => {
                buff.push(*c);
                buff.write_all(a.as_bytes())?;
                buff.push(0xff);
            }
            ConditionCompact::Kind(k) => {
                buff.push(1);
                buff.write_all(&k.to_be_bytes())?;
            }
        }
        Ok(())
    }

    pub fn to_vec(&self) -> Vec<u8> {
        let mut buff = Vec::with_capacity(10);
        self.write_bytes(&mut buff).unwrap();
        buff
    }

    pub fn new(c: &Condition, db: &mut Db) -> ConditionCompact {
        match c {
            Condition::Tag(k, v) => ConditionCompact::Tag(*k, FirstTagValueCompact::new(v, db)),
            Condition::Author(a) => {
                let n = db.get_n_from_hash(&a.to_bytes());
                ConditionCompact::Author(n)
            }
            Condition::Kind(a) => ConditionCompact::Kind(*a),
        }
    }
}
