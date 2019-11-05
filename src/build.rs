extern crate filesystem;
extern crate multimap;

use filesystem::FileSystem;
use multimap::MultiMap;

use std::thread;
use std::sync::mpsc::{Sender, Receiver};
use std::sync::mpsc;
use std::str::from_utf8;
use std::fmt;
use std::io::Error;

use crate::rule::{Record, parse, topological_sort};
use crate::packet::Packet;
use crate::work::
{
    WorkResult,
    WorkError,
    build_targets,
    clean_targets,
    upload_targets
};
use crate::metadata::MetadataGetter;
use crate::executor::Executor;
use crate::station::{Station, TargetFileInfo};
use crate::memory::{Memory, MemoryError};
use crate::cache::LocalCache;


fn make_multimaps(records : &Vec<Record>)
    -> (
        MultiMap<usize, (usize, Sender<Packet>)>,
        MultiMap<usize, (Receiver<Packet>)>
    )
{
    let mut senders : MultiMap<usize, (usize, Sender<Packet>)> = MultiMap::new();
    let mut receivers : MultiMap<usize, (Receiver<Packet>)> = MultiMap::new();

    for (target_index, record) in records.iter().enumerate()
    {
        for (source_index, sub_index) in record.source_indices.iter()
        {
            let (sender, receiver) : (Sender<Packet>, Receiver<Packet>) = mpsc::channel();
            senders.insert(*source_index, (*sub_index, sender));
            receivers.insert(target_index, receiver);
        }
    }

    (senders, receivers)
}


pub enum BuildError
{
    MemoryFileFailedToRead(MemoryError),
    RuleFileNotUTF8,
    RuleFileFailedToOpen(String, Error),
    WorkErrors(Vec<WorkError>),
    RuleFileFailedToParse(String),
    TopologicalSortFailed(String),
    DirectoryMalfunction,
    Weird,
}


impl fmt::Display for BuildError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            BuildError::MemoryFileFailedToRead(error) =>
                write!(formatter, "Error history file not found: {}", error),

            BuildError::RuleFileNotUTF8 =>
                write!(formatter, "Rule file not valid UTF8."),

            BuildError::RuleFileFailedToOpen(path, error) =>
                write!(formatter, "Rule file did not open: {}\n{}", path, error),

            BuildError::RuleFileFailedToParse(error) =>
                write!(formatter, "{}", error),

            BuildError::TopologicalSortFailed(error) =>
                write!(formatter, "Dependence search failed: {}", error),

            BuildError::WorkErrors(work_errors) =>
            {
                let mut error_text = String::new();
                for work_error in work_errors.iter()
                {
                    error_text.push_str(&format!("{}\n", work_error));
                }
                write!(formatter, "{}", error_text)
            },

            BuildError::DirectoryMalfunction =>
                write!(formatter, "Error while managing ruler directory."),

            BuildError::Weird =>
                write!(formatter, "Weird! How did you do that!"),
        }
    }
}

pub enum InitDirectoryError
{
    FailedToCreateDirectory(Error),
    FailedToCreateCacheDirectory(Error),
    FailedToReadMemoryFile(MemoryError),
}

impl fmt::Display for InitDirectoryError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            InitDirectoryError::FailedToCreateDirectory(error) =>
                write!(formatter, "Failed to create directory: {}", error),

            InitDirectoryError::FailedToCreateCacheDirectory(error) =>
                write!(formatter, "Failed to create cache directory: {}", error),

            InitDirectoryError::FailedToReadMemoryFile(error) =>
                write!(formatter, "Failed to read memory file: {}", error),
        }
    }
}

pub fn init_directory<
    FileSystemType : FileSystem
        + Clone + Send + 'static
>(
    file_system : &mut FileSystemType,
    directory : &str
)
-> Result<(Memory, LocalCache, String), InitDirectoryError>
{
    if ! file_system.is_dir(directory)
    {
        match file_system.create_dir(directory)
        {
            Ok(_) => {},
            Err(error) => return Err(InitDirectoryError::FailedToCreateDirectory(error)),
        }
    }

    let cache_path = format!("{}/cache", directory);

    if ! file_system.is_dir(&cache_path)
    {
        match file_system.create_dir(&cache_path)
        {
            Ok(_) => {},
            Err(error) => return Err(InitDirectoryError::FailedToCreateCacheDirectory(error)),
        }
    }

    let memoryfile = format!("{}/memory", directory);

    Ok((
        match Memory::from_file(file_system, &memoryfile)
        {
            Ok(memory) => memory,
            Err(error) => return Err(InitDirectoryError::FailedToReadMemoryFile(error)),
        },
        LocalCache::new(&cache_path),
        memoryfile
    ))
}

