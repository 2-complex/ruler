use crate::ticket::{TicketFactory, Ticket};
use crate::system::
{
    System,
    ReadWriteError,
};
use std::collections::HashMap;
use serde::
{
    Serialize,
    Deserialize,
};
use std::fmt;
use std::io::
{
    Read,
    Write,
};

/*  Takes a System, a path a a str and a vector of binary data.  Supplants the file at the given path in the
    filesystem with the binary content.  If file-opening fails, this function echoes the std::io error. */
pub fn write_file
<
    SystemType : System,
>
(
    system : &mut SystemType,
    file_path : &str,
    content : &[u8]
)
-> Result<(), ReadWriteError>
{
    match system.create_file(file_path)
    {
        Ok(mut file) =>
        {
            match file.write_all(&content)
            {
                Ok(_) => return Ok(()),
                Err(error) => return Err(ReadWriteError::IOError(error)),
            }
        }
        Err(error) => return Err(ReadWriteError::SystemError(error)),
    }
}

/*  Recall that a Rule is three things: sources, targets and command.  For each particular rule, a RuleHistory stores
    the Tickets of target files recorded when the command executed and built a given rule.

    This is what Ruler uses to determine if targets are up-to-date.  It creates a ticket based on the current
    state of the rule, and indexes by that ticket into a RuleHistory to get target-tickets.  If the target
    tickets match, then the targets are up-to-date. */
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct RuleHistory
{
    /*  Each rule history consists of a map
            key = source-ticket
            value = a target ticket for each target */
    source_to_targets : HashMap<Ticket, Vec<Ticket>>,
}

/*  Inserting target tickets in a RuleHistory can go wrong in a couple ways.
    Either there's already something there, which suggests user error, or the number
    of target tickets is wrong, which suggests a logical error in the code. */
pub enum RuleHistoryInsertError
{
    Contradiction(Vec<usize>),
    TargetSizesDifferWeird,
}

impl RuleHistory
{
    /*  Create a new rule history with empty map. */
    pub fn new() -> RuleHistory
    {
        RuleHistory
        {
            source_to_targets : HashMap::new()
        }
    }

    /*  With the given source_ticket, add the given target_tickets to the history.
        If there's a contradiction, constructs a Contradiction with a vector of indices. */
    pub fn insert(
        &mut self,
        source_ticket: &Ticket,
        target_tickets: Vec<Ticket>)
    -> Result<(), RuleHistoryInsertError>
    {
        match self.source_to_targets.get(source_ticket)
        {
            Some(existing_tickets) =>
            {
                let elen : usize = existing_tickets.len();

                if elen != target_tickets.len()
                {
                    return Err(RuleHistoryInsertError::TargetSizesDifferWeird)
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
                        return Err(RuleHistoryInsertError::Contradiction(contradicting_indices))
                    }
                }
            },
            None =>
            {
                self.source_to_targets.insert(source_ticket.clone(), target_tickets);
                Ok(())
            }
        }

    }

    pub fn get_target_tickets(&self, source_ticket: &Ticket) -> Option<Vec<Ticket>>
    {
        match self.source_to_targets.get(source_ticket)
        {
            Some(target_tickets) => Some(target_tickets.clone()),
            None => None,
        }
    }
}

impl fmt::Display for RuleHistory
{
    /*  Displaying the rule history shows the source tickets' hashes and the target hashe
        with indentation showing which is which. */
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        let mut out = String::new();

        for (source_ticket, target_tickets) in self.source_to_targets.iter()
        {
            out.push_str("  ");
            out.push_str(&source_ticket.base64());
            out.push_str("\n");

            for target_ticket in target_tickets.iter()
            {
                out.push_str("    ");
                out.push_str(&target_ticket.base64());
                out.push_str("\n");
            }
        }

        write!(formatter, "{}", out)
    }
}

/*  The main purpose of TargetHistory is to be the value in the map 'target_histories'  target_histories
    help ruler tell if a target is up-to-date.  Ostensibly, to tell if the target is up to date, we only
    need to know the current Ticket.  But recall there's also an optimization where if the current target's
    timestamp is exactly the same we don't bother checking the Ticket.  That's why this object contains
    a ticket and timestamp. */
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct TargetHistory
{
    pub ticket : Ticket,
    pub timestamp : u64,
}

