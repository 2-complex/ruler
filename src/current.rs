use crate::ticket::{Ticket};
use crate::system::
{
    System,
    ReadWriteError,
};
use crate::blob::
{
    TargetHistory,
    TargetTickets,
    BlobError,
};
use std::collections::HashMap;
use serde::
{
    Serialize,
    Deserialize
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

/*  Memory contains a map associating each target path to the the most recently observed state of that path
    encoded in a struct called TargetHistory. */
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct Memory
{
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
            target_histories : HashMap::new(),
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
    use crate::blob::
    {
        TargetTickets,
    };
    use crate::ticket::{TicketFactory};
    use crate::system::util::read_file;

    /*  Create a Memory, fill it with rule-histories and target-histories, then serialize it to binary, and deserialize
        to create a new Memory. Check that the contents of the new Memory are the same as the old one. */
    #[test]
    fn round_trip_memory()
    {
        let mut mem = Memory::new();

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

    /*  Create a RuleHistory, populate with some mock target tickets, serialize the RuleHistory, then make a new
        RuleHistory by deserializing.  Read the target tickets and check that they're the same as what we started
        with. */
    #[test]
    fn round_trip_history()
    {
        let mut history = RuleHistory::new();
        match history.insert(TicketFactory::from_str("source").result(),
            TargetTickets::from_vec(vec![
                TicketFactory::from_str("target1").result(),
                TicketFactory::from_str("target2").result(),
                TicketFactory::from_str("target3").result(),
            ]))
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
                assert_eq!(*target_tickets,
                    TargetTickets::from_vec(vec![
                        TicketFactory::from_str("target1").result(),
                        TicketFactory::from_str("target2").result(),
                        TicketFactory::from_str("target3").result(),
                    ]));
            },
            None => panic!("Targets not found"),
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
