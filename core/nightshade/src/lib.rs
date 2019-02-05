use std::collections::{HashSet, HashMap};
use primitives::hash::{CryptoHash, hash_struct};
use std::cmp::max;
#[macro_use]
extern crate serde_derive;

pub type AuthorityId = usize;

#[derive(PartialEq, Eq, Debug)]
pub enum NSResult {
    Success,
    Retrieve(Vec<CryptoHash>),
    Known,
    Error(String)
}

impl NSResult {
    fn expect(&self, message: &'static str) {
        match &self {
            NSResult::Success => {},
            _ => { panic!(message); }
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Message {
    pub author: AuthorityId,
    pub parents: Vec<CryptoHash>,
    pub data: Vec<u8>,
}

struct Node {
    message: Message,
    endorses: AuthorityId,
    max_score: Vec<u64>,
    max_confidence: Vec<u64>,
}

pub struct Nightshade {
    owner_id: usize,
    num_authorities: usize,
    nodes: HashMap<CryptoHash, Node>,
    last_message_per_authority: HashMap<AuthorityId, CryptoHash>,
    tips: HashSet<CryptoHash>,
    pending_messages: Vec<Message>,
}

impl Nightshade {
    pub fn new(owner_id: usize, num_authorities: usize) -> Self {
        Nightshade {
            owner_id,
            num_authorities,
            nodes: Default::default(),
            last_message_per_authority: Default::default(),
            pending_messages: Default::default(),
            tips: Default::default(),
        }
    }

    fn filter_candidates(&self, candidates: Vec<usize>, scores: &Vec<u64>) -> Vec<usize> {
        let mut result = vec![];
        for &candidate in candidates.iter() {
            if result.is_empty() || scores[candidate] == scores[result[0]] {
                result.push(candidate);
            } else if scores[candidate] > scores[result[0]] {
                result = vec![candidate];
            }
        }
        result
    }

    pub fn process_message(&mut self, message: Message) -> NSResult {
        let h = hash_struct(&message);
        if self.nodes.contains_key(&h) {
            return NSResult::Known;
        }
        let missing_messages: Vec<CryptoHash> = message.parents.iter()
            .filter(|&h| !self.nodes.contains_key(h)).cloned().collect();
        if !missing_messages.is_empty() {
            self.pending_messages.push(message);
            return NSResult::Retrieve(missing_messages);
        }
        let endorses;
        let mut max_score = vec![0; self.num_authorities];
        let mut max_confidence = vec![0; self.num_authorities];
        if message.parents.is_empty() {
            endorses = message.author;
            max_score[message.author] = 1;
        } else {
            let mut local_score = vec![0; self.num_authorities];
            let mut parents_per_author = vec![CryptoHash::default(); self.num_authorities];
            for parent in message.parents.iter() {
                if self.tips.contains(parent) {
                    self.tips.remove(parent);
                }
                let parent_node = self.nodes.get(parent).expect("Checked that parents are present");
                local_score[parent_node.endorses] += 1;
                parents_per_author[parent_node.message.author] = *parent;
                for i in 0..self.num_authorities {
                    max_score[i] = max(max_score[i], parent_node.max_score[i]);
                    max_confidence[i] = max(max_confidence[i], parent_node.max_confidence[i]);
                }
            }
            for i in 0..self.num_authorities {
                max_score[i] = max(max_score[i], local_score[i]);
            }
            let mut candidates: Vec<usize> = (0..self.num_authorities).collect();
            candidates = self.filter_candidates(candidates, &max_confidence);
            candidates = self.filter_candidates(candidates, &max_score);
            endorses = candidates[0];

            let mut confidence = 0;
            for p in parents_per_author {
                if p != CryptoHash::default() {
                    let parent_node = self.nodes.get(&p).expect("Checked that parents are present");
                    if parent_node.endorses == endorses && parent_node.max_confidence[endorses] == max_confidence[endorses] && parent_node.max_score[endorses] == max_score[endorses] {
                        confidence += 1;
                    }
                }
            }
            if confidence > (self.num_authorities * 2) / 3 {
                max_confidence[endorses] += 1;
            }
        }
        self.tips.insert(h);
        self.last_message_per_authority.insert(message.author, h);
        let node = Node {
            message, endorses, max_score, max_confidence
        };
        self.nodes.insert(h, node);
        // Check if pending messages are unblocked.
        NSResult::Success
    }

    pub fn create_message(&mut self) -> Message {
        let m = Message {
            author: self.owner_id,
            parents: self.tips.iter().cloned().collect(),
            data: vec![],
        };
        self.process_message(m.clone()).expect("Creating own message");
        m
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    fn message(author: AuthorityId, parents: Vec<CryptoHash>) -> (Message, CryptoHash) {
        let m = Message { author, parents, data: vec![] };
        let h = hash_struct(&m);
        (m, h)
    }

    #[test]
    fn test_nightshade_basic() {
        let mut ns = Nightshade::new(0, 3);
        let (m1, mh1) = message(0, vec![]);
        let (m2, mh2) = message(1, vec![]);
        let (m3, mh3) = message(2, vec![]);
        assert_eq!(ns.process_message(m1.clone()), NSResult::Success);
        assert_eq!(ns.process_message(m1), NSResult::Known);
        assert_eq!(ns.process_message(m2), NSResult::Success);
        assert_eq!(ns.process_message(m3), NSResult::Success);
        let (m4, mh4) = message(0, vec![mh1]);
        let (m5, mh5) = message(0, vec![mh4]);
        assert_eq!(ns.process_message(m5), NSResult::Retrieve(vec![mh4]));
        assert_eq!(ns.process_message(m4), NSResult::Success);
        let m6 = ns.create_message();
        assert_eq!(ns.process_message(m6), NSResult::Known);
    }
}