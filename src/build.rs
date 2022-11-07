extern crate multimap;

use multimap::MultiMap;

use std::thread;
use std::sync::mpsc::{Sender, Receiver};
use std::sync::mpsc;
use std::str::from_utf8;
use std::fmt;
use std::io::
{
    self,
    Read,
};

use crate::rule::
{
    parse_all,
    ParseError,
    Node,
    topological_sort,
    topological_sort_all,
    TopologicalSortError,
};
use crate::packet::Packet;
use crate::blob::
{
    TargetFileInfo,
    FileResolution,
};
use crate::work::
{
    WorkOption,
    WorkResult,
    WorkError,
    handle_node,
    clean_targets,
};

use crate::memory::{Memory, MemoryError};
use crate::cache::LocalCache;
use crate::printer::Printer;

use termcolor::
{
    Color,
};

use crate::system::
{
    System,
    SystemError
};

/*  Takes a vector of Nodes, iterates through them, and creates two multimaps, one for
    senders and one for receivers. */
fn make_multimaps(nodes : &Vec<Node>)
    -> (
        MultiMap<usize, (usize, Sender<Packet>)>,
        MultiMap<usize, Receiver<Packet>>
    )
{
    let mut senders : MultiMap<usize, (usize, Sender<Packet>)> = MultiMap::new();
    let mut receivers : MultiMap<usize, Receiver<Packet>> = MultiMap::new();

    for (target_index, node) in nodes.iter().enumerate()
    {
        for (source_index, sub_index) in node.source_indices.iter()
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
    RuleFileFailedToRead(String, io::Error),
    RuleFileFailedToOpen(String, SystemError),
    WorkErrors(Vec<WorkError>),
    RuleFileFailedToParse(ParseError),
    TopologicalSortFailed(TopologicalSortError),
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

            BuildError::RuleFileFailedToParse(error) =>
                write!(formatter, "{}", error),

            BuildError::TopologicalSortFailed(error) =>
                write!(formatter, "Dependence search failed: {}", error),

            BuildError::RuleFileFailedToRead(path, error) =>
                write!(formatter, "Build file {} failed to read with error: {}", path, error),

            BuildError::RuleFileFailedToOpen(path, error) =>
                write!(formatter, "Build file {} failed to open with error: {}", path, error),

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
    FailedToCreateDirectory(SystemError),
    FailedToCreateCacheDirectory(SystemError),
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

pub fn init_directory<SystemType : System + Clone + Send + 'static>
(
    system : &mut SystemType,
    directory : &str
)
->
Result<(Memory, LocalCache, String), InitDirectoryError>
{
    if ! system.is_dir(directory)
    {
        match system.create_dir(directory)
        {
            Ok(_) => {},
            Err(error) => return Err(InitDirectoryError::FailedToCreateDirectory(error)),
        }
    }

    let cache_path = format!("{}/cache", directory);

    if ! system.is_dir(&cache_path)
    {
        match system.create_dir(&cache_path)
        {
            Ok(_) => {},
            Err(error) => return Err(InitDirectoryError::FailedToCreateCacheDirectory(error)),
        }
    }

    let memoryfile = format!("{}/memory", directory);

    Ok((
        match Memory::from_file(system, &memoryfile)
        {
            Ok(memory) => memory,
            Err(error) => return Err(InitDirectoryError::FailedToReadMemoryFile(error)),
        },
        LocalCache::new(&cache_path),
        memoryfile
    ))
}


fn read_all_rules<SystemType : System>
(
    system : &SystemType,
    mut rulefile_paths : Vec<String>
)
-> Result<Vec<(String, String)>, BuildError>
{
    let mut result : Vec<(String, String)> = vec![];
    for rulefile_path in rulefile_paths.drain(..)
    {
        match system.open(&rulefile_path)
        {
            Ok(mut file) =>
            {
                let mut rule_content = Vec::new();
                match file.read_to_end(&mut rule_content)
                {
                    Ok(_size) =>
                    {
                        match from_utf8(&rule_content)
                        {
                            Ok(rule_text) => result.push((rulefile_path, rule_text.to_string())),
                            Err(_) => return Err(BuildError::RuleFileNotUTF8),
                        }
                    },
                    Err(error) => return Err(
                        BuildError::RuleFileFailedToRead(
                            rulefile_path.to_string(), error)),
                }
            },
            Err(error) => return Err(
                BuildError::RuleFileFailedToOpen(
                    rulefile_path.to_string(), error)),
        }
    }

    Ok(result)
}

/*  This is the function that runs when you type "ruler nodes" at the commandline.
    It opens the rulefile, parses it, and returns the vector of rule Nodes. */
pub fn get_nodes
<
    SystemType : System + Clone + Send + 'static,
>
(
    system : &SystemType,
    rulefile_paths : Vec<String>,
    goal_target_opt: Option<String>
)
-> Result<Vec<Node>, BuildError>
{
    let all_rule_text = read_all_rules(system, rulefile_paths)?;

    let rules =
    match parse_all(all_rule_text)
    {
        Ok(rules) => rules,
        Err(error) => return Err(BuildError::RuleFileFailedToParse(error)),
    };

    Ok(
        match goal_target_opt
        {
            Some(goal_target) =>
            {
                match topological_sort(rules, &goal_target)
                {
                    Ok(nodes) => nodes,
                    Err(error) => return Err(BuildError::TopologicalSortFailed(error)),
                }
            },
            None =>
            {
                match topological_sort_all(rules)
                {
                    Ok(nodes) => nodes,
                    Err(error) => return Err(BuildError::TopologicalSortFailed(error)),
                }
            }
        }
    )
}


/*  This is the function that runs when you type "ruler build" at the commandline.
    It opens the rulefile, parses it, and then either updates all targets in all rules
    or, if goal_target_opt is Some, only the targets that are ancestors of goal_target_opt
    in the dependence graph. */
pub fn build
<
    SystemType : System + Clone + Send + 'static,
    PrinterType : Printer,
>
(
    mut system : SystemType,
    directory : &str,
    rulefile_paths : Vec<String>,
    goal_target_opt: Option<String>,
    printer: &mut PrinterType,
)
-> Result<(), BuildError>
{
    let (mut memory, cache, memoryfile) =
    match init_directory(&mut system, directory)
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

    let mut nodes = get_nodes(&system, rulefile_paths, goal_target_opt)?;

    let (mut senders, mut receivers) = make_multimaps(&nodes);
    let mut handles = Vec::new();
    let mut index : usize = 0;

    for mut node in nodes.drain(..)
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

        let mut target_infos = vec![];
        for target_path in node.targets.drain(..)
        {
            target_infos.push(
                TargetFileInfo
                {
                    history : memory.take_target_history(&target_path),
                    path : target_path,
                }
            );
        }

        let local_cache_clone = cache.clone();

        let command = node.command;
        let rule_history =  match &node.rule_ticket
        {
            Some(ticket) => Some(memory.take_rule_history(&ticket)),
            None => None,
        };
        let system_clone = system.clone();

        handles.push(
            (
                node.rule_ticket,
                thread::spawn(
                    move || -> Result<WorkResult, WorkError>
                    {
                        handle_node(
                            target_infos,
                            command,
                            rule_history,
                            system_clone,
                            sender_vec,
                            receiver_vec,
                            local_cache_clone)
                    }
                )
            )
        );

        index+=1;
    }

    let mut work_errors = Vec::new();

    for (node_ticket, handle) in handles
    {
        match handle.join()
        {
            Err(_error) => return Err(BuildError::Weird),
            Ok(work_result_result) =>
            {
                match work_result_result
                {
                    Ok(mut work_result) =>
                    {
                        match work_result.work_option
                        {
                            WorkOption::SourceOnly =>
                            {
                            },

                            WorkOption::Resolutions(resolutions) =>
                            {
                                for (i, target_info) in work_result.target_infos.iter().enumerate()
                                {
                                    let (banner_text, banner_color) =
                                        match resolutions[i]
                                        {
                                            FileResolution::Recovered =>
                                                (" Recovered", Color::Green),

                                            FileResolution::Downloaded =>
                                                ("Downloaded", Color::Yellow),

                                            FileResolution::AlreadyCorrect =>
                                                ("Up-to-date", Color::Cyan),

                                            FileResolution::NeedsRebuild =>
                                                ("  Outdated", Color::Red),
                                        };

                                    printer.print_single_banner_line(banner_text, banner_color, &target_info.path);
                                }
                            },

                            WorkOption::CommandExecuted(output) =>
                            {
                                for target_info in work_result.target_infos.iter()
                                {
                                    printer.print_single_banner_line("     Built", Color::Magenta, &target_info.path);  
                                }

                                if output.out != ""
                                {
                                    printer.print(&output.out);
                                }

                                if output.err != ""
                                {
                                    printer.error(&output.err);
                                }

                                if !output.success
                                {
                                    printer.error(
                                        &format!("RESULT: {}", 
                                            match output.code
                                            {
                                                Some(code) => format!("{}", code),
                                                None => "None".to_string(),
                                            }
                                        )
                                    );
                                }

                            },
                        }

                        match node_ticket
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

                        for target_info in work_result.target_infos.drain(..)
                        {
                            memory.insert_target_history(target_info.path, target_info.history);
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

    match memory.to_file(&mut system, &memoryfile)
    {
        Ok(_) => {},
        Err(_) => printer.error("Error writing history"),
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

/*  This is the function that runs when you type "ruler clean" at the command-line.
    It takes a rulefile, parses it and either removes all targets to the cache,
    or, if goal_target_opt is Some, removes only those targets that are acnestors
    of goal_target_opt in the depdnece-graph. */
pub fn clean<SystemType : System + Clone + Send + 'static>
(
    mut system : SystemType,
    directory : &str,
    rulefile_paths: Vec<String>,
    goal_target_opt: Option<String>
)
-> Result<(), BuildError>
{
    let (mut memory, cache, _memoryfile) =
    match init_directory(&mut system, directory)
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

    let rules =
    match parse_all(read_all_rules(&system, rulefile_paths)?)
    {
        Ok(rules) => rules,
        Err(error) => return Err(BuildError::RuleFileFailedToParse(error)),
    };

    let mut nodes =
    match goal_target_opt
    {
        Some(goal_target) =>
        {
            match topological_sort(rules, &goal_target)
            {
                Ok(nodes) => nodes,
                Err(error) => return Err(BuildError::TopologicalSortFailed(error)),
            }
        },
        None =>
        {
            match topological_sort_all(rules)
            {
                Ok(nodes) => nodes,
                Err(error) => return Err(BuildError::TopologicalSortFailed(error)),
            }
        }
    };

    let mut handles = Vec::new();

    for mut node in nodes.drain(..)
    {
        let mut target_infos = Vec::new();
        for target_path in node.targets.drain(..)
        {
            target_infos.push(
                TargetFileInfo
                {
                    history : memory.take_target_history(&target_path),
                    path : target_path,
                }
            );
        }

        let mut system_clone = system.clone();
        let local_cache_clone = cache.clone();

        match node.rule_ticket
        {
            Some(_ticket) =>
                handles.push(
                    thread::spawn(
                        move || -> Result<(), WorkError>
                        {
                            clean_targets(
                                target_infos,
                                &mut system_clone,
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

#[cfg(test)]
mod test
{
    use crate::build::
    {
        build,
        init_directory,
        BuildError,
    };
    use crate::system::
    {
        System,
        fake::FakeSystem
    };
    use crate::work::WorkError;
    use crate::ticket::TicketFactory;
    use crate::cache::LocalCache;
    use crate::system::util::
    {
        write_str_to_file,
        read_file_to_string
    };
    use crate::printer::EmptyPrinter;
    use crate::blob::
    {
        TargetHistory
    };

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
        let mut system = FakeSystem::new(10);

        match write_str_to_file(&mut system, "verse1.txt", "Roses are red.\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File failed to write"),
        }

        match write_str_to_file(&mut system, "verse2.txt", "Violets are violet.\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File failed to write"),
        }

        match write_str_to_file(&mut system, "test.rules", rules)
        {
            Ok(_) => {},
            Err(_) => panic!("File failed to write"),
        }

        match build(
            system.clone(),
            "test.directory",
            vec!["test.rules".to_string()],
            Some("poem.txt".to_string()),
            &mut EmptyPrinter::new())
        {
            Ok(()) => {},
            Err(error) => panic!("Unexpected build error: {}", error),
        }

        match read_file_to_string(&mut system, "poem.txt")
        {
            Ok(text) => assert_eq!(text, "Roses are red.\nViolets are violet.\n"),
            Err(_) => panic!("Failed to read poem."),
        }
    }

    /*  Set up a filesystem and a .rules file with one real dependence missing
        from the rules.  Build once, make sure it goes as planned, then change
        the contents of the omitted source file.  Check that Building again produces
        a particular error: a contradiction. */
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
        let mut system = FakeSystem::new(10);

        match system.create_dir(".ruler-cache")
        {
            Ok(_) => {},
            Err(_) => panic!("Failed to create directory"),
        }

        match write_str_to_file(&mut system, "verse1.txt", "Roses are red.\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File failed to write"),
        }

        match write_str_to_file(&mut system, "verse2.txt", "Violets are blue.\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File failed to write"),
        }

        match write_str_to_file(&mut system, "test.rules", rules)
        {
            Ok(_) => {},
            Err(_) => panic!("File failed to write"),
        }

        match build(
            system.clone(),
            "test.directory",
            vec!["test.rules".to_string()],
            Some("poem.txt".to_string()),
            &mut EmptyPrinter::new())
        {
            Ok(()) => {},
            Err(error) => panic!("Unexpected build error: {}", error),
        }

        system.time_passes(1);

        match read_file_to_string(&mut system, "poem.txt")
        {
            Ok(text) => assert_eq!(text, "Roses are red.\nViolets are blue.\n"),
            Err(_) => panic!("Failed to read poem."),
        }

        match write_str_to_file(&mut system, "verse2.txt", "Violets are violet.\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File failed to write."),
        }

        match write_str_to_file(&mut system, "poem.txt", "Wrong content forcing a rebuild")
        {
            Ok(_) => {},
            Err(_) => panic!("File failed to write."),
        }

        match build(
            system.clone(),
            "test.directory",
            vec!["test.rules".to_string()],
            Some("poem.txt".to_string()),
            &mut EmptyPrinter::new())
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

        match read_file_to_string(&mut system, "poem.txt")
        {
            Ok(text) => assert_eq!(text, "Roses are red.\nViolets are violet.\n"),
            Err(_) => panic!("Failed to read poem."),
        }
    }

    /*  Set up filesystem to build a poem with two verses.  */
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
        let mut system = FakeSystem::new(10);

        match write_str_to_file(&mut system, "verse1.txt", "Roses are red.\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File failed to write"),
        }

        match write_str_to_file(&mut system, "verse2.txt", "Violets are blue.\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File failed to write"),
        }

        match write_str_to_file(&mut system, "test.rules", rules)
        {
            Ok(_) => {},
            Err(_) => panic!("File failed to write"),
        }

        match build(
            system.clone(),
            ".ruler",
            vec!["test.rules".to_string()],
            Some("poem.txt".to_string()),
            &mut EmptyPrinter::new())
        {
            Ok(()) => {},
            Err(error) => panic!("Unexpected build error: {}", error),
        }

        system.time_passes(1);

        match read_file_to_string(&mut system, "poem.txt")
        {
            Ok(text) => assert_eq!(text, "Roses are red.\nViolets are blue.\n"),
            Err(_) => panic!("Failed to read poem."),
        }

        let ticket =
        match TicketFactory::from_file(&system, "poem.txt")
        {
            Ok(mut factory) => factory.result(),
            Err(_) => panic!("Failed to make ticket?"),
        };

        match write_str_to_file(&mut system, "verse2.txt", "Violets are violet.\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File failed to write."),
        }

        match build(
            system.clone(),
            ".ruler",
            vec!["test.rules".to_string()],
            Some("poem.txt".to_string()),
            &mut EmptyPrinter::new())
        {
            Ok(()) => {},
            Err(error) => panic!("Unexpected build error: {}", error),
        }

        match read_file_to_string(&mut system, "poem.txt")
        {
            Ok(text) => assert_eq!(text, "Roses are red.\nViolets are violet.\n"),
            Err(_) => panic!("Poem failed to be utf8?"),
        }

        let cache = LocalCache::new(".ruler/cache");
        cache.restore_file(&mut system, &ticket, "temp-poem.txt");

        match read_file_to_string(&mut system, "temp-poem.txt")
        {
            Ok(text) => assert_eq!(text, "Roses are red.\nViolets are blue.\n"),
            Err(_) => panic!("Poem failed to be utf8?"),
        }

        match cache.back_up_file_with_ticket(&mut system, &ticket, "temp-poem.txt")
        {
            Ok(_) => {},
            Err(_) => panic!("Failed to back up temp-poem"),
        }

        match write_str_to_file(&mut system, "verse2.txt", "Violets are blue.\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File failed to write"),
        }

        match build(
            system.clone(),
            ".ruler",
            vec!["test.rules".to_string()],
            Some("poem.txt".to_string()),
            &mut EmptyPrinter::new())
        {
            Ok(()) => {},
            Err(error) => panic!("Unexpected build error: {}", error),
        }

        match read_file_to_string(&mut system, "poem.txt")
        {
            Ok(text) => assert_eq!(text, "Roses are red.\nViolets are blue.\n"),
            Err(_) => panic!("Failed to read poem a second time."),
        }
    }


    #[test]
    fn build_check_target_history()
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
        let mut system = FakeSystem::new(17);

        match write_str_to_file(&mut system, "verse1.txt", "Roses are red.\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File failed to write"),
        }

        match write_str_to_file(&mut system, "verse2.txt", "Violets are violet.\n")
        {
            Ok(_) => {},
            Err(error) => panic!("File failed to write: {}", error),
        }

        match write_str_to_file(&mut system, "test.rules", rules)
        {
            Ok(_) => {},
            Err(error) => panic!("File failed to write: {}", error),
        }

        {
            let (mut memory, _cache, _memoryfile) =
            match init_directory(&mut system, "ruler-directory")
            {
                Ok((memory, cache, memoryfile)) => (memory, cache, memoryfile),
                Err(error) => panic!("Failed to init directory error: {}", error)
            };

            let target_history_before = memory.take_target_history("poem.txt");
            assert_eq!(target_history_before, TargetHistory::empty());
            memory.insert_target_history("poem.txt".to_string(), target_history_before);
        }

        match build(
            system.clone(),
            "ruler-directory",
            vec!["test.rules".to_string()],
            Some("poem.txt".to_string()),
            &mut EmptyPrinter::new())
        {
            Ok(()) => {},
            Err(error) => panic!("Unexpected build error: {}", error),
        }

        match read_file_to_string(&mut system, "poem.txt")
        {
            Ok(text) => assert_eq!(text, "Roses are red.\nViolets are violet.\n"),
            Err(_) => panic!("Failed to read poem."),
        }

        {
            let (mut memory, _cache, _memoryfile) =
            match init_directory(&mut system, "ruler-directory")
            {
                Ok((memory, cache, memoryfile)) => (memory, cache, memoryfile),
                Err(error) => panic!("Failed to init directory error: {}", error)
            };

            let target_history = memory.take_target_history("poem.txt");

            let exemplar_target_history = TargetHistory::new(
                TicketFactory::from_str("Roses are red.\nViolets are violet.\n").result(), 17);

            assert_eq!(target_history, exemplar_target_history);
            memory.insert_target_history("poem.txt".to_string(), target_history);
        }

    }

}
