extern crate filesystem;

use crate::ticket::{TicketFactory, Ticket};
use filesystem::FileSystem;

use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use std::path::Path;

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct RuleHistory
{
    source_to_targets : HashMap<Ticket, Vec<Ticket>>,
}

pub enum RuleHistoryError
{
    Contradiction(Vec<usize>),
    TargetSizesDifferWeird,
}

impl RuleHistory
{
    pub fn new() -> RuleHistory
    {
        RuleHistory
        {
            source_to_targets : HashMap::new()
        }
    }

    pub fn insert(&mut self, source_ticket: Ticket, target_tickets: Vec<Ticket>) -> Result<(), RuleHistoryError>
    {
        match self.source_to_targets.get(&source_ticket)
        {
            Some(existing_tickets) =>
            {
                let elen : usize = existing_tickets.len();

                if elen != target_tickets.len()
                {
                    return Err(RuleHistoryError::TargetSizesDifferWeird)
                }
                else
                {
                    let mut contradicting_indices = Vec::new();
                    for i in 0..elen
                    {
                        if existing_tickets[i] != target_tickets[i]
                        {
                            contradicting_indices.push(i);
                        }
                    }

                    if contradicting_indices.len() == 0
                    {
                        Ok(())
                    }
                    else
                    {
                        return Err(RuleHistoryError::Contradiction(contradicting_indices))
                    }
                }
            },
            None =>
            {
                self.source_to_targets.insert(source_ticket, target_tickets);
                Ok(())
            }
        }

    }

