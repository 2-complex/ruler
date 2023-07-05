extern crate multimap;

use multimap::MultiMap;

use std::thread;
use std::sync::mpsc::
{
    self,
    Sender,
    Receiver,
    SendError,
    RecvError,
};
use std::str::from_utf8;
use std::fmt;
use std::io::
{
    self,
    Read,
};
use serde::Deserialize;
use crate::directory::
{
    self,
    InitDirectoryError
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
use crate::ticket::
{
    Ticket,
    TicketFactory,
};
use crate::packet::
{
    Packet,
    PacketError,
};
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
    HandleNodeInfo,
    RuleExt,
    handle_rule_node,
    handle_source_only_node,
    clean_targets,
};
use crate::cache::
{
    DownloaderCache,
};
use crate::history::
{
    HistoryError,
    DownloaderHistory,
};
use crate::memory::
{
    MemoryError
};
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
use crate::system::util::
{
    read_file_to_string,
    ReadFileToStringError,
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

#[derive(Debug)]
pub enum BuildError
{
    Canceled,
    ReceiverError(RecvError),
    SenderError(SendError<Packet>),
    MemoryFileFailedToRead(MemoryError),
    RuleFileNotUTF8,
    RuleFileFailedToRead(String, io::Error),
    RuleFileFailedToOpen(String, SystemError),
    WorkErrors(Vec<WorkError>),
    RuleFileFailedToParse(ParseError),
    TopologicalSortFailed(TopologicalSortError),
    DirectoryMalfunction,
    HistoryError(HistoryError),
    DownloadUrlsError(DownloadUrlsError),
    WorkError(WorkError),
    Weird,
}


impl fmt::Display for BuildError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            BuildError::Canceled =>
                write!(formatter, "Canceled by a depdendence"),

            BuildError::ReceiverError(error) =>
                write!(formatter, "Failed to recieve anything from source: {}", error),

            BuildError::SenderError(error) =>
                write!(formatter, "Failed to send to dependent: {}", error),

            BuildError::MemoryFileFailedToRead(error) =>
                write!(formatter, "Error history file not found: {}", error),

            BuildError::RuleFileNotUTF8 =>
                write!(formatter, "Rule file not valid UTF8."),

            BuildError::RuleFileFailedToParse(error) =>
                write!(formatter, "{}", error),

            BuildError::TopologicalSortFailed(error) =>
                write!(formatter, "Dependence search failed: {}", error),

            BuildError::RuleFileFailedToRead(path, error) =>
                write!(formatter, "Rules file {} failed to read with error: {}", path, error),

            BuildError::RuleFileFailedToOpen(path, error) =>
                write!(formatter, "Rules file {} failed to open with error: {}", path, error),

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

            BuildError::HistoryError(error) =>
                write!(formatter, "Rule history error: {}", error),

            BuildError::DownloadUrlsError(error) =>
                write!(formatter, "Error while establishing download-urls: {}", error),

            BuildError::WorkError(error) =>
                write!(formatter, "{}", error),

            BuildError::Weird =>
                write!(formatter, "Weird! How did you do that!"),
        }
    }
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
                    Ok(_size) => match from_utf8(&rule_content)
                    {
                        Ok(rule_text) => result.push((rulefile_path, rule_text.to_string())),
                        Err(_) => return Err(BuildError::RuleFileNotUTF8),
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
    SystemType : System,
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

#[derive(Deserialize, PartialEq, Debug)]
struct DownloadUrls
{
    urls: Vec<String>
}

impl DownloadUrls
{
    fn new() -> DownloadUrls
    {
        DownloadUrls
        {
            urls : Vec::new()
        }
    }
}

#[derive(Debug)]
pub enum DownloadUrlsError
{
    FailedToReadFile(ReadFileToStringError),
    TomlDeError(toml::de::Error),
}

impl fmt::Display for DownloadUrlsError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            DownloadUrlsError::FailedToReadFile(error) =>
                write!(formatter, "Failed to create cache directory: {}", error),

            DownloadUrlsError::TomlDeError(error) =>
                write!(formatter, "Download Urls file opened, but failed to parse as toml: {}", error),
        }
    }
}

