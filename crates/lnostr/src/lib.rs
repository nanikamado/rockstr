mod client_message;
mod event_builder;
pub mod kinds;
mod relay_message;

use bitcoin_hashes::hex::DisplayHex;
use bitcoin_hashes::{sha256, Hash};
pub use client_message::ClientMessage;
pub use event_builder::EventBuilder;
use hex::FromHex;
pub use relay_message::RelayMessage;
use rustc_hash::FxHashSet;
use secp256k1::schnorr::Signature;
pub use secp256k1::Keypair;
use secp256k1::{Message, XOnlyPublicKey};
use serde::de::{self, IgnoredAny, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt::{Debug, Display, LowerHex};
use std::io::Write;
use std::slice;
use std::str::FromStr;

#[derive(Deserialize, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Serialize)]
pub struct EventId(sha256::Hash);

impl FromStr for EventId {
    type Err = <sha256::Hash as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(EventId(sha256::Hash::from_str(s)?))
    }
}

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

impl From<[u8; 32]> for EventId {
    fn from(value: [u8; 32]) -> Self {
        EventId(sha256::Hash::from_byte_array(value))
    }
}

impl EventId {
    pub fn as_byte_array(&self) -> &[u8; 32] {
        self.0.as_byte_array()
    }

    pub fn to_byte_array(self) -> [u8; 32] {
        self.0.to_byte_array()
    }
}

#[derive(Serialize, Deserialize, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct PubKey(XOnlyPublicKey);

impl Display for PubKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        LowerHex::fmt(&self.0, f)
    }
}

impl std::fmt::Debug for PubKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PubKey({})", self)
    }
}

impl std::str::FromStr for PubKey {
    type Err = secp256k1::Error;
    fn from_str(s: &str) -> Result<PubKey, Self::Err> {
        Ok(PubKey(XOnlyPublicKey::from_str(s)?))
    }
}

impl PubKey {
    pub fn to_bytes(self) -> [u8; 32] {
        self.0.serialize()
    }

    pub fn from_slice(s: &[u8]) -> Result<Self, secp256k1::Error> {
        Ok(PubKey(XOnlyPublicKey::from_slice(s)?))
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Event {
    pub id: EventId,
    pub pubkey: PubKey,
    pub created_at: u64,
    pub kind: u32,
    pub tags: Vec<Tag>,
    pub content: String,
    pub sig: Signature,
}

#[derive(Debug)]
pub struct Tag(pub String, pub Option<(FirstTagValue, Vec<String>)>);

impl Serialize for Tag {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut s = serializer.serialize_seq(None)?;
        s.serialize_element(&self.0)?;
        match &self.1 {
            Some((t, ts)) => {
                s.serialize_element(t)?;
                for t in ts {
                    s.serialize_element(t)?;
                }
            }
            None => (),
        }
        s.end()
    }
}

impl<'de> Deserialize<'de> for Tag {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct TagVisitor;

        impl<'de> Visitor<'de> for TagVisitor {
            type Value = Tag;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("an array")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                use serde::de::Error;
                let t = seq
                    .next_element()?
                    .ok_or_else(|| A::Error::invalid_length(0, &"one or more"))?;
                if let Some(t2) = seq.next_element()? {
                    let mut t3 = Vec::new();
                    while let Some(t) = seq.next_element()? {
                        t3.push(t);
                    }
                    Ok(Tag(t, Some((t2, t3))))
                } else {
                    Ok(Tag(t, None))
                }
            }
        }

        deserializer.deserialize_seq(TagVisitor)
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
#[serde(untagged)]
pub enum FirstTagValue {
    #[serde(with = "hex::serde")]
    Hex32([u8; 32]),
    String(String),
}

impl Display for FirstTagValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FirstTagValue::Hex32(a) => {
                write!(f, "{}", a.as_hex())
            }
            FirstTagValue::String(a) => {
                write!(f, "{a}")
            }
        }
    }
}

impl FirstTagValue {
    pub fn parse(s: &str) -> Self {
        if let Ok(a) = <[u8; 32]>::from_hex(s) {
            FirstTagValue::Hex32(a)
        } else {
            FirstTagValue::String(s.to_string())
        }
    }
}

