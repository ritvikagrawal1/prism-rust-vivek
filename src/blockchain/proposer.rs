use crate::crypto::hash::H256;
use std::collections::HashMap;
use std::collections::HashSet;
use super::database::{BlockChainDatabase, PROP_TREE_LEADER_VEC_CF};
use std::sync::{Arc, Mutex};
use bincode::{deserialize, serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Ord, Eq, PartialEq, PartialOrd, Hash)]
/// The metadata of a proposer block.
pub struct NodeData {
    /// Level of the proposer node.
    pub level: u32,
    /// Leadership Status.
    pub leadership_status: Status,
    /// Number of votes from voter blocks on the main chains (longest chains).
    pub votes: u16,
}

// TODO: remove it and replace with a new() function
impl Default for NodeData {
    fn default() -> Self {
        let level = 0;
        let leadership_status = Status::PotentialLeader;
        return Self {
            level,
            leadership_status,
            votes: 0,
        };
    }
}

impl std::fmt::Display for NodeData {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "level: {}; #votes: {}", self.level, self.votes)?; // Ignoring status for now
        Ok(())
    }
}

impl NodeData {
    // TODO: either make `votes` and `leadership_status` private, or remove those functions
    pub fn increment_vote(&mut self) {
        self.votes += 1;
    }
    pub fn decrement_vote(&mut self) {
        self.votes -= 1;
    }
    pub fn give_leader_status(&mut self) {
        self.leadership_status = Status::Leader
    }
    pub fn give_potential_leader_status(&mut self) {
        self.leadership_status = Status::PotentialLeader
    }
    pub fn give_not_leader_status(&mut self) {
        self.leadership_status = Status::NotLeaderUnconfirmed
    }
    pub fn give_not_leader_confirmed_status(&mut self) {
        self.leadership_status = Status::NotLeaderAndConfirmed
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Ord, Eq, PartialEq, PartialOrd, Hash, Debug)]
/// The leader status of a proposer block.
pub enum Status {
    /// The leader of this level.
    Leader,
    /// Will be later used for fast active confirmation.
    PotentialLeader,
    /// When a leader block at a level is confirmed, rest of the proposer blocks at that level become `NotLeaderUnconfirmed`
    NotLeaderUnconfirmed,
    /// When a proposer block is not a leader, and has been confirmed by any of the child
    /// leader blocks.
    NotLeaderAndConfirmed,
}

impl NodeData {
    /// Generates the metadata of the genesis block.
    pub fn genesis(number_of_voter_chains: u16) -> Self {
        let mut genesis = NodeData::default();
        genesis.leadership_status = Status::Leader;
        genesis.votes = number_of_voter_chains;
        return genesis;
    }
}

//#[derive(Clone, Eq, PartialEq)]
/// The metadata of a proposer block tree.
pub struct Tree {
    pub db: Arc<Mutex<BlockChainDatabase>>,
    /// The best proposer node on the tree (the node with the deepest level). This info is for mining.
    pub best_block: H256,
    /// The deepest level. This is for mining.
    pub best_level: u32,
    /// The hashes of proposer blocks, stored by level.
    pub prop_nodes: Vec<Vec<H256>>,
    /// The number of votes at each level.
    pub number_of_votes: HashMap<u32, u32>, // TODO: why are we using hashmap here?
    /// The level upto which all levels have a leader.
    pub min_unconfirmed_level: u32,
    /// The pool of unreferred proposer blocks. This is for mining.
    pub unreferred: HashSet<H256>,
}

impl Tree {
    pub fn new(db: Arc<Mutex<BlockChainDatabase>>) -> Self {
        let best_block = H256::default();
        let prop_nodes: Vec<Vec<H256>> = vec![];
        let all_votes: HashMap<u32, u32> = HashMap::<u32, u32>::new();
        let unreferred: HashSet<H256> = HashSet::new();
        return Self {
            db,
            best_block,
            best_level: 0,
            prop_nodes,
            number_of_votes: all_votes,
            min_unconfirmed_level: 1,
            unreferred,
        };
    }
    /// Adds a proposer block at the given level.
    pub fn add_block_at_level(&mut self, block: H256, level: u32) {
        if self.best_level >= level {
            self.prop_nodes[level as usize].push(block);
        } else if self.best_level == level - 1 {
            self.prop_nodes.push(vec![block]); // start a new level
            self.best_block = block;
            self.best_level = level;
        } else {
            panic!("Trying to insert a new proposer block at level greater than best level + 1.")
        }
    }

    /// Adds a vote to the given level.
    pub fn increment_vote_at_level(&mut self, level: u32) {
        *self.number_of_votes.entry(level).or_insert(0) += 1;
    }

    /// Inserts an entry to the unreferred proposer block list.
    pub fn insert_unreferred(&mut self, hash: H256) {
        self.unreferred.insert(hash);
    }

    /// Remove an entry from the unreferred proposer block list.
    pub fn remove_unreferred(&mut self, hash: &H256) {
        self.unreferred.remove(hash);
    }

    /// Adds a leader at level 'level'
    pub fn insert_leader_block(&mut self, level: u32, hash: H256) {
        let db = self.db.lock().unwrap();
        let key = serialize(&level).unwrap();
        let value = serialize(&hash).unwrap();
        let cf = db.handle.cf_handle(PROP_TREE_LEADER_VEC_CF).unwrap();
        let serialized = db.handle.get_cf(cf, &key).unwrap();
        match serialized {
            Some(_) => {panic!("The leader the level {} exists", level)},
            None => {
                db.handle.put_cf(cf, &key, &value);
            },
        }
    }

    /// Deletes the leader block at level 'level'
    pub fn remove_leader_block(&mut self, level: u32) {
        let db = self.db.lock().unwrap();
        let key = serialize(&level).unwrap();
        let cf = db.handle.cf_handle(PROP_TREE_LEADER_VEC_CF).unwrap();
        match db.handle.delete_cf(cf, &key) {
            Ok(_) => {},
            Err(e) => {  panic!("Database error {}", e)
            },
        }
    }

    /// Get the leader block at level 'level'
    pub fn get_leader_block_at(&mut self, level: u32) -> H256 {
        let db = self.db.lock().unwrap();
        let key = serialize(&level).unwrap();
        let cf = db.handle.cf_handle(PROP_TREE_LEADER_VEC_CF).unwrap();
        let serialized_option = db.handle.get_cf(cf, &key).unwrap();
        match serialized_option {
            Some(serialized) => {return deserialize(&serialized).unwrap()},
            None => { panic!("No leader block at level {}", level)},
        }
    }

    /// Checks if level 'level' contains a leader block
    pub fn contains_leader_block_at(&mut self, level: u32) -> bool {
        let db = self.db.lock().unwrap();
        let key = serialize(&level).unwrap();
        let cf = db.handle.cf_handle(PROP_TREE_LEADER_VEC_CF).unwrap();
        let serialized_option = db.handle.get_cf(cf, &key).unwrap();
        match serialized_option {
            Some(_) => {return true},
            None => { return false},
        }
    }


}

impl std::fmt::Display for Tree {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "best_block: {}; best_level: {};",
            self.best_block, self.best_level
        )?; // Ignoring status for now
        Ok(())
    }
}