pub fn build<
    FileSystemType : FileSystem
        + Clone + Send + 'static,
    ExecType : Executor
        + Clone + Send + 'static,
    MetadataGetterType : MetadataGetter
        + Clone + Send + 'static
>(
    mut file_system : FileSystemType,
    executor : ExecType,
    metadata_getter: MetadataGetterType,
    directory : &str,
    rulefile: &str,
    target: &str
)
-> Result<(), BuildError>
{
    let (mut memory, cache, memoryfile) =
    match init_directory(&mut file_system, directory)
    {
        Ok((memory, cache, memoryfile)) => (memory, cache, memoryfile),
        Err(error) =>
        {
            return match error
            {
                InitDirectoryError::FailedToReadMemoryFile(memory_error) =>
                    Err(BuildError::MemoryFileFailedToRead(memory_error)),
                _ => Err(BuildError::DirectoryMalfunction),
            }
        }
    };

    let rule_text =
    match file_system.read_file(rulefile)
    {
        Ok(rule_content) =>
        {
            match from_utf8(&rule_content)
            {
                Ok(rule_text) => rule_text.to_owned(),
                Err(_) => return Err(BuildError::RuleFileNotUTF8),
            }
        },
        Err(error) => return Err(BuildError::RuleFileFailedToOpen(rulefile.to_string(), error)),
    };

    let rules =
    match parse(rule_text)
    {
        Ok(rules) => rules,
        Err(error) => return Err(BuildError::RuleFileFailedToParse(error)),
    };

    let mut records =
    match topological_sort(rules, &target)
    {
        Ok(records) => records,
        Err(error) => return Err(BuildError::TopologicalSortFailed(error)),
    };

    let (mut senders, mut receivers) = make_multimaps(&records);
    let mut handles = Vec::new();
    let mut index : usize = 0;

    for mut record in records.drain(..)
    {
        let sender_vec = match senders.remove(&index)
        {
            Some(v) => v,
            None => vec![],
        };

        let receiver_vec = match receivers.remove(&index)
        {
            Some(v) => v,
            None => vec![],
        };

        let mut target_infos = Vec::new();
        for target_path in record.targets.drain(..)
        {
            target_infos.push(
                TargetFileInfo
                {
                    history : memory.take_target_history(&target_path),
                    path : target_path,
                }
            );
        }

        let station = Station::new(
            target_infos,
            record.command,
            match &record.rule_ticket
            {
                Some(ticket) => Some(memory.get_rule_history(&ticket)),
                None => None,
            },
            file_system.clone(),
            metadata_getter.clone(),
        );

        let executor_clone = executor.clone();
        let local_cache_clone = cache.clone();

        handles.push(
            (
                record.rule_ticket,
                thread::spawn(
                    move || -> Result<WorkResult, WorkError>
                    {
                        build_targets(
                            station,
                            sender_vec,
                            receiver_vec,
                            executor_clone,
                            local_cache_clone)
                    }
                )
            )
        );

        index+=1;
    }

    let mut work_errors = Vec::new();

    for (record_ticket, handle) in handles
    {
        match handle.join()
        {
            Err(_error) => return Err(BuildError::Weird),
            Ok(work_result_result) =>
            {
                match work_result_result
                {
                    Ok(work_result) =>
                    {
                        match work_result.command_line_output
                        {
                            Some(output) =>
                            {
                                for target_info in work_result.target_infos.iter()
                                {
                                    println!("{}", target_info.path);
                                }

                                if output.out != ""
                                {
                                    println!("{}", output.out);
                                }

                                if output.err != ""
                                {
                                    eprintln!("ERROR:\n{}", output.err);
                                }

                                if !output.success
                                {
                                    eprintln!("RESULT: {}", 
                                        match output.code
                                        {
                                            Some(code) => format!("{}", code),
                                            None => "None".to_string(),
                                        }
                                    );
                                }

                            },
                            None => {},
                        }

                        match record_ticket
                        {
                            Some(ticket) =>
                            {
                                match work_result.rule_history
                                {
                                    Some(history) => memory.insert_rule_history(ticket, history),
                                    None => {},
                                }
                            }
                            None => {},
                        }
                    },
                    Err(work_error) =>
                    {
                        match work_error
                        {
                            WorkError::ReceiverError(_error) => {},
                            WorkError::SenderError => {},

                            _ =>
                            {
                                work_errors.push(work_error);
                            }
                        }
                    },
                }
            }
        }
    }

    if work_errors.len() == 0
    {
        match memory.to_file(&mut file_system, &memoryfile)
        {
            Ok(_) => {},    
            Err(_) => eprintln!("Error writing history"),
        }

        Ok(())
    }
    else
    {
        Err(BuildError::WorkErrors(work_errors))
    }
}

