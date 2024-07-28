use bitcoin_hashes::hex::DisplayHex;
use bitcoin_hashes::{sha256, Hash};
use secp256k1::schnorr::Signature;
use secp256k1::{Message, XOnlyPublicKey};
use serde::de::{self, IgnoredAny, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize};
use smallvec::SmallVec;
use std::borrow::Cow;
use std::collections::BTreeSet;
use std::fmt::{Debug, Display, LowerHex};
use std::io::{self, Write};
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

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
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

type Priority = f32;

#[derive(Debug, Deserialize, Clone)]
pub struct Filter {
    #[serde(default)]
    pub since: u64,
    #[serde(default = "until_default")]
    pub until: u64,
    #[serde(default = "limit_default")]
    pub limit: u32,
    #[serde(flatten, deserialize_with = "deserialize_tags")]
    pub conditions: Vec<(Priority, BTreeSet<Condition>)>,
    pub search: Option<IgnoredAny>,
}

impl Filter {
    pub fn matches(&self, e: &Event) -> bool {
        if !(self.since <= e.created_at && e.created_at <= self.until) || self.search.is_some() {
            return false;
        }
        let tags: BTreeSet<_> = SingleLetterTags::new(&e.tags)
            .map(|(c, v)| Condition::Tag(c, v.clone()))
            .collect();
        for (_, c) in &self.conditions {
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

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum Condition {
    Tag(u8, FirstTagValue),
    Author(PubKey),
    Kind(u32),
    Id(EventId),
}

impl Condition {
    pub fn write_bytes(&self, buff: &mut Vec<u8>) -> io::Result<()> {
        match self {
            Condition::Id(a) => {
                buff.push(0);
                buff.write_all(a.0.as_byte_array())?;
                buff.push(0);
            }
            Condition::Author(a) => {
                buff.push(0);
                buff.write_all(&a.0.serialize())?;
                buff.push(1);
            }
            Condition::Tag(c, FirstTagValue::Hex32(a)) => {
                buff.push(0);
                buff.write_all(a)?;
                buff.push(*c);
            }
            Condition::Tag(c, FirstTagValue::String(a)) => {
                buff.push(*c);
                buff.write_all(a.as_bytes())?;
                buff.push(0xff);
            }
            Condition::Kind(k) => {
                buff.push(1);
                buff.write_all(&k.to_be_bytes())?;
                buff.push(0xff);
            }
        }
        Ok(())
    }

    pub fn byte_len(&self) -> usize {
        match self {
            Condition::Id(_)
            | Condition::Author(_)
            | Condition::Tag(_, FirstTagValue::Hex32(_)) => 34,
            Condition::Tag(_, FirstTagValue::String(a)) => a.len() + 2,
            Condition::Kind(_) => 6,
        }
    }

    pub fn to_vec(&self) -> Vec<u8> {
        let mut buff = Vec::with_capacity(10);
        self.write_bytes(&mut buff).unwrap();
        buff
    }
}

fn limit_default() -> u32 {
    5000
}

fn until_default() -> u64 {
    u64::MAX
}

fn deserialize_tags<'de, D>(
    deserializer: D,
) -> Result<Vec<(Priority, BTreeSet<Condition>)>, D::Error>
where
    D: Deserializer<'de>,
{
    struct TagsVisitor;

    impl<'de> Visitor<'de> for TagsVisitor {
        type Value = Vec<(Priority, BTreeSet<Condition>)>;

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
                    "ids" => {
                        let s = map
                            .next_value::<Vec<EventId>>()?
                            .into_iter()
                            .map(Condition::Id)
                            .collect();
                        tags.push((1000., s))
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

#[derive(Debug, Deserialize)]
pub enum ClientToRelayTag {
    #[serde(rename = "EVENT")]
    Event,
    #[serde(rename = "REQ")]
    Req,
    #[serde(rename = "CLOSE")]
    Close,
    #[serde(rename = "AUTH")]
    Auth,
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
    Auth(Arc<Event>),
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
            ClientToRelayTag::Auth => {
                let e = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                Ok(ClientToRelay::Auth(e))
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
