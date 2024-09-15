use crate::{event_hash, kinds, Event, EventId, FirstTagValue, PubKey, Tag};
use bitcoin_hashes::Hash;
use secp256k1::{Keypair, Message};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub struct EventBuilder {
    pub created_at: Option<u64>,
    pub kind: u32,
    pub tags: Vec<Tag>,
    pub content: String,
}

impl EventBuilder {
    pub fn new(kind: u32, content: String) -> Self {
        Self {
            created_at: None,
            kind,
            tags: Vec::new(),
            content,
        }
    }

    pub fn add_tag(mut self, tag: Tag) -> Self {
        self.tags.push(tag);
        self
    }

    pub fn auth(challenge: &str, relay: &str) -> Self {
        Self::new(kinds::CLIENT_AUTHENTICATION, String::new())
            .add_tag(Tag(
                "challenge".to_string(),
                Some((FirstTagValue::parse(challenge), Vec::new())),
            ))
            .add_tag(Tag(
                "relay".to_string(),
                Some((FirstTagValue::parse(relay), Vec::new())),
            ))
    }

    pub fn to_event(self, keys: &Keypair) -> Event {
        let created_at = self.created_at.unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
        });
        let pubkey = PubKey(keys.x_only_public_key().0);
        let id = event_hash(&pubkey, created_at, self.kind, &self.tags, &self.content);
        let sig = keys.sign_schnorr(Message::from_digest(id.to_byte_array()));
        Event {
            id: EventId(id),
            pubkey,
            created_at,
            kind: self.kind,
            tags: self.tags,
            content: self.content,
            sig: Some(sig),
        }
    }
}
