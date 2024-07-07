use bitcoin_hashes::{sha256, Hash};
use secp256k1::schnorr::Signature;
use secp256k1::{Message, XOnlyPublicKey};
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use smallvec::SmallVec;
use std::borrow::Cow;
use std::collections::BTreeSet;
use std::fmt::{Debug, Display, LowerHex};
use std::io::Write;
use std::slice;
use std::str::FromStr;
use std::sync::Arc;

#[derive(Deserialize, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize)]
pub struct EventId(sha256::Hash);

impl FromStr for EventId {
    type Err = <sha256::Hash as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(EventId(sha256::Hash::from_str(s)?))
    }
}

#[derive(Debug, Serialize, Deserialize, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct PubKey(XOnlyPublicKey);

impl Display for EventId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        LowerHex::fmt(&self.0, f)
    }
}

impl std::fmt::Debug for EventId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EventId({})", self)
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Event {
    pub id: EventId,
    pub pubkey: PubKey,
    pub created_at: u64,
    pub kind: u32,
    pub tags: Vec<SmallVec<[String; 3]>>,
    pub content: String,
    pub sig: Signature,
}

impl Event {
    pub fn verify_hash(&self) -> bool {
        let mut h = sha256::HashEngine::default();
        write!(
            &mut h,
            r#"[0,"{:x}",{},{},"#,
            self.pubkey.0, self.created_at, self.kind
        )
        .unwrap();
        serde_json::to_writer(&mut h, &self.tags).unwrap();
        h.write_all(b",").unwrap();
        serde_json::to_writer(&mut h, &self.content).unwrap();
        h.write_all(b"]").unwrap();
        sha256::Hash::from_engine(h) == self.id.0
    }

    pub fn verify_sig(&self) -> bool {
        self.sig
            .verify(&Message::from_digest(*self.id.0.as_ref()), &self.pubkey.0)
            .is_ok()
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Filter {
    #[serde(default)]
    pub since: u64,
    #[serde(default = "until_default")]
    pub until: u64,
    #[serde(default = "limit_default")]
    pub limit: u32,
    #[serde(flatten, deserialize_with = "deserialize_tags")]
    pub conditions: Vec<BTreeSet<Condition>>,
}

impl Filter {
    pub fn matches(&self, e: &Event) -> bool {
        if !(self.since <= e.created_at && e.created_at <= self.until) {
            return false;
        }
        let tags: BTreeSet<_> = SingleLetterTags::new(&e.tags)
            .map(|(c, v)| Condition::Tag(c, v))
            .collect();
        for c in &self.conditions {
            if !c.contains(&Condition::Author(e.pubkey))
                && !c.contains(&Condition::Kind(e.kind))
                && !c.contains(&Condition::Id(e.id))
                && c.is_disjoint(&tags)
            {
                return false;
            }
        }
        true
    }
}

pub struct SingleLetterTags<'a>(slice::Iter<'a, SmallVec<[String; 3]>>);

impl<'a> SingleLetterTags<'a> {
    pub fn new(tags: &'a [SmallVec<[String; 3]>]) -> Self {
        SingleLetterTags(tags.iter())
    }
}

impl Iterator for SingleLetterTags<'_> {
    type Item = (char, String);

    fn next(&mut self) -> Option<Self::Item> {
        for t in self.0.by_ref() {
            if let Some(tag) = t.first() {
                let mut cs = tag.chars();
                if let (Some(tag), None) = (cs.next(), cs.next()) {
                    return Some((tag, t.get(1).map(|a| a.to_string()).unwrap_or_default()));
                }
            }
        }
        None
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum Condition {
    Tag(char, String),
    Author(PubKey),
    Kind(u32),
    Id(EventId),
}

fn limit_default() -> u32 {
    5000
}

fn until_default() -> u64 {
    u64::MAX
}

fn deserialize_tags<'de, D>(deserializer: D) -> Result<Vec<BTreeSet<Condition>>, D::Error>
where
    D: Deserializer<'de>,
{
    struct TagsVisitor;

    impl<'de> Visitor<'de> for TagsVisitor {
        type Value = Vec<BTreeSet<Condition>>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a map")
        }

        fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
        where
            M: de::MapAccess<'de>,
        {
            let mut tags = Vec::new();
            while let Some(key) = map.next_key::<String>()? {
                match key.as_str() {
                    "authors" => tags.push(
                        map.next_value::<Vec<PubKey>>()?
                            .into_iter()
                            .map(Condition::Author)
                            .collect(),
                    ),
                    "kinds" => tags.push(
                        map.next_value::<Vec<u32>>()?
                            .into_iter()
                            .map(Condition::Kind)
                            .collect(),
                    ),
                    "ids" => tags.push(
                        map.next_value::<Vec<EventId>>()?
                            .into_iter()
                            .map(Condition::Id)
                            .collect(),
                    ),
                    _ => {
                        let mut chars = key.chars();
                        if let (Some('#'), Some(c), None) =
                            (chars.next(), chars.next(), chars.next())
                        {
                            tags.push(
                                map.next_value::<Vec<String>>()?
                                    .into_iter()
                                    .map(|v| Condition::Tag(c, v))
                                    .collect(),
                            );
                        } else {
                            map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
            }
            Ok(tags)
        }
    }

    deserializer.deserialize_map(TagsVisitor)
}

#[derive(Debug, Deserialize)]
pub enum ClientToRelayTag {
    #[serde(rename = "EVENT")]
    Event,
    #[serde(rename = "REQ")]
    Req,
    #[serde(rename = "CLOSE")]
    Close,
}

#[derive(Debug)]
pub enum ClientToRelay<'a> {
    Event(Arc<Event>),
    Req {
        id: String,
        filters: SmallVec<[Filter; 2]>,
    },
    #[allow(unused)]
    Close(Cow<'a, str>),
}

impl<'a> Deserialize<'a> for ClientToRelay<'a> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'a>,
    {
        deserializer.deserialize_seq(ClientToRelayVisitor)
    }
}

struct ClientToRelayVisitor;

impl<'a> Visitor<'a> for ClientToRelayVisitor {
    type Value = ClientToRelay<'a>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(formatter, "a client-to-relay message")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'a>,
    {
        let tag: ClientToRelayTag = seq
            .next_element()?
            .ok_or_else(|| de::Error::invalid_length(0, &self))?;
        match tag {
            ClientToRelayTag::Event => {
                let e = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                Ok(ClientToRelay::Event(e))
            }
            ClientToRelayTag::Req => {
                let id = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                let mut filters = SmallVec::with_capacity(seq.size_hint().unwrap_or_default());
                while let Some(a) = seq.next_element()? {
                    filters.push(a);
                }
                Ok(ClientToRelay::Req { id, filters })
            }
            ClientToRelayTag::Close => {
                let id = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                Ok(ClientToRelay::Close(id))
            }
        }
    }
}
