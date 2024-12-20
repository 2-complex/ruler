use crate::system::
{
    System,
    ReadWriteError,
};
use crate::blob::
{
    Blob,
    FileState,
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
fn write_file
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
                Err(error) => return Err(ReadWriteError::IOError(format!("{}", error))),
            }
        }
        Err(error) => return Err(ReadWriteError::SystemError(error)),
    }
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct CurrentFileStatesInside
{
    /*  Map target path to target-history */
    file_states : HashMap<String, FileState>,
}

/*  file_states: For a given target (file path) stores the most recently observed hash of that target along
    with the modified timestamp for the file at that time, and whether it is exectuable. */
pub struct CurrentFileStates<SystemType : System>
{
    system_box : Box<SystemType>,
    path : String,
    inside : CurrentFileStatesInside,
}

/*  When accessing current_file_states, a few things can go wrong.  CurrentFileStates is stored in a file, so that file could be unreadable or
    corrupt.  These would mean that user has tried to modify files that ruler depends on to to work.  Serialization
    of an empty history could fail, which would indicate a logical error in this source code. */
#[derive(Debug)]
pub enum CurrentFileStatesError
{
    CannotReadCurrentFileStatesFile(String),
    CannotInterpretFile(String),
    CannotRecordHistoryFile(String)
}

/*  Display a CurrentFileStatesError by printing a reasonable error message.  Of course, during everyday Ruler use, these
    will not likely display. */
impl fmt::Display for CurrentFileStatesError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            CurrentFileStatesError::CannotReadCurrentFileStatesFile(path) =>
                write!(formatter, "Cannot read current_file_states file: {}", path),

            CurrentFileStatesError::CannotInterpretFile(path) =>
                write!(formatter, "Cannot interpret current_file_states file: {}", path),

            CurrentFileStatesError::CannotRecordHistoryFile(path) =>
                write!(formatter, "Cannot record history file: {}", path),
        }
    }
}

impl<SystemType : System> CurrentFileStates<SystemType>
{
    /*  Opens file at a path and deserializaes contents to create a CurrentFileStates object. */
    fn read_all_current_file_states_from_file
    (
        system : SystemType,
        current_file_statesfile_path : String
    )
    -> Result<CurrentFileStates<SystemType>, CurrentFileStatesError>
    {
        let mut file =
        match system.open(&current_file_statesfile_path)
        {
            Ok(file) => file,
            Err(_) => return Err(CurrentFileStatesError::CannotReadCurrentFileStatesFile(current_file_statesfile_path)),
        };

        let mut content = Vec::new();
        match file.read_to_end(&mut content)
        {
            Ok(_size) => {},
            Err(_) => return Err(CurrentFileStatesError::CannotReadCurrentFileStatesFile(current_file_statesfile_path)),
        };

        match bincode::deserialize(&content)
        {
            Ok(inside) => Ok(CurrentFileStates::from_inside(system, current_file_statesfile_path, inside)),
            Err(_) => Err(CurrentFileStatesError::CannotInterpretFile(current_file_statesfile_path)),
        }
    }

    /*  Create a new CurrentFileStates object from a file in a filesystem, create it if it doesn't exist, and If file fails to
        open or is corrupt, generate an appropriate CurrentFileStatesError. */
    pub fn from_file(
        system: SystemType,
        path : String)
        -> Result<CurrentFileStates<SystemType>, CurrentFileStatesError>
    {
        if system.is_file(&path)
        {
            Self::read_all_current_file_states_from_file(system, path)
        }
        else
        {
            let mut current_file_states = CurrentFileStates::new(system, path);
            current_file_states.to_file()?;
            Ok(current_file_states)
        }
    }

    pub fn from_inside(
        system : SystemType,
        path : String,
        inside : CurrentFileStatesInside) -> CurrentFileStates<SystemType>
    {
        CurrentFileStates
        {
            system_box : Box::new(system),
            path : path,
            inside : inside,
        }
    }

    /*  Write a current_file_states object to a file in a filesystem. */
    pub fn to_file(&mut self) -> Result<(), CurrentFileStatesError>
    {
        let system = &mut (*self.system_box);
        match write_file(system, &self.path, &bincode::serialize(&self.inside).unwrap())
        {
            Err(_) => Err(CurrentFileStatesError::CannotRecordHistoryFile(self.path.to_string())),
            Ok(_) => Ok(()),
        }
    }

    /*  Create a new, empty CurrentFileStates */
    fn new(system : SystemType, path : String) -> CurrentFileStates<SystemType>
    {
        CurrentFileStates
        {
            system_box : Box::new(system),
            path : path,
            inside : CurrentFileStatesInside
            {
                file_states : HashMap::new(),
            },
        }
    }

    /*  Adds the given FileState to the map for the given file-path. */
    pub fn insert_file_state(&mut self, target_path: String, file_state : FileState)
    {
        self.inside.file_states.insert(target_path, file_state);
    }

    /*  Takes a vector of paths and returns a blob with current FileStates for those paths.

        If a FileState is not present in the map, this function returns a new, empty FileState instead. */
    pub fn take_blob(
        self : &mut Self,
        paths : Vec<String>) -> Blob
    {
        return Blob::from_paths(paths, |path|
        {
            match self.inside.file_states.remove(path)
            {
                Some(file_state) => file_state,
                None => FileState::empty(),
            }
        });
    }

