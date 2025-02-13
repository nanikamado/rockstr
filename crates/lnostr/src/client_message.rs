use crate::{Event, Filter};
use serde::de::{IgnoredAny, Visitor};
use serde::{de, Deserialize};
use smallvec::SmallVec;
use std::borrow::Cow;
use std::sync::Arc;

#[derive(Debug)]
pub enum ClientMessage<'a> {
    Event(Arc<Event>),
    InvalidEvent {
        id: String,
    },
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
        #[derive(Debug, Deserialize)]
        pub struct InvalidEvent {
            pub id: String,
        }
        #[derive(Deserialize, Clone, Debug, PartialEq)]
        #[serde(untagged)]
        pub enum ParseEither<A, B> {
            First(A),
            Second(B),
        }
        match tag {
            "EVENT" => {
                let e: ParseEither<Arc<Event>, InvalidEvent> = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                match e {
                    ParseEither::First(e) => Ok(ClientMessage::Event(e)),
                    ParseEither::Second(e) => Ok(ClientMessage::InvalidEvent { id: e.id }),
                }
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
                while let Some(a) = seq.next_element::<ParseEither<_, IgnoredAny>>()? {
                    if let ParseEither::First(a) = a {
                        filters.push(a);
                    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn req_parse() {
        let req = r#"["REQ","test",{"authors":["aaaa"],"limit":1},{"limit":2}]"#;
        let m: ClientMessage = serde_json::from_str(req).unwrap();
        let ClientMessage::Req { id, filters } = m else {
            panic!()
        };
        assert_eq!(id, "test");
        assert_eq!(filters.len(), 1);
        assert_eq!(filters[0].limit, 2);
    }
}