fn event_hash(
    pubkey: &PubKey,
    created_at: u64,
    kind: u32,
    tags: &[Tag],
    content: &str,
) -> sha256::Hash {
    let mut h = sha256::HashEngine::default();
    write!(&mut h, r#"[0,"{:x}",{},{},"#, pubkey.0, created_at, kind).unwrap();
    serde_json::to_writer(&mut h, tags).unwrap();
    h.write_all(b",").unwrap();
    serde_json::to_writer(&mut h, content).unwrap();
    h.write_all(b"]").unwrap();
    sha256::Hash::from_engine(h)
}

impl Event {
    pub fn verify_hash(&self) -> bool {
        event_hash(
            &self.pubkey,
            self.created_at,
            self.kind,
            &self.tags,
            &self.content,
        ) == self.id.0
    }

    pub fn verify_sig(&self) -> bool {
        self.sig
            .verify(&Message::from_digest(*self.id.0.as_ref()), &self.pubkey.0)
            .is_ok()
    }
}

type Priority = f32;

#[derive(Debug, Deserialize, Clone)]
pub struct Filter {
    #[serde(default)]
    pub since: u64,
    #[serde(default = "until_default")]
    pub until: u64,
    #[serde(default = "limit_default")]
    pub limit: u32,
    pub ids: Option<FxHashSet<EventId>>,
    #[serde(flatten, deserialize_with = "deserialize_tags")]
    pub conditions: Vec<(Priority, FxHashSet<Condition>)>,
    pub search: Option<IgnoredAny>,
}

impl Filter {
    pub fn matches(&self, e: &Event) -> bool {
        if !(self.since <= e.created_at && e.created_at <= self.until) || self.search.is_some() {
            return false;
        }
        if !self.ids.as_ref().map_or(true, |ids| ids.contains(&e.id)) {
            return false;
        }
        for (_, c) in &self.conditions {
            if !c.contains(&Condition::Author(e.pubkey))
                && !c.contains(&Condition::Kind(e.kind))
                && SingleLetterTags::new(&e.tags)
                    .all(|(k, v)| !c.contains(&Condition::Tag(k, v.clone())))
            {
                return false;
            }
        }
        true
    }
}

pub struct SingleLetterTags<'a>(slice::Iter<'a, Tag>);

impl<'a> SingleLetterTags<'a> {
    pub fn new(tags: &'a [Tag]) -> Self {
        SingleLetterTags(tags.iter())
    }
}

impl<'a> Iterator for SingleLetterTags<'a> {
    type Item = (u8, &'a FirstTagValue);

    fn next(&mut self) -> Option<Self::Item> {
        for Tag(t1, t2) in self.0.by_ref() {
            if t1.len() == 1 {
                let key = t1.as_bytes()[0];
                if key.is_ascii_alphabetic() {
                    if let Some((value, _)) = t2 {
                        return Some((key, value));
                    }
                }
            }
        }
        None
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
pub enum Condition {
    Tag(u8, FirstTagValue),
    Author(PubKey),
    Kind(u32),
}

fn limit_default() -> u32 {
    5000
}

fn until_default() -> u64 {
    u64::MAX
}

fn deserialize_tags<'de, D>(
    deserializer: D,
) -> Result<Vec<(Priority, FxHashSet<Condition>)>, D::Error>
where
    D: Deserializer<'de>,
{
    struct TagsVisitor;

    impl<'de> Visitor<'de> for TagsVisitor {
        type Value = Vec<(Priority, FxHashSet<Condition>)>;

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
                    "authors" => {
                        let s = map
                            .next_value::<Vec<PubKey>>()?
                            .into_iter()
                            .map(Condition::Author)
                            .collect();
                        tags.push((700., s))
                    }
                    "kinds" => {
                        let s = map
                            .next_value::<Vec<u32>>()?
                            .into_iter()
                            .map(Condition::Kind)
                            .collect();
                        tags.push((0., s))
                    }
                    _ => {
                        if key.len() == 2 && key.starts_with('#') {
                            let t = key.as_bytes()[1];
                            let s = map
                                .next_value::<Vec<FirstTagValue>>()?
                                .into_iter()
                                .map(|v| Condition::Tag(t, v))
                                .collect();
                            let priority = if t == b'e' {
                                900.
                            } else if t == b'p' {
                                800.
                            } else {
                                600.
                            };
                            tags.push((priority, s));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_value_serde_test() {
        let t: FirstTagValue = serde_json::from_str(r#""aa""#).unwrap();
        assert_eq!(t, FirstTagValue::String("aa".to_string()));
        let t: FirstTagValue = serde_json::from_str(
            r#""aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa""#,
        )
        .unwrap();
        assert_eq!(
            t,
            FirstTagValue::Hex32(
                hex::FromHex::from_hex(
                    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                )
                .unwrap()
            )
        );
    }
}