pub fn clean<
    FileSystemType : FileSystem
        + Clone + Send + 'static,
    MetadataGetterType : MetadataGetter
        + Clone + Send + 'static
>(
    mut file_system : FileSystemType,
    metadata_getter: MetadataGetterType,
    directory : &str,
    rulefile: &str,
    target: &str
)
-> Result<(), BuildError>
{
    let (mut memory, cache, _memoryfile) =
    match init_directory(&mut file_system, directory)
    {
        Ok((memory, cache, memoryfile)) => (memory, cache, memoryfile),
        Err(error) =>
        {
            return match error
            {
                InitDirectoryError::FailedToReadMemoryFile(memory_error) =>
                    Err(BuildError::MemoryFileFailedToRead(memory_error)),
                _ => Err(BuildError::DirectoryMalfunction),
            }
        }
    };

    let rule_text =
    match file_system.read_file(rulefile)
    {
        Ok(rule_content) =>
        {
            match from_utf8(&rule_content)
            {
                Ok(rule_text) => rule_text.to_owned(),
                Err(_) => return Err(BuildError::RuleFileNotUTF8),
            }
        },
        Err(error) => return Err(BuildError::RuleFileFailedToOpen(rulefile.to_string(), error)),
    };

    let rules =
    match parse(rule_text)
    {
        Ok(rules) => rules,
        Err(error) => return Err(BuildError::RuleFileFailedToParse(error)),
    };

    let mut records =
    match topological_sort(rules, &target)
    {
        Ok(records) => records,
        Err(error) => return Err(BuildError::TopologicalSortFailed(error)),
    };

    let mut handles = Vec::new();

    for mut record in records.drain(..)
    {
        let mut target_infos = Vec::new();
        for target_path in record.targets.drain(..)
        {
            target_infos.push(
                TargetFileInfo
                {
                    history : memory.take_target_history(&target_path),
                    path : target_path,
                }
            );
        }

        let file_system_clone = file_system.clone();
        let metadata_getter_clone = metadata_getter.clone();
        let local_cache_clone = cache.clone();

        match record.rule_ticket
        {
            Some(_ticket) =>
                handles.push(
                    thread::spawn(
                        move || -> Result<(), WorkError>
                        {
                            clean_targets(
                                target_infos,
                                &file_system_clone,
                                &metadata_getter_clone,
                                &local_cache_clone)
                        }
                    )
                ),
            None => {},
        }
    }

    let mut work_errors = Vec::new();

    for handle in handles
    {
        match handle.join()
        {
            Err(_error) => return Err(BuildError::Weird),
            Ok(remove_result_result) =>
            {
                match remove_result_result
                {
                    Ok(_) => {},
                    Err(work_error) =>
                    {
                        match work_error
                        {
                            _ =>
                            {
                                work_errors.push(work_error);
                            }
                        }
                    },
                }
            }
        }
    }

    if work_errors.len() == 0
    {
        Ok(())
    }
    else
    {
        Err(BuildError::WorkErrors(work_errors))
    }
}

pub fn upload<
    FileSystemType : FileSystem
        + Clone + Send + 'static,
>(
    file_system : FileSystemType,
    rulefile : &str,
    target : &str,
    server_url : &str,
)
-> Result<(), BuildError>
{
    let rule_text =
    match file_system.read_file(rulefile)
    {
        Ok(rule_content) =>
        {
            match from_utf8(&rule_content)
            {
                Ok(rule_text) => rule_text.to_owned(),
                Err(_) => return Err(BuildError::RuleFileNotUTF8),
            }
        },
        Err(error) => return Err(BuildError::RuleFileFailedToOpen(rulefile.to_string(), error)),
    };

    let rules =
    match parse(rule_text)
    {
        Ok(rules) => rules,
        Err(error) => return Err(BuildError::RuleFileFailedToParse(error)),
    };

    let mut records =
    match topological_sort(rules, &target)
    {
        Ok(records) => records,
        Err(error) => return Err(BuildError::TopologicalSortFailed(error)),
    };

    let mut handles = Vec::new();

    for mut record in records.drain(..)
    {
        let mut target_paths = Vec::new();
        for target_path in record.targets.drain(..)
        {
            target_paths.push(target_path);
        }

        let file_system_clone = file_system.clone();
        let server_url_clone = server_url.to_string();

        match record.rule_ticket
        {
            Some(_ticket) =>
                handles.push(
                    thread::spawn(
                        move || -> Result<(), WorkError>
                        {
                            upload_targets(
                                &file_system_clone,
                                target_paths,
                                server_url_clone)
                        }
                    )
                ),
            None => {},
        }
    }

    let mut work_errors = Vec::new();

    for handle in handles
    {
        match handle.join()
        {
            Err(_error) => return Err(BuildError::Weird),
            Ok(remove_result_result) =>
            {
                match remove_result_result
                {
                    Ok(_) => {},
                    Err(work_error) =>
                    {
                        match work_error
                        {
                            _ =>
                            {
                                work_errors.push(work_error);
                            }
                        }
                    },
                }
            }
        }
    }

    if work_errors.len() == 0
    {
        Ok(())
    }
    else
    {
        Err(BuildError::WorkErrors(work_errors))
    }
}