    pub fn get_target_tickets(&self, source_ticket: &Ticket) -> Option<&Vec<Ticket>>
    {
        self.source_to_targets.get(source_ticket)
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct TargetHistory
{
    pub ticket : Ticket,
    pub timestamp : u64,
}

impl TargetHistory
{
    pub fn empty() -> TargetHistory
    {
        TargetHistory
        {
            ticket : TicketFactory::new().result(),
            timestamp : 0,
        }
    }

    #[cfg(test)]
    pub fn new(
        ticket : Ticket,
        timestamp : u64) -> TargetHistory
    {
        TargetHistory
        {
            ticket : ticket,
            timestamp : timestamp,
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct Memory
{
    rule_histories : HashMap<Ticket, RuleHistory>,
    target_histories : HashMap<String, TargetHistory>,
}

impl Memory
{
    pub fn from_file<FSType: FileSystem>(
        file_system: &mut FSType,
        path_as_str : &str)
        -> Result<Memory, String>
    {
        let path = Path::new(&path_as_str);

        if file_system.is_file(path)
        {
            match file_system.read_file(path)
            {
                Err(_) => Err(format!("Cannot read memory file: {}", path_as_str)),
                Ok(content) =>
                {
                    match bincode::deserialize(&content)
                    {
                        Ok(memory) => Ok(memory),
                        Err(_) => Err(format!("Cannot interpret memory file: {}", path_as_str)),
                    }
                }
            }
        }
        else
        {
            let memory = Memory::new();

            match bincode::serialize(&memory)
            {
                Ok(bytes) => match file_system.write_file(path, bytes)
                {
                    Err(_) => Err(format!("Cannot write history file: {}", path_as_str)),
                    Ok(()) => Ok(memory),
                },
                Err(_error) => Err(format!("Cannot serialize empty history... that's weird")),
            }
        }
    }

    pub fn to_file<FSType: FileSystem>(
        &self, file_system: &mut FSType,
        path_as_str : &str
    ) -> Result<(), String>
    {
        match file_system.write_file(Path::new(&path_as_str), bincode::serialize(&self).unwrap())
        {
            Err(_) => Err(format!("Cannot record history file: {}", path_as_str)),
            Ok(_) => Ok(()),
        }
    }

    fn new() -> Memory
    {
        Memory
        {
            rule_histories : HashMap::new(),
            target_histories : HashMap::new(),
        }
    }

    #[cfg(test)]
    fn insert(&mut self, rule_ticket: Ticket, source_ticket: Ticket, target_tickets: Vec<Ticket>)
    {
        let rule_history = self.rule_histories.entry(rule_ticket).or_insert(
            RuleHistory
            {
                source_to_targets: HashMap::new(),
            }
        );

        match rule_history.insert(source_ticket, target_tickets)
        {
            Ok(_) => {},
            Err(_) => panic!("Insert broken"),
        }
    }

    pub fn insert_rule_history(&mut self, rule_ticket: Ticket, rule_history: RuleHistory)
    {
        self.rule_histories.insert(rule_ticket, rule_history);
    }

    pub fn get_rule_history(&mut self, rule_ticket: &Ticket) -> RuleHistory
    {
        match self.rule_histories.remove(rule_ticket)
        {
            Some(rule_history) => rule_history,
            None => RuleHistory::new(),
        }
    }

    #[cfg(test)]
    pub fn insert_target_history(&mut self, target_path: String, target_history : TargetHistory)
    {
        self.target_histories.insert(target_path, target_history);
    }

    pub fn take_target_history(&mut self, target_path: &str) -> TargetHistory
    {
        match self.target_histories.remove(target_path)
        {
            Some(target_history) => target_history,
            None => TargetHistory::empty(),
        }
    }
}

#[cfg(test)]
mod test
{
    use filesystem::{FileSystem, FakeFileSystem};
    use crate::memory::{RuleHistory, Memory, TargetHistory};
    use crate::ticket::{TicketFactory};

    #[test]
    fn round_trip_memory()
    {
        let mut mem = Memory::new();
        mem.insert(
            TicketFactory::from_str("rule").result(),
            TicketFactory::from_str("source").result(),
            [
                TicketFactory::from_str("target1").result(),
                TicketFactory::from_str("target2").result(),
                TicketFactory::from_str("target3").result(),
            ].to_vec()
        );

        let target_history = TargetHistory::new(
            TicketFactory::from_str("main(){}").result(), 123);

        mem.insert_target_history("src/meta.c".to_string(), target_history);

        let encoded: Vec<u8> = bincode::serialize(&mem).unwrap();
        let mut decoded_mem: Memory = bincode::deserialize(&encoded[..]).unwrap();
        assert_eq!(mem, decoded_mem);
        assert_eq!(mem.target_histories, decoded_mem.target_histories);

        let decoded_history = decoded_mem.take_target_history("src/meta.c");
        assert_eq!(decoded_history.ticket, TicketFactory::from_str("main(){}").result());
    }

    #[test]
    fn round_trip_memory_through_file()
    {
        let mut mem = Memory::new();
        mem.insert(
            TicketFactory::from_str("rule").result(),
            TicketFactory::from_str("source").result(),
            [
                TicketFactory::from_str("target1").result(),
                TicketFactory::from_str("target2").result(),
                TicketFactory::from_str("target3").result(),
            ].to_vec()
        );

        let target_history = TargetHistory::new(
            TicketFactory::from_str("main(){}").result(), 123);

        mem.insert_target_history("src/meta.c".to_string(), target_history);

        let file_system = FakeFileSystem::new();

        let encoded: Vec<u8> = bincode::serialize(&mem).unwrap();
        match file_system.write_file("memory.file", encoded)
        {
            Ok(()) =>
            {
                match file_system.read_file("memory.file")
                {
                    Ok(content) =>
                    {
                        let read_mem: Memory = bincode::deserialize(&content).unwrap();
                        assert_eq!(mem, read_mem);
                    },
                    Err(_) => panic!("Memory file read failed"),
                }
            },
            Err(_) => panic!("Memory file write failed"),
        }
    }

    #[test]
    fn round_trip_memory_through_file_to_from()
    {
        let mut memory = Memory::new();
        memory.insert(
            TicketFactory::from_str("rule").result(),
            TicketFactory::from_str("source").result(),
            [
                TicketFactory::from_str("target1").result(),
                TicketFactory::from_str("target2").result(),
                TicketFactory::from_str("target3").result(),
            ].to_vec()
        );

        let target_history = TargetHistory::new(
            TicketFactory::from_str("main(){}").result(), 123);

        memory.insert_target_history("src/meta.c".to_string(), target_history);

        let mut file_system = FakeFileSystem::new();

        match memory.to_file(&mut file_system, "memory.file")
        {
            Ok(()) => {},
            Err(_) => panic!("Memory failed to write into file"),
        }

        match Memory::from_file(&mut file_system, "memory.file")
        {
            Ok(mut new_memory) =>
            {
                assert_eq!(new_memory, memory);

                assert_eq!(new_memory.rule_histories, memory.rule_histories);
                assert_eq!(new_memory.target_histories, memory.target_histories);

                let new_history = new_memory.take_target_history("src/meta.c");
                assert_eq!(new_history.ticket, TicketFactory::from_str("main(){}").result());
                assert_eq!(new_history.timestamp, 123);
            },
            Err(_) => panic!("Memory failed to read from file"),
        }
    }

    #[test]
    fn round_trip_history()
    {
        let mut history = RuleHistory::new();
        match history.insert(TicketFactory::from_str("source").result(),
            [
                TicketFactory::from_str("target1").result(),
                TicketFactory::from_str("target2").result(),
                TicketFactory::from_str("target3").result(),
            ].to_vec())
        {
            Ok(_) => {},
            Err(_) => panic!("Rule history failed to insert"),
        }

        let encoded: Vec<u8> = bincode::serialize(&history).unwrap();
        let decoded: RuleHistory = bincode::deserialize(&encoded[..]).unwrap();
        assert_eq!(history, decoded);

        match history.get_target_tickets(&TicketFactory::from_str("source").result())
        {
            Some(target_tickets) =>
            {
                assert_eq!(target_tickets.len(), 3);

                assert_eq!(
                    target_tickets[0],
                    TicketFactory::from_str("target1").result()
                );

                assert_eq!(
                    target_tickets[1],
                    TicketFactory::from_str("target2").result()
                );

                assert_eq!(
                    target_tickets[2],
                    TicketFactory::from_str("target3").result()
                );
            },
            None => panic!("Targets not found"),
        }
    }

    #[test]
    fn add_remove_rules()
    {
        let mut history_a = RuleHistory::new();
        match history_a.insert(TicketFactory::from_str("sourceA").result(),
            [
                TicketFactory::from_str("target1A").result(),
                TicketFactory::from_str("target2A").result(),
                TicketFactory::from_str("target3A").result(),
            ].to_vec())
        {
            Ok(_) => {},
            Err(_) => panic!("Rule history failed to insert"),
        }

        let mut history_b = RuleHistory::new();
        match history_b.insert(TicketFactory::from_str("sourceB").result(),
            [
                TicketFactory::from_str("target1B").result(),
                TicketFactory::from_str("target2B").result(),
                TicketFactory::from_str("target3B").result(),
            ].to_vec())
        {
            Ok(_) => {},
            Err(_) => panic!("Rule history failed to insert"),
        }

        let mut memory = Memory::new();

        memory.insert(TicketFactory::from_str("ruleA").result(),
            TicketFactory::from_str("sourceA").result(),
            [
                TicketFactory::from_str("target1A").result(),
                TicketFactory::from_str("target2A").result(),
                TicketFactory::from_str("target3A").result(),
            ].to_vec());

        memory.insert(TicketFactory::from_str("ruleB").result(),
            TicketFactory::from_str("sourceB").result(),
            [
                TicketFactory::from_str("target1B").result(),
                TicketFactory::from_str("target2B").result(),
                TicketFactory::from_str("target3B").result(),
            ].to_vec());

        let history = memory.get_rule_history(&TicketFactory::from_str("ruleA").result());

        assert_eq!(history, history_a);
        match history.get_target_tickets(&TicketFactory::from_str("sourceA").result())
        {
            Some(target_tickets) =>
            {
                assert_eq!(target_tickets.len(), 3);
            },
            None => panic!("Important event missing from hisotry"),
        }

        match history.get_target_tickets(&TicketFactory::from_str("sourceB").result())
        {
            Some(_target_tickets) => panic!("Important event missing from hisotry"),
            None => {},
        }

        let empty_history = memory.get_rule_history(&TicketFactory::from_str("ruleA").result());
        assert_eq!(empty_history, RuleHistory::new());

        let history = memory.get_rule_history(&TicketFactory::from_str("ruleB").result());
        assert_eq!(history, history_b);
        match history.get_target_tickets(&TicketFactory::from_str("sourceB").result())
        {
            Some(target_tickets) =>
            {
                assert_eq!(target_tickets.len(), 3);
            },
            None => panic!("Important event missing from hisotry"),
        }
    }

    #[test]
    fn insert_remove_target_history()
    {
        let mut memory = Memory::new();

        let target_history = TargetHistory::new(
            TicketFactory::from_str("main(){}").result(), 17123);

        memory.insert_target_history("src/meta.c".to_string(), target_history);

        let history = memory.take_target_history("src/meta.c");

        assert_eq!(history.ticket, TicketFactory::from_str("main(){}").result());
        assert_eq!(history.timestamp, 17123);
    }
}