impl TargetHistory
{
    /*  Create a new empty TargetHistory */
    pub fn empty() -> TargetHistory
    {
        TargetHistory
        {
            ticket : TicketFactory::new().result(),
            timestamp : 0,
        }
    }

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

/*  Memory includes both the rule-histories and target-histories.  Recall that:
    rule_histories: For a given rule-hash stores the previously witnessed hashes of the targets built by that rule.
    target_histories: For a given target (file path) stores the most recently observed hash of that target along
        with the modified timestamp for the file at that time. */
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct Memory
{
    rule_histories : HashMap<Ticket, RuleHistory>,

    /*  Map target path to target-history */
    target_histories : HashMap<String, TargetHistory>,
}

/*  When accessing memory, a few things can go wrong.  Memory is stored in a file, so that file could be unreadable or
    corrupt.  These would mean that user has tried to modify files that ruler depends on to to work.  Serialization
    of an empty history could fail, which would indicate a logical error in this source code. */
pub enum MemoryError
{
    CannotReadMemoryFile(String),
    CannotInterpretMemoryFile(String),
    CannotRecordHistoryFile(String),
    CannotSerializeEmptyHistoryWeird,
}

/*  Display a MemoryError by printing a reasonable error message.  Of course, during everyday Ruler use, these
    will not likely display. */
impl fmt::Display for MemoryError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            MemoryError::CannotReadMemoryFile(path) =>
                write!(formatter, "Cannot read memory file: {}", path),

            MemoryError::CannotInterpretMemoryFile(path) =>
                write!(formatter, "Cannot interpret memory file: {}", path),

            MemoryError::CannotRecordHistoryFile(path) =>
                write!(formatter, "Cannot record history file: {}", path),

            MemoryError::CannotSerializeEmptyHistoryWeird =>
                write!(formatter, "Cannot serialize empty history... that's weird"),
        }
    }
}

/*  Opens file at a path and deserializaes contents to create a Memory object. */
fn read_all_memory_from_file<SystemType : System>
(
    system : &mut SystemType,
    memoryfile_path : &str
)
-> Result<Memory, MemoryError>
{
    match system.open(memoryfile_path)
    {
        Ok(mut file) =>
        {
            let mut content = Vec::new();
            match file.read_to_end(&mut content)
            {
                Ok(_size) =>
                {
                    match bincode::deserialize(&content)
                    {
                        Ok(memory) => Ok(memory),
                        Err(_) => Err(MemoryError::CannotInterpretMemoryFile(memoryfile_path.to_string())),
                    }
                }
                Err(_) => Err(MemoryError::CannotReadMemoryFile(memoryfile_path.to_string())),
            }
        },
        Err(_) => Err(MemoryError::CannotReadMemoryFile(memoryfile_path.to_string())),
    }
}

impl Memory
{
    /*  Create a new Memory object from a file in a filesystem, create it if it doesn't exist, and If file fails to
        open or is corrupt, generate an appropriate MemoryError. */
    pub fn from_file<SystemType: System>(
        system: &mut SystemType,
        path_as_str : &str)
        -> Result<Memory, MemoryError>
    {
        if system.is_file(path_as_str)
        {
            return read_all_memory_from_file(system, path_as_str);
        }
        else
        {
            let memory = Memory::new();
            match bincode::serialize(&memory)
            {
                Ok(bytes) => match write_file(system, path_as_str, &bytes)
                {
                    Err(_) => Err(MemoryError::CannotRecordHistoryFile(path_as_str.to_string())),
                    Ok(()) => Ok(memory),
                },
                Err(_error) => Err(MemoryError::CannotSerializeEmptyHistoryWeird),
            }
        }
    }

    /*  Write a memory object to a file in a filesystem. */
    pub fn to_file<SystemType: System>(
        &self,
        system: &mut SystemType,
        path_as_str : &str
    ) -> Result<(), MemoryError>
    {
        match write_file(system, path_as_str, &bincode::serialize(&self).unwrap())
        {
            Err(_) => Err(MemoryError::CannotRecordHistoryFile(path_as_str.to_string())),
            Ok(_) => Ok(()),
        }
    }

    /*  Create a new, empty Memory */
    fn new() -> Memory
    {
        Memory
        {
            rule_histories : HashMap::new(),
            target_histories : HashMap::new(),
        }
    }

    /*  For testing, it is useful to create a mock Memory with specific source and target tickets. */
    #[cfg(test)]
    fn insert(&mut self, rule_ticket: Ticket, source_ticket: Ticket, target_tickets: Vec<Ticket>)
    {
        let rule_history = self.rule_histories.entry(rule_ticket).or_insert(
            RuleHistory
            {
                source_to_targets: HashMap::new(),
            }
        );

        match rule_history.insert(&source_ticket, target_tickets)
        {
            Ok(_) => {},
            Err(_) => panic!("Insert broken"),
        }
    }

    /*  Insert a RuleHistory for a given rule. */
    pub fn insert_rule_history(&mut self, rule_ticket: Ticket, rule_history: RuleHistory)
    {
        self.rule_histories.insert(rule_ticket, rule_history);
    }

