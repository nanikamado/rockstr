use crate::{Event, Filter};
use serde::de::Visitor;
use serde::{de, Deserialize};
use smallvec::SmallVec;
use std::borrow::Cow;
use std::sync::Arc;

#[derive(Debug)]
pub enum ClientMessage<'a> {
    Event(Arc<Event>),
    Req {
        id: String,
        filters: SmallVec<[Filter; 2]>,
    },
    Close(Cow<'a, str>),
    Auth(Arc<Event>),
}

impl<'a> Deserialize<'a> for ClientMessage<'a> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'a>,
    {
        deserializer.deserialize_seq(ClientMessageVisitor)
    }
}

struct ClientMessageVisitor;

impl<'a> Visitor<'a> for ClientMessageVisitor {
    type Value = ClientMessage<'a>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(formatter, "an array")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'a>,
    {
        let tag: &str = seq
            .next_element()?
            .ok_or_else(|| de::Error::invalid_length(0, &self))?;
        match tag {
            "EVENT" => {
                let e = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                Ok(ClientMessage::Event(e))
            }
            "AUTH" => {
                let e = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                Ok(ClientMessage::Auth(e))
            }
            "REQ" => {
                let id = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                let mut filters = SmallVec::with_capacity(seq.size_hint().unwrap_or_default());
                while let Some(a) = seq.next_element()? {
                    filters.push(a);
                }
                Ok(ClientMessage::Req { id, filters })
            }
            "CLOSE" => {
                let id = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                Ok(ClientMessage::Close(id))
            }
            _ => Err(de::Error::custom(format!("Unknown Message: {tag}"))),
        }
    }
}