#[cfg(test)]
mod test
{
    use crate::build::{build, BuildError};
    use crate::executor::{FakeExecutor};
    use crate::metadata::FakeMetadataGetter;
    use crate::work::WorkError;
    use crate::ticket::TicketFactory;
    use crate::cache::LocalCache;

    use filesystem::{FileSystem, FakeFileSystem};
    use std::str::from_utf8;

    #[test]
    fn build_basic()
    {
        let rules = "\
poem.txt
:
verse1.txt
verse2.txt
:
mycat
verse1.txt
verse2.txt
poem.txt
:
";
        let file_system = FakeFileSystem::new();
        let executor = FakeExecutor::new(file_system.clone());
        let metadata_getter = FakeMetadataGetter::new();

        match file_system.write_file("verse1.txt", "Roses are red.\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File failed to write"),
        }

        match file_system.write_file("verse2.txt", "Violets are violet.\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File failed to write"),
        }

        match file_system.write_file("test.rules", rules)
        {
            Ok(_) => {},
            Err(_) => panic!("File failed to write"),
        }

        match build(
            file_system.clone(),
            executor,
            metadata_getter,
            "test.directory",
            "test.rules",
            "poem.txt")
        {
            Ok(()) => {},
            Err(error) => panic!("Unexpected build error: {}", error),
        }

        match file_system.read_file("poem.txt")
        {
            Ok(content) =>
            {
                match from_utf8(&content)
                {
                    Ok(text) => assert_eq!(text, "Roses are red.\nViolets are violet.\n"),
                    Err(_) => panic!("Poem failed to be utf8?"),
                }
            }
            Err(_) => panic!("Failed to read poem."),
        }
    }

    #[test]
    fn build_with_missing_source()
    {
        let rules = "\
poem.txt
:
verse1.txt
:
mycat
verse1.txt
verse2.txt
poem.txt
:
";
        let file_system = FakeFileSystem::new();

        match file_system.create_dir(".ruler-cache")
        {
            Ok(_) => {},
            Err(_) => panic!("Failed to create directory"),
        }

        let executor = FakeExecutor::new(file_system.clone());
        let metadata_getter = FakeMetadataGetter::new();

        match file_system.write_file("verse1.txt", "Roses are red.\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File failed to write"),
        }

        match file_system.write_file("verse2.txt", "Violets are blue.\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File failed to write"),
        }

        match file_system.write_file("test.rules", rules)
        {
            Ok(_) => {},
            Err(_) => panic!("File failed to write"),
        }

        match build(
            file_system.clone(),
            executor.clone(),
            metadata_getter.clone(),
            "test.directory",
            "test.rules",
            "poem.txt")
        {
            Ok(()) => {},
            Err(error) => panic!("Unexpected build error: {}", error),
        }

        match file_system.read_file("poem.txt")
        {
            Ok(content) =>
            {
                match from_utf8(&content)
                {
                    Ok(text) => assert_eq!(text, "Roses are red.\nViolets are blue.\n"),
                    Err(_) => panic!("Poem failed to be utf8?"),
                }
            }
            Err(_) => panic!("Failed to read poem."),
        }

        match file_system.write_file("verse2.txt", "Violets are violet.\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File failed to write."),
        }

        match file_system.write_file("poem.txt", "Wrong content forcing a rebuild")
        {
            Ok(_) => {},
            Err(_) => panic!("File failed to write."),
        }

        match build(
            file_system.clone(),
            executor.clone(),
            metadata_getter.clone(),
            "test.directory",
            "test.rules",
            "poem.txt")
        {
            Ok(()) => panic!("Unexpected silence when contradiction should arise"),
            Err(error) =>
            {
                match error
                {
                    BuildError::WorkErrors(work_errors) =>
                    {
                        assert_eq!(work_errors.len(), 1);
                        match &work_errors[0]
                        {
                            WorkError::Contradiction(paths) => assert_eq!(paths, &vec!["poem.txt".to_string()]),
                            _ => panic!("Wrong type of WorkError"),
                        }
                    }
                    _ => panic!("Wrong type of error"),
                }
            },
        }

        match file_system.read_file("poem.txt")
        {
            Ok(content) =>
            {
                match from_utf8(&content)
                {
                    Ok(text) => assert_eq!(text, "Roses are red.\nViolets are violet.\n"),
                    Err(_) => panic!("Poem failed to be utf8?"),
                }
            }
            Err(_) => panic!("Failed to read poem."),
        }
    }

    #[test]
    fn build_change_build_check_cache()
    {
        let rules = "\
poem.txt
:
verse1.txt
verse2.txt
:
mycat
verse1.txt
verse2.txt
poem.txt
:
";
        let file_system = FakeFileSystem::new();
        let executor = FakeExecutor::new(file_system.clone());
        let metadata_getter = FakeMetadataGetter::new();

        match file_system.write_file("verse1.txt", "Roses are red.\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File failed to write"),
        }

        match file_system.write_file("verse2.txt", "Violets are blue.\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File failed to write"),
        }

        match file_system.write_file("test.rules", rules)
        {
            Ok(_) => {},
            Err(_) => panic!("File failed to write"),
        }

        match build(
            file_system.clone(),
            executor.clone(),
            metadata_getter.clone(),
            ".ruler",
            "test.rules",
            "poem.txt")
        {
            Ok(()) => {},
            Err(error) => panic!("Unexpected build error: {}", error),
        }

        match file_system.read_file("poem.txt")
        {
            Ok(content) =>
            {
                match from_utf8(&content)
                {
                    Ok(text) => assert_eq!(text, "Roses are red.\nViolets are blue.\n"),
                    Err(_) => panic!("Poem failed to be utf8?"),
                }
            }
            Err(_) => panic!("Failed to read poem."),
        }

        let ticket =
        match TicketFactory::from_file(&file_system, "poem.txt")
        {
            Ok(mut factory) => factory.result(),
            Err(_) => panic!("Failed to make ticket?"),
        };

        match file_system.write_file("verse2.txt", "Violets are violet.\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File failed to write."),
        }

        match build(
            file_system.clone(),
            executor.clone(),
            metadata_getter.clone(),
            ".ruler",
            "test.rules",
            "poem.txt")
        {
            Ok(()) => {},
            Err(error) => panic!("Unexpected build error: {}", error),
        }

        match file_system.read_file("poem.txt")
        {
            Ok(content) =>
            {
                match from_utf8(&content)
                {
                    Ok(text) => assert_eq!(text, "Roses are red.\nViolets are violet.\n"),
                    Err(_) => panic!("Poem failed to be utf8?"),
                }
            }
            Err(_) => panic!("Failed to read poem a second time."),
        }

        let cache = LocalCache::new(".ruler/cache");
        cache.restore_file(&file_system, &ticket, "temp-poem.txt");

        match file_system.read_file("temp-poem.txt")
        {
            Ok(content) =>
            {
                match from_utf8(&content)
                {
                    Ok(text) => assert_eq!(text, "Roses are red.\nViolets are blue.\n"),
                    Err(_) => panic!("Poem failed to be utf8?"),
                }
            }
            Err(_) => panic!("Failed to read temp-poem."),
        }

        match cache.back_up_file_with_ticket(&file_system, &ticket, "temp-poem.txt")
        {
            Ok(_) => {},
            Err(_) => panic!("Failed to back up temp-poem"),
        }

        match file_system.write_file("verse2.txt", "Violets are blue.\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File failed to write"),
        }

        match build(
            file_system.clone(),
            executor.clone(),
            metadata_getter.clone(),
            ".ruler",
            "test.rules",
            "poem.txt")
        {
            Ok(()) => {},
            Err(error) => panic!("Unexpected build error: {}", error),
        }

        match file_system.read_file("poem.txt")
        {
            Ok(content) =>
            {
                match from_utf8(&content)
                {
                    Ok(text) => assert_eq!(text, "Roses are red.\nViolets are blue.\n"),
                    Err(_) => panic!("Poem failed to be utf8?"),
                }
            }
            Err(_) => panic!("Failed to read poem a second time."),
        }
    }

}