/*  From the given urls file, read the config file and parse as toml to obtain a DownloadUrlsList */
fn read_download_urls<SystemType : System>
(
    system : &SystemType,
    path_str : &str
)
->
Result<DownloadUrls, DownloadUrlsError>
{
    match read_file_to_string(system, path_str)
    {
        Ok(content_string) =>
        {
            return match toml::from_str(&content_string)
            {
                Ok(config) => Ok(config),
                Err(error) => Err(DownloadUrlsError::TomlDeError(error)),
            }
        },
        Err(error) => return Err(DownloadUrlsError::FailedToReadFile(error)),
    }
}

/*  Takes a vector of receivers, and waits for them all to receive, so it can
    hash together all their results into one Ticket obejct.  Returns an error
    if the receivers error or if the packet produces an error when it tries to
    get the ticket from it. */
fn wait_for_sources_ticket(receiver_vec : Vec<Receiver<Packet>>) -> Result<Ticket, BuildError>
{
    let mut factory = TicketFactory::new();
    for receiver in receiver_vec.iter()
    {
        match receiver.recv()
        {
            Ok(packet) =>
            {
                match packet.get_ticket()
                {
                    Ok(ticket) => factory.input_ticket(ticket),
                    Err(PacketError::Cancel) => return Err(BuildError::Canceled),
                }
            },
            Err(error) => return Err(BuildError::ReceiverError(error)),
        }
    }
    Ok(factory.result())
}

/*  This is the function that runs when you type "ruler build" at the commandline.
    It opens the rulefile, parses it, and then either updates all targets in all rules
    or, if goal_target_opt is Some, only the targets that are ancestors of goal_target_opt
    in the dependence graph. */
pub fn build
<
    SystemType : System + 'static,
    PrinterType : Printer,