    /*  Retrive a RuleHisotry for a given rule.  Note: this function removes the RuleHistory from Memory, and transfers
        ownership to the caller. */
    pub fn take_rule_history(&mut self, rule_ticket: &Ticket) -> RuleHistory
    {
        match self.rule_histories.remove(rule_ticket)
        {
            Some(rule_history) => rule_history,
            None => RuleHistory::new(),
        }
    }

    /*  Adds the given TargetHistory to the map for the given file-path. */
    pub fn insert_target_history(&mut self, target_path: String, target_history : TargetHistory)
    {
        self.target_histories.insert(target_path, target_history);
    }

    /*  Retrieve a TargetHistory by the target path.  Note: this function removes the TargetHistory from Memory,
        and transfers ownership of the TargetHistory to the caller.

        If a target history is not present in the map, this function returns a new, empty history instead. */
    pub fn take_target_history(&mut self, target_path: &str) -> TargetHistory
    {
        match self.target_histories.remove(target_path)
        {
            Some(target_history) => target_history,
            None => TargetHistory::empty(),
        }
    }
}

/*  Display a Memory by printing the rule-histories, nevermind about the target-histories. */
impl fmt::Display for Memory
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        let mut out = String::new();

        out.push_str("Rule Histories:\n");

        for (rule_ticket, rule_history) in self.rule_histories.iter()
        {
            out.push_str(&rule_ticket.base64());
            out.push_str("\n");
            out.push_str(&format!("{}", rule_history))
        }

        write!(formatter, "{}", out)
    }
}

#[cfg(test)]
mod test
{
    use crate::system::fake::FakeSystem;
    use crate::memory::
    {
        RuleHistory,
        Memory,
        TargetHistory,
        write_file,
    };
    use crate::ticket::{TicketFactory};
    use crate::system::util::read_file;

    /*  Create a Memory, fill it with rule-histories and target-histories, then serialize it to binary, and deserialize
        to create a new Memory. Check that the contents of the new Memory are the same as the old one. */
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

    /*  Create a Memory, fill it with rule-histories and target-histories, then write it to a file in a filesystem,
        read back from that same file to create a new Memory and check that new Memory contents are the same as the
        old one. */
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

        let mut system = FakeSystem::new(10);

        let encoded: Vec<u8> = bincode::serialize(&mem).unwrap();
        match write_file(&mut system, "memory.file", &encoded)
        {
            Ok(()) =>
            {
                match read_file(&mut system, "memory.file")
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

    /*  Create a Memory, fill it with rule-histories and target-histories, then write it to a file in a filesystem,
        read back from that same file to create a new Memory and check that new Memory contents are the same as the
        old one.  This time using the functions to_file and from_file */
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

        let mut system = FakeSystem::new(10);

        match memory.to_file(&mut system, "memory.file")
        {
            Ok(()) => {},
            Err(_) => panic!("Memory failed to write into file"),
        }

        match Memory::from_file(&mut system, "memory.file")
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

    /*  Create a RuleHistory, populate with some mock target tickets, serialize the RuleHistory, then make a new
        RuleHistory by deserializing.  Read the target tickets and check that they're the same as what we started
        with. */
    #[test]
    fn round_trip_history()
    {
        let mut history = RuleHistory::new();
        match history.insert(&TicketFactory::from_str("source").result(),
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

    /*  Construct a couple rule-histories and use 'insert' to add and remove them from a Memory.  Then check their
        presence in the Memory is as expected */
    #[test]
    fn add_remove_rules()
    {
        let mut history_a = RuleHistory::new();
        match history_a.insert(&TicketFactory::from_str("sourceA").result(),
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
        match history_b.insert(&TicketFactory::from_str("sourceB").result(),
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

        let history = memory.take_rule_history(&TicketFactory::from_str("ruleA").result());

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

        let empty_history = memory.take_rule_history(&TicketFactory::from_str("ruleA").result());
        assert_eq!(empty_history, RuleHistory::new());

        let history = memory.take_rule_history(&TicketFactory::from_str("ruleB").result());
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

    /*  Make a Memory and insert a target-history.  Then take out the target history, and make sure it matches when was
        inserted. */
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

    /*  Make a Memory and insert a target-history.  Then take ask to see a history from a different path, and make sure
        the history returned is empty. */
    #[test]
    fn history_of_unknown_file_empty()
    {
        let mut memory = Memory::new();

        let target_history = TargetHistory::new(
            TicketFactory::from_str("main(){}").result(), 17123);

        memory.insert_target_history("src/meta.c".to_string(), target_history);
        let history = memory.take_target_history("src/math.cpp");

        let empty_target_history = TargetHistory::empty();

        assert_eq!(history.ticket, empty_target_history.ticket);
        assert_eq!(history.timestamp, empty_target_history.timestamp);
    }
}
