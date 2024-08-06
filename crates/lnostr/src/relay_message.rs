// the following code is based on https://github.com/mikedilger/nostr-types/blob/c96dd411e48b34ec8f4977d86bca54d465024be3/src/versioned/relay_message4.rs#L1

use serde::de::{SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::marker::PhantomData;

/// A message from a relay to a client
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RelayMessage<Event = crate::Event, EventId = crate::EventId>
where
    Event: Serialize + for<'a> Deserialize<'a>,
    EventId: Serialize + for<'a> Deserialize<'a>,
{
    /// Used to send authentication challenges
    Auth(String),

    /// Used to indicate that a subscription was ended on the server side
    /// Every ClientMessage::Req _may_ trigger a RelayMessage::Closed response
    /// The last parameter may have a colon-terminated machine-readable prefix of:
    ///     duplicate, pow, blocked, rate-limited, invalid, auth-required,
    ///     restricted, or error
    Closed(String, String),

    /// End of subscribed events notification
    Eose(String),

    /// An event matching a subscription
    Event(String, Event),

    /// A human readable notice for errors and other information
    Notice(String),

    /// Used to notify clients if an event was successful
    /// Every ClientMessage::Event will trigger a RelayMessage::OK response
    /// The last parameter may have a colon-terminated machine-readable prefix of:
    ///     duplicate, pow, blocked, rate-limited, invalid, auth-required,
    ///     restricted or error
    Ok(EventId, bool, String),
}

impl<Event, EventId> Serialize for RelayMessage<Event, EventId>
where
    Event: Serialize + for<'a> Deserialize<'a>,
    EventId: Serialize + for<'a> Deserialize<'a>,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            RelayMessage::Auth(challenge) => {
                let mut seq = serializer.serialize_seq(Some(2))?;
                seq.serialize_element("AUTH")?;
                seq.serialize_element(&challenge)?;
                seq.end()
            }
            RelayMessage::Closed(id, message) => {
                let mut seq = serializer.serialize_seq(Some(3))?;
                seq.serialize_element("CLOSED")?;
                seq.serialize_element(&id)?;
                seq.serialize_element(&message)?;
                seq.end()
            }
            RelayMessage::Eose(id) => {
                let mut seq = serializer.serialize_seq(Some(2))?;
                seq.serialize_element("EOSE")?;
                seq.serialize_element(&id)?;
                seq.end()
            }
            RelayMessage::Event(id, event) => {
                let mut seq = serializer.serialize_seq(Some(3))?;
                seq.serialize_element("EVENT")?;
                seq.serialize_element(&id)?;
                seq.serialize_element(&event)?;
                seq.end()
            }
            RelayMessage::Notice(s) => {
                let mut seq = serializer.serialize_seq(Some(2))?;
                seq.serialize_element("NOTICE")?;
                seq.serialize_element(&s)?;
                seq.end()
            }
            RelayMessage::Ok(id, ok, message) => {
                let mut seq = serializer.serialize_seq(Some(4))?;
                seq.serialize_element("OK")?;
                seq.serialize_element(&id)?;
                seq.serialize_element(&ok)?;
                seq.serialize_element(&message)?;
                seq.end()
            }
        }
    }
}

impl<'de, Event, EventId> Deserialize<'de> for RelayMessage<Event, EventId>
where
    Event: Serialize + for<'a> Deserialize<'a>,
    EventId: Serialize + for<'a> Deserialize<'a>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(RelayMessageVisitor(PhantomData, PhantomData))
    }
}

struct RelayMessageVisitor<T, U>(PhantomData<T>, PhantomData<U>);

impl<'de, Event, EventId> Visitor<'de> for RelayMessageVisitor<Event, EventId>
where
    Event: Serialize + for<'a> Deserialize<'a>,
    EventId: Serialize + for<'a> Deserialize<'a>,
{
    type Value = RelayMessage<Event, EventId>;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "a sequence of strings")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<RelayMessage<Event, EventId>, A::Error>
    where
        A: SeqAccess<'de>,
    {
        use serde::de::Error as DeError;
        let t: &str = seq
            .next_element()?
            .ok_or_else(|| DeError::custom("Message missing initial string field"))?;
        match t {
            "EVENT" => {
                let id = seq
                    .next_element()?
                    .ok_or_else(|| DeError::custom("Message missing id field"))?;
                let event: Event = seq
                    .next_element()?
                    .ok_or_else(|| DeError::custom("Message missing event field"))?;
                Ok(RelayMessage::Event(id, event))
            }
            "NOTICE" => {
                let s: String = seq
                    .next_element()?
                    .ok_or_else(|| DeError::custom("Message missing string field"))?;
                Ok(RelayMessage::Notice(s))
            }
            "EOSE" => {
                let id = seq
                    .next_element()?
                    .ok_or_else(|| DeError::custom("Message missing id field"))?;
                Ok(RelayMessage::Eose(id))
            }
            "OK" => {
                let id = seq
                    .next_element()?
                    .ok_or_else(|| DeError::custom("Message missing id field"))?;
                let ok = seq
                    .next_element()?
                    .ok_or_else(|| DeError::custom("Message missing ok field"))?;
                let message: String = seq
                    .next_element()?
                    .ok_or_else(|| DeError::custom("Message missing string field"))?;
                Ok(RelayMessage::Ok(id, ok, message))
            }
            "AUTH" => {
                let challenge: String = seq
                    .next_element()?
                    .ok_or_else(|| DeError::custom("Message missing challenge field"))?;
                Ok(RelayMessage::Auth(challenge))
            }
            "CLOSED" => {
                let id = seq
                    .next_element()?
                    .ok_or_else(|| DeError::custom("Message messing id field"))?;
                let message: String = seq
                    .next_element()?
                    .ok_or_else(|| DeError::custom("Message missing string field"))?;
                Ok(RelayMessage::Closed(id, message))
            }
            _ => Err(DeError::custom(format!("Unknown Message: {t}"))),
        }
    }
}