    pub fn insert_blob(self : &mut Self, blob : Blob)
    {
        for info in blob.get_file_infos().into_iter()
        {
            self.insert_file_state(info.path, info.file_state)
        }
    }
}

#[cfg(test)]
mod test
{
    use crate::system::fake::FakeSystem;
    use crate::current::
    {
        CurrentFileStates,
        FileState,
        Blob,
        write_file,
    };
    use crate::ticket::{TicketFactory};
    use crate::system::util::read_file;

    /*  Create a CurrentFileStates, populate with a FileState, then serialize it to binary, and deserialize
        to create a new CurrentFileStates.  Check that the contents of the new CurrentFileStates are the same
        as the old one. */
    #[test]
    fn round_trip_current_file_states()
    {
        let system = FakeSystem::new(10);
        let mut current_file_states = CurrentFileStates::new(system.clone(), "current_file_states.file".to_string());

        let file_state = FileState::new(
            TicketFactory::from_str("main(){}").result(), 123);

        current_file_states.insert_file_state("src/meta.c".to_string(), file_state);

        let encoded : Vec<u8> = bincode::serialize(&current_file_states.inside).unwrap();
        let inside = bincode::deserialize(&encoded).unwrap();
        let decoded_current_file_states = CurrentFileStates::from_inside(system, "current_file_states.file".to_string(), inside);

        assert_eq!(current_file_states.inside, decoded_current_file_states.inside);
    }

    /*  Create a CurrentFileStates, fill it with rule-histories and target-histories, then write it to a file in a filesystem,
        read back from that same file to create a new CurrentFileStates and check that new CurrentFileStates contents are the same as the
        old one. */
    #[test]
    fn round_trip_current_file_states_through_file()
    {
        let mut system = FakeSystem::new(10);

        let mut current_file_states = CurrentFileStates::new(system.clone(), "current_file_states.file".to_string());
        
        let file_state = FileState::new(
            TicketFactory::from_str("main(){}").result(), 123);

        current_file_states.insert_file_state("src/meta.c".to_string(), file_state);

        let encoded : Vec<u8> = bincode::serialize(&current_file_states.inside).unwrap();
        match write_file(&mut system, "current_file_states.file", &encoded)
        {
            Ok(()) =>
            {
                match read_file(&mut system, "current_file_states.file")
                {
                    Ok(content) =>
                    {
                        assert_eq!(current_file_states.inside, bincode::deserialize(&content).unwrap());
                    },
                    Err(_) => panic!("CurrentFileStates file read failed"),
                }
            },
            Err(_) => panic!("CurrentFileStates file write failed"),
        }
    }

    /*  Create a CurrentFileStates, fill it with rule-histories and target-histories, then write it to a file in a filesystem,
        read back from that same file to create a new CurrentFileStates and check that new CurrentFileStates contents are the same as the
        old one.  This time using the functions to_file and from_file */
    #[test]
    fn round_trip_current_file_states_through_file_to_from()
    {
        let system = FakeSystem::new(10);
        let mut current_file_states = CurrentFileStates::new(system.clone(), "current_file_states.file".to_string());

        let file_state = FileState::new(
            TicketFactory::from_str("main(){}").result(), 123);

        current_file_states.insert_file_state("src/meta.c".to_string(), file_state);

        match current_file_states.to_file()
        {
            Ok(()) => {},
            Err(_) => panic!("CurrentFileStates failed to write into file"),
        }

        match CurrentFileStates::from_file(system, "current_file_states.file".to_string())
        {
            Ok(mut new_current_file_states) =>
            {
                assert_eq!(new_current_file_states.inside, current_file_states.inside);
                assert_eq!(
                    new_current_file_states.take_blob(vec!["src/meta.c".to_string()]),
                    Blob::from_paths(vec!["src/meta.c".to_string()], |_path|
                    {
                        FileState::new(TicketFactory::from_str("main(){}").result(), 123)
                    }));
            },
            Err(_) => panic!("CurrentFileStates failed to read from file"),
        }
    }

    /*  Make a CurrentFileStates and insert a FileState.  Then take out the target history,
        and make sure it matches what was inserted. */
    #[test]
    fn insert_remove_file_state()
    {
        let system = FakeSystem::new(10);
        let mut current_file_states = CurrentFileStates::new(system, "current_file_states.file".to_string());
        let file_state = FileState::new(TicketFactory::from_str("main(){}").result(), 17123);
        current_file_states.insert_file_state("src/meta.c".to_string(), file_state.clone());
        assert_eq!(
            current_file_states.take_blob(vec!["src/meta.c".to_string()]),
            Blob::from_paths(vec!["src/meta.c".to_string()], |_path|{file_state.clone()}));
    }

    /*  Make a CurrentFileStates and insert a FileState.  Then ask to see a blob from a different path, and make sure
        the history returned is empty. */
    #[test]
    fn file_state_of_unknown_file_empty()
    {
        let system = FakeSystem::new(10);
        let mut current_file_states = CurrentFileStates::new(system, "current_file_states.file".to_string());

        let file_state = FileState::new(
            TicketFactory::from_str("main(){}").result(), 17123);

        current_file_states.insert_file_state("src/meta.c".to_string(), file_state);

        assert_eq!(
            current_file_states.take_blob(vec!["src/math.cpp".to_string()]),
            Blob::from_paths(vec!["src/math.cpp".to_string()], |_path|{FileState::empty()}));
    }
}