>
(
    mut system : SystemType,
    directory_path : &str,
    rulefile_paths : Vec<String>,
    urlfile_path_opt : Option<String>,
    goal_target_opt: Option<String>,
    printer: &mut PrinterType,
)
-> Result<(), BuildError>
{
    let mut elements =
    match directory::init(&mut system, directory_path)
    {
        Ok(elements) => elements,
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

    let download_urls =
    match urlfile_path_opt
    {
        None => DownloadUrls::new(),
        Some(path_string) =>
        {
            match read_download_urls(&system, &path_string)
            {
                Ok(download_urls) => download_urls,
                Err(error) => return Err(BuildError::DownloadUrlsError(error)),
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
                    history : elements.memory.take_target_history(&target_path),
                    path : target_path,
                }
            );
        }

        let mut downloader_cache_urls = vec![];
        let mut downloader_history_urls = vec![];

        for url in &download_urls.urls
        {
            downloader_cache_urls.push(format!("{}/files", url));
            downloader_history_urls.push(format!("{}/rules", url));
        }

        let downloader_cache = DownloaderCache::new(downloader_cache_urls);
        let downloader_history = DownloaderHistory::new(downloader_history_urls);
        let system_clone = system.clone();

        handles.push(
            (
                node.rule_ticket.clone(),
                match &node.rule_ticket
                {
                    None =>
                    {
                        thread::spawn(
                            move || -> Result<WorkResult, BuildError>
                            {
                                match handle_source_only_node(system_clone, target_infos)
                                {
                                    Ok(result) =>
                                    {
                                        for (sub_index, sender) in sender_vec
                                        {
                                            match sender.send(Packet::from_ticket(result.target_tickets[sub_index].clone()))
                                            {
                                                Ok(_) => {},
                                                Err(error) => return Err(BuildError::SenderError(error)),
                                            }
                                        }
                                        Ok(result)
                                    },
                                    Err(error) =>
                                    {
                                        for (_sub_index, sender) in sender_vec
                                        {
                                            match sender.send(Packet::cancel())
                                            {
                                                Ok(_) => {},
                                                Err(error) => return Err(BuildError::SenderError(error)),
                                            }
                                        }
                                        Err(BuildError::WorkError(error))
                                    },
                                }
                            }
                        )
                    },
                    Some(ticket) =>
                    {
                        let rule_history = match elements.history.read_rule_history(&ticket)
                        {
                            Ok(rule_history) => rule_history,
                            Err(history_error) => return Err(BuildError::HistoryError(history_error)),
                        };

                        let cache_clone = elements.cache.clone();
                        let downloader_cache_clone = downloader_cache.clone();
                        let downloader_rule_history = downloader_history.get_rule_history(&ticket);

                        thread::spawn(
                            move || -> Result<WorkResult, BuildError>
                            {
                                let mut info = HandleNodeInfo::new(system_clone);
                                info.target_infos = target_infos;

                                let sources_ticket = match wait_for_sources_ticket(receiver_vec)
                                {
                                    Ok(sources_ticket) => sources_ticket,
                                    Err(error) =>
                                    {
                                        for (_sub_index, sender) in sender_vec
                                        {
                                            match sender.send(Packet::cancel())
                                            {
                                                Ok(_) => {},
                                                Err(error) => return Err(BuildError::SenderError(error)),
                                            }
                                        }
                                        return Err(error);
                                    }
                                };

                                match handle_rule_node(info, RuleExt
                                    {
                                        sources_ticket : sources_ticket,
                                        command : node.command,
                                        rule_history : rule_history,
                                        cache : cache_clone,
                                        downloader_cache_opt : Some(downloader_cache_clone),
                                        downloader_rule_history_opt : Some(downloader_rule_history),
                                    })
                                {
                                    Ok(result) =>
                                    {
                                        for (sub_index, sender) in sender_vec
                                        {
                                            match sender.send(Packet::from_ticket(result.target_tickets[sub_index].clone()))
                                            {
                                                Ok(_) => {},
                                                Err(error) => return Err(BuildError::SenderError(error)),
                                            }
                                        }
                                        Ok(result)
                                    },
                                    Err(error) =>
                                    {
                                        for (_sub_index, sender) in sender_vec
                                        {
                                            match sender.send(Packet::cancel())
                                            {
                                                Ok(_) => {},
                                                Err(error) => return Err(BuildError::SenderError(error)),
                                            }
                                        }
                                        Err(BuildError::WorkError(error))
                                    },
                                }
                            }
                        )
                    }
                }
            )
        );

        index+=1;
    }

    let mut work_errors = Vec::new();

    for (node_ticket, handle) in handles
    {
        match handle.join()
        {
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
                                    Some(history) =>
                                    {
                                        match elements.history.write_rule_history(ticket, history)
                                        {
                                            Ok(()) => {},
                                            Err(error) => panic!("Fatal Error: {}", error),
                                        }
                                    },
                                    None => {},
                                }
                            }
                            None => {},
                        }

                        for target_info in work_result.target_infos.drain(..)
                        {
                            elements.memory.insert_target_history(target_info.path, target_info.history);
                        }
                    },
                    Err(BuildError::WorkError(work_error)) => work_errors.push(work_error),
                    Err(BuildError::Canceled) => {},
                    Err(error) => panic!("Unexpected build error: {}", error),
                }
            },
            Err(_error) => return Err(BuildError::Weird),
        }
    }

    match elements.memory.to_file()
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
pub fn clean<SystemType : System + 'static>
(
    mut system : SystemType,
    directory_path : &str,
    rulefile_paths: Vec<String>,
    goal_target_opt: Option<String>
)
-> Result<(), BuildError>
{
    let mut elements =
    match directory::init(&mut system, directory_path)
    {
        Ok(elements) => elements,
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
                    history : elements.memory.take_target_history(&target_path),
                    path : target_path,
                }
            );
        }

        let mut system_clone = system.clone();
        let mut local_cache_clone = elements.cache.clone();

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
                                &mut local_cache_clone)
                        }
                    )
                ),
            None => {},
        }
    }

    let mut work_errors : Vec<WorkError> = Vec::new();

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
                    Err(work_error) => work_errors.push(work_error),
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
    use crate::directory;
    use crate::build::
    {
        build,
        BuildError,
    };
    use crate::system::
    {
        System,
        fake::FakeSystem
    };
    use crate::work::WorkError;
    use crate::ticket::TicketFactory;
    use crate::cache::
    {
        SysCache,
        OpenError,
    };
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
    use std::io::Write;

    /*  Set up a filesystem and a .rules file with one poem depending on two verses
        as source. Populate the verses with lines of the target poem.  Run the build
        command and check that the file appears and has the correct contents. */
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

        write_str_to_file(&mut system, "verse1.txt", "Roses are red.\n").unwrap();
        write_str_to_file(&mut system, "verse2.txt", "Violets are violet.\n").unwrap();
        write_str_to_file(&mut system, "test.rules", rules).unwrap();

        build(
            system.clone(),
            "test.directory",
            vec!["test.rules".to_string()],
            None,
            Some("poem.txt".to_string()),
            &mut EmptyPrinter::new()).unwrap();

        assert_eq!(read_file_to_string(&mut system, "poem.txt").unwrap(), "Roses are red.\nViolets are violet.\n");
    }

    /*  Set up a filesystem and a .rules file with one poem depending on two verses
        as source. Populate the verses with lines of the target poem, except, omit one
        of the source files.  Run the build command and check that it errors sensibly. */
    #[test]
    fn build_one_source_file_missing()
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

        write_str_to_file(&mut system, "verse1.txt", "Roses are red.\n").unwrap();
        write_str_to_file(&mut system, "test.rules", rules).unwrap();

        match build(
            system.clone(),
            "test.directory",
            vec!["test.rules".to_string()],
            None,
            Some("poem.txt".to_string()),
            &mut EmptyPrinter::new())
        {
            Ok(_) => panic!("unexpected success"),
            Err(BuildError::WorkErrors(errors)) =>
            {
                assert_eq!(errors.len(), 1);
                match &errors[0]
                {
                    WorkError::FileNotFound(path_str) => assert_eq!(path_str, "verse2.txt"),
                    _ => panic!("Got work error but not the correct error: {}", errors[0]),
                }
            },
            Err(error) => panic!("Got error but not the correct error: {}", error),
        }
    }

    #[test]
    fn build_one_dependence()
    {
        let rules = "\
stanza1.txt
:
verse1.txt
:
mycat
verse1.txt
stanza1.txt
:

poem.txt
:
stanza1.txt
:
mycat
stanza1.txt
poem.txt
:
";
        let mut system = FakeSystem::new(10);

        write_str_to_file(&mut system, "verse1.txt", "I looked over Jordan, and what did I see?\n").unwrap();
        write_str_to_file(&mut system, "test.rules", rules).unwrap();

        match build(
            system.clone(),
            "test.directory",
            vec!["test.rules".to_string()],
            None,
            Some("poem.txt".to_string()),
            &mut EmptyPrinter::new())
        {
            Ok(_) =>
            {
                assert_eq!(read_file_to_string(&mut system, "poem.txt").unwrap(),
                    "I looked over Jordan, and what did I see?\n");
            },
            Err(error) => panic!("Unexpected error: {}", error),
        }
    }

    #[test]
    fn build_one_dependence_with_intermediate_already_present()
    {
        let rules = "\
stanza1.txt
:
verse1.txt
:
mycat
verse1.txt
stanza1.txt
:

poem.txt
:
stanza1.txt
:
mycat
stanza1.txt
poem.txt
:
";
        let mut system = FakeSystem::new(10);

        write_str_to_file(&mut system, "verse1.txt", "I looked over Jordan, and what did I see?\n").unwrap();
        write_str_to_file(&mut system, "stanza1.txt", "Some wrong content\n").unwrap();
        write_str_to_file(&mut system, "test.rules", rules).unwrap();

        match build(
            system.clone(),
            "test.directory",
            vec!["test.rules".to_string()],
            None,
            Some("poem.txt".to_string()),
            &mut EmptyPrinter::new())
        {
            Ok(_) =>
            {
                assert_eq!(read_file_to_string(&mut system, "poem.txt").unwrap(),
                    "I looked over Jordan, and what did I see?\n");
            },
            Err(error) => panic!("Unexpected error: {}", error),
        }
    }

    /*  Test a more complex build with missing sources.  Make sure the error matches the missing file. */
    #[test]
    fn build_chained_with_missing_sources()
    {
        let rules = "\
stanza1.txt
:
verse1.txt
refrain.txt
:
mycat
verse1.txt
refrain.txt
stanza1.txt
:

stanza2.txt
:
verse2.txt
refrain.txt
:
mycat
verse2.txt
refrain.txt
stanza2.txt
:

poem.txt
:
stanza1.txt
stanza2.txt
:
mycat
stanza1.txt
stanza2.txt
poem.txt
:
";
        for omit_me in ["verse1.txt", "verse2.txt", "refrain.txt"]
        {
            let mut system = FakeSystem::new(10);

            if omit_me != "verse1.txt"
            {
                write_str_to_file(&mut system, "verse1.txt", "I looked over Jordan, and what did I see?\n").unwrap();
            }

            if omit_me != "verse2.txt"
            {
                write_str_to_file(&mut system, "verse2.txt", "A band of angels comin' after me\n").unwrap();
            }

            if omit_me != "refrain.txt"
            {
                write_str_to_file(&mut system, "refrain.txt", "Comin' for to carry me home\n").unwrap();
            }

            write_str_to_file(&mut system, "test.rules", rules).unwrap();

            match build(
                system.clone(),
                "test.directory",
                vec!["test.rules".to_string()],
                None,
                Some("poem.txt".to_string()),
                &mut EmptyPrinter::new())
            {
                Ok(_) => panic!("unexpected success"),
                Err(BuildError::WorkErrors(errors)) =>
                {
                    assert_eq!(errors.len(), 1);
                    match &errors[0]
                    {
                        WorkError::FileNotFound(path_str) => assert_eq!(path_str, omit_me),
                        _ => panic!("When omitting {}, Got work error but not the correct error: {}", omit_me, errors[0]),
                    }
                },
                Err(error) => panic!("When omitting {}, Got error but not the correct error: {}", omit_me, error),
            }
        }
    }

    /*  Set up a filesystem and a .rules file with invalid UTF8 in it instead of rules.
        Check that the build fails with a message about UTF8 */
    #[test]
    fn build_rulefile_not_utf8()
    {
        let mut system = FakeSystem::new(11);

        write_str_to_file(&mut system, "verse1.txt", "Roses are red.\n").unwrap();
        write_str_to_file(&mut system, "verse2.txt", "Violets are violet.\n").unwrap();
        system.create_file("test.rules").unwrap().write_all(&[0x80u8]).unwrap();

        match build(
            system.clone(),
            "test.directory",
            vec!["test.rules".to_string()],
            None,
            Some("poem.txt".to_string()),
            &mut EmptyPrinter::new())
        {
            Ok(_) => panic!("Unexpected success with invalid rules file."),
            Err(BuildError::RuleFileNotUTF8) => {},
            Err(error) => panic!("Got error but not the correct error: {}", error),
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

        system.create_dir(".ruler-cache").unwrap();
        write_str_to_file(&mut system, "verse1.txt", "Roses are red.\n").unwrap();
        write_str_to_file(&mut system, "verse2.txt", "Violets are blue.\n").unwrap();
        write_str_to_file(&mut system, "test.rules", rules).unwrap();

        build(
            system.clone(),
            "test.directory",
            vec!["test.rules".to_string()],
            None,
            Some("poem.txt".to_string()),
            &mut EmptyPrinter::new()).unwrap();

        system.time_passes(1);

        assert_eq!(read_file_to_string(&mut system, "poem.txt").unwrap(), "Roses are red.\nViolets are blue.\n");
        write_str_to_file(&mut system, "verse2.txt", "Violets are violet.\n").unwrap();
        write_str_to_file(&mut system, "poem.txt", "Wrong content forcing a rebuild").unwrap();

        match build(
            system.clone(),
            "test.directory",
            vec!["test.rules".to_string()],
            None,
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

        assert_eq!(read_file_to_string(&mut system, "poem.txt").unwrap(), "Roses are red.\nViolets are violet.\n");
    }

    /*  Set up filesystem to build a poem with two verses.  Invoke the build, and check the resulting poem. */
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

        write_str_to_file(&mut system, "verse1.txt", "Roses are red.\n").unwrap();
        write_str_to_file(&mut system, "verse2.txt", "Violets are blue.\n").unwrap();
        write_str_to_file(&mut system, "test.rules", rules).unwrap();

        build(
            system.clone(),
            ".ruler",
            vec!["test.rules".to_string()],
            None,
            Some("poem.txt".to_string()),
            &mut EmptyPrinter::new()).unwrap();

        system.time_passes(1);

        assert_eq!(read_file_to_string(&mut system, "poem.txt").unwrap(),
            "Roses are red.\nViolets are blue.\n");

        let ticket = TicketFactory::from_file(&system, "poem.txt").unwrap().result();
        write_str_to_file(&mut system, "verse2.txt", "Violets are violet.\n").unwrap();

        build(
            system.clone(),
            ".ruler",
            vec!["test.rules".to_string()],
            None,
            Some("poem.txt".to_string()),
            &mut EmptyPrinter::new()).unwrap();

        assert_eq!(read_file_to_string(&mut system, "poem.txt").unwrap(),
            "Roses are red.\nViolets are violet.\n");

        let mut cache = SysCache::new(system.clone(), ".ruler/cache");
        cache.restore_file(&ticket, "temp-poem.txt");

        assert_eq!(read_file_to_string(&mut system, "temp-poem.txt").unwrap(),
            "Roses are red.\nViolets are blue.\n");

        cache.back_up_file_with_ticket(&ticket, "temp-poem.txt").unwrap();
        write_str_to_file(&mut system, "verse2.txt", "Violets are blue.\n").unwrap();

        build(
            system.clone(),
            ".ruler",
            vec!["test.rules".to_string()],
            None,
            Some("poem.txt".to_string()),
            &mut EmptyPrinter::new()).unwrap();

        assert_eq!(read_file_to_string(&mut system, "poem.txt").unwrap(), "Roses are red.\nViolets are blue.\n");
    }

    /*  Set up filesystem to build a poem with incorrect rules, which say they generate a target, but actually do not. */
    #[test]
    fn build_command_fails_to_generate_target()
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
someotherpoem.txt
:
";
        let mut system = FakeSystem::new(10);

        write_str_to_file(&mut system, "verse1.txt", "Roses are red.\n").unwrap();
        write_str_to_file(&mut system, "verse2.txt", "Violets are blue.\n").unwrap();
        write_str_to_file(&mut system, "test.rules", rules).unwrap();

        match build(
            system.clone(),
            ".ruler",
            vec!["test.rules".to_string()],
            None,
            Some("poem.txt".to_string()),
            &mut EmptyPrinter::new())
        {
            Ok(_) => panic!("unexpected success"),
            Err(BuildError::WorkErrors(errors)) =>
            {
                assert_eq!(errors.len(), 1);
                match &errors[0]
                {
                    WorkError::TargetFileNotGenerated(path_str) => assert_eq!(path_str, "poem.txt"),
                    _ => panic!("Got work error but not the correct error: {}", errors[0]),
                }
            },
            Err(error) => panic!("Got error but not the correct error: {}", error),
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

        write_str_to_file(&mut system, "verse1.txt", "Roses are red.\n").unwrap();
        write_str_to_file(&mut system, "verse2.txt", "Violets are violet.\n").unwrap();
        write_str_to_file(&mut system, "test.rules", rules).unwrap();

        {
            let mut elements = directory::init(&mut system, "ruler-directory").unwrap();
            let target_history_before = elements.memory.take_target_history("poem.txt");
            assert_eq!(target_history_before, TargetHistory::empty());
            elements.memory.insert_target_history("poem.txt".to_string(), target_history_before);
        }

        build(
            system.clone(),
            "ruler-directory",
            vec!["test.rules".to_string()],
            None,
            Some("poem.txt".to_string()),
            &mut EmptyPrinter::new()).unwrap();

        assert_eq!(read_file_to_string(&mut system, "poem.txt").unwrap(),
            "Roses are red.\nViolets are violet.\n");

        {
            let mut elements = directory::init(&mut system, "ruler-directory").unwrap();
            let target_history = elements.memory.take_target_history("poem.txt");
            assert_eq!(target_history, TargetHistory::new(
                TicketFactory::from_str("Roses are red.\nViolets are violet.\n").result(), 17));
            elements.memory.insert_target_history("poem.txt".to_string(), target_history);
        }
    }

    #[test]
    fn build_first_does_not_cache()
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
        let mut system = FakeSystem::new(19);

        write_str_to_file(&mut system, "verse1.txt", "Roses are red.\n").unwrap();
        write_str_to_file(&mut system, "verse2.txt", "Violets are violet.\n").unwrap();
        write_str_to_file(&mut system, "test.rules", rules).unwrap();

        build(
            system.clone(),
            "ruler-directory",
            vec!["test.rules".to_string()],
            None,
            Some("poem.txt".to_string()),
            &mut EmptyPrinter::new()).unwrap();

        assert_eq!(
            read_file_to_string(&mut system, "poem.txt").unwrap(),
            "Roses are red.\nViolets are violet.\n");

        let elements = directory::init(&mut system, "ruler-directory").unwrap();
        match elements.cache.open(&TicketFactory::from_str("Roses are red.\nViolets are violet.\n").result())
        {
            Ok(_file) => panic!("Unexpected cache presence after first build"),
            Err(OpenError::NotThere) => {},
            Err(_) => panic!("Unexpected error trying to access cache after first build"),
        }
    }

}
