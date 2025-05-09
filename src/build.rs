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
};
use crate::sort::
{
    Node,
    NodePack,
    SourceIndex,
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
use crate::current::
{
    CurrentFileStatesError
};
use crate::printer::Printer;
use termcolor::
{
    Color,
};
use crate::system::
{
    System,
    SystemError,
    to_command_script
};
use crate::system::util::
{
    read_file_to_string,
    ReadFileToStringError,
};

/*  The topological sort step takes a vector of Rules and converts it to collection with more
    structure called a NodePack.  A NodePack has leaves corresponding to source files, nodes corresponding
    to rules, references between them, and sorted structure.  But a NodePack does not know about how _this_ module
    will dispatch the work of building onto threads, so the first step when receiving a NodePack is to
    process it and turn it into one of these ChannelPacks which has channel sender/receiver according to the
    dependencies in the NodePack. */
struct ChannelPack
{
    leaves: Vec<(String, Vec<Sender<Packet>>)>,
    nodes: Vec<(Node, Vec<(usize, Sender<Packet>)>, Vec<Receiver<Packet>>)>,
}

impl ChannelPack
{
    /*  Consumes a NodePack, returns the same leaves and nodes in a ChannelPack */
    fn new(node_pack : NodePack) -> Self
    {
        let mut leaves : Vec<(String, Vec<Sender<Packet>>)> =
            node_pack.leaves.into_iter().map(|leaf| {(leaf, vec![])}).collect();

        let mut nodes : Vec<(Node, Vec<(usize, Sender<Packet>)>, Vec<Receiver<Packet>>)> =
            node_pack.nodes.into_iter().map(|node| {(node, vec![], vec![])}).collect();

        for node_index in 0..nodes.len()
        {
            for source_indicies_index in 0..nodes[node_index].0.source_indices.len()
            {
                let (sender, receiver) : (Sender<Packet>, Receiver<Packet>) = mpsc::channel();
                match nodes[node_index].0.source_indices[source_indicies_index]
                {
                    SourceIndex::Leaf(i) => leaves[i].1.push(sender),
                    SourceIndex::Pair(i, sub_index) => nodes[i].1.push((sub_index, sender)),
                }

                nodes[node_index].2.push(receiver);
            }
        }

        ChannelPack
        {
            leaves: leaves,
            nodes: nodes,
        }
    }
}

#[derive(Debug)]
pub enum BuildError
{
    Canceled,
    ReceiverError(RecvError),
    SenderError(SendError<Packet>),
    FailedToReadCurrentFileStates(CurrentFileStatesError),
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

            BuildError::FailedToReadCurrentFileStates(error) =>
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

pub enum RunError
{
    BuildError(BuildError),
    ExecutionError(SystemError),
}

impl fmt::Display for RunError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            RunError::BuildError(build_error) =>
                write!(formatter, "{}", build_error),

            RunError::ExecutionError(system_error) =>
                write!(formatter, "Target built but failed to execute cleanly: {}", system_error),
        }
    }
}

fn read_all_rules_files_to_strings<SystemType : System>
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

/*  Open the rulefile(s), parse, and return the vector of Nodes. */
pub fn get_nodes
<
    SystemType : System,
>
(
    system : &SystemType,
    rulefile_paths : Vec<String>,
    goal_target_opt: Option<String>
)
-> Result<NodePack, BuildError>
{
    let all_rule_text = read_all_rules_files_to_strings(system, rulefile_paths)?;

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
                    Ok(pack) => pack,
                    Err(error) => return Err(BuildError::TopologicalSortFailed(error)),
                }
            },
            None =>
            {
                match topological_sort_all(rules)
                {
                    Ok(pack) => pack,
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
    let mut tickets = vec![];
    let mut canceled = false;

    /*  It is tempting to have this loop exit early if one source cancels, but
        that makes possible the following race:

        Suppose two sources A and B.  A cancels quickly, then this loop bails early,
        the thread exist, the receiving channel closes.  Later B tries to send a
        source ticket and fails with "sending on a closed channel" */
    for receiver in receiver_vec.iter()
    {
        match receiver.recv()
        {
            Ok(packet) =>
            {
                match packet.get_ticket()
                {
                    Ok(ticket) => tickets.push(ticket),
                    Err(PacketError::Cancel) => canceled = true,
                }
            },
            Err(error) => return Err(BuildError::ReceiverError(error)),
        }
    }

    if canceled
    {
        return Err(BuildError::Canceled);
    }

    let mut factory = TicketFactory::new();
    for ticket in tickets
    {
        factory.input_ticket(ticket);
    }
    Ok(factory.result())
}

pub struct BuildParams
{
    directory_path : String,
    rulefile_paths : Vec<String>,
    urlfile_path_opt : Option<String>,
    goal_target_opt: Option<String>,
}

impl BuildParams
{
    pub fn from_all(
        directory_path : String,
        rulefile_paths : Vec<String>,
        urlfile_path_opt : Option<String>,
        goal_target_opt : Option<String>,
    ) -> Self
    {
        BuildParams
        {
            directory_path : directory_path,
            rulefile_paths : rulefile_paths,
            urlfile_path_opt : urlfile_path_opt,
            goal_target_opt : goal_target_opt,
        }
    }
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
    printer : &mut PrinterType,
    params : BuildParams
)
-> Result<(), BuildError>
{
    let mut elements =
    match directory::init(&mut system, &params.directory_path)
    {
        Ok(elements) => elements,
        Err(error) =>
        {
            return match error
            {
                InitDirectoryError::FailedToReadCurrentFileStates(current_file_states_error) =>
                    Err(BuildError::FailedToReadCurrentFileStates(current_file_states_error)),
                _ => Err(BuildError::DirectoryMalfunction),
            }
        }
    };

    let download_urls =
    match params.urlfile_path_opt
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

    let mut channel_pack = ChannelPack::new(get_nodes(&system, params.rulefile_paths, params.goal_target_opt)?);
    let mut handles = Vec::new();

    for (leaf, sender_vec) in channel_pack.leaves.drain(..)
    {
        let blob = elements.current_file_states.take_blob(vec![leaf.clone()]);
        let system_clone = system.clone();
        handles.push(
            (
                None,
                thread::spawn(
                    move || -> Result<WorkResult, BuildError>
                    {
                        match handle_source_only_node(system_clone, blob)
                        {
                            Ok(result) =>
                            {
                                for sender in sender_vec
                                {
                                    match sender.send(Packet::from_ticket(result.file_state_vec.get_ticket(0)))
                                    {
                                        Ok(_) => {},
                                        Err(error) => return Err(BuildError::SenderError(error)),
                                    }
                                }
                                Ok(result)
                            },
                            Err(error) =>
                            {
                                for sender in sender_vec
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
            )
        )
    }

    for (mut node, sender_vec, receiver_vec) in channel_pack.nodes.drain(..)
    {
        let temp_targets = node.targets;
        node.targets = vec![];
        let blob = elements.current_file_states.take_blob(temp_targets);

        let mut downloader_cache_urls = Vec::new();
        let mut downloader_history_urls = Vec::new();

        for url in &download_urls.urls
        {
            downloader_cache_urls.push(format!("{}/files", url));
            downloader_history_urls.push(format!("{}/rules", url));
        }

        let downloader_cache = DownloaderCache::new(downloader_cache_urls);
        let downloader_history = DownloaderHistory::new(downloader_history_urls);
        let system_clone = system.clone();

        let rule_history = match elements.history.read_rule_history(&node.rule_ticket)
        {
            Ok(rule_history) => rule_history,
            Err(history_error) => return Err(BuildError::HistoryError(history_error)),
        };

        let cache_clone = elements.cache.clone();
        let downloader_cache_clone = downloader_cache.clone();
        let downloader_rule_history = downloader_history.get_rule_history(&node.rule_ticket);

        handles.push(
            (
                Some(node.rule_ticket.clone()),
                thread::spawn(
                    move || -> Result<WorkResult, BuildError>
                    {
                        let mut info = HandleNodeInfo::new(system_clone);
                        info.blob = blob;

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
                                    match sender.send(Packet::from_ticket(result.file_state_vec.get_ticket(sub_index)))
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
            )
        )
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
                    Ok(work_result) =>
                    {
                        match work_result.work_option
                        {
                            WorkOption::SourceOnly =>
                            {
                            },

                            WorkOption::Resolutions(resolutions) =>
                            {
                                for (i, path) in work_result.blob.get_paths().iter().enumerate()
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

                                    printer.print_single_banner_line(banner_text, banner_color, &path);
                                }
                            },

                            WorkOption::CommandExecuted(output) =>
                            {
                                for path in work_result.blob.get_paths().iter()
                                {
                                    printer.print_single_banner_line("     Built", Color::Magenta, &path);
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

                        elements.current_file_states.insert_blob(work_result.blob);
                    },
                    Err(BuildError::WorkError(work_error)) => work_errors.push(work_error),
                    Err(BuildError::Canceled) => {},
                    Err(error) => panic!("Unexpected build error: {}", error),
                }
            },
            Err(_error) => return Err(BuildError::Weird),
        }
    }

    match elements.current_file_states.to_file()
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

/*  Called when you type "ruler run".  Appeals to build() function to do the build.
    If there are no errors, executes the target file specified, passing it extra_args. */
pub fn run
<
    SystemType : System + 'static,
    PrinterType : Printer,
>
(
    mut system : SystemType,
    directory_path : &str,
    rulefile_paths : Vec<String>,
    urlfile_path_opt : Option<String>,
    executable : String,
    mut extra_args : Vec<String>,
    printer : &mut PrinterType
)
-> Result<(), RunError>
{
    match build(
        system.clone(),
        printer,
        BuildParams::from_all(
            directory_path.to_string(),
            rulefile_paths,
            urlfile_path_opt,
            Some(executable.clone()))
    )
    {
        Err(error) => return Err(RunError::BuildError(error)),
        Ok(()) => {},
    }

    let mut all = vec![format!("./{}", executable)];
    all.append(&mut extra_args);

    for result in system.execute_command(to_command_script(all))
    {
        match result
        {
            Ok(_command_line_output) => {},
            Err(system_error) => return Err(RunError::ExecutionError(system_error)),
        }
    }

    Ok(())
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
                InitDirectoryError::FailedToReadCurrentFileStates(current_file_states_error) =>
                    Err(BuildError::FailedToReadCurrentFileStates(current_file_states_error)),
                _ => Err(BuildError::DirectoryMalfunction),
            }
        }
    };

    let mut node_pack = get_nodes(&mut system, rulefile_paths, goal_target_opt)?;

    let mut handles = Vec::new();
    for node in node_pack.nodes.drain(..)
    {
        let blob = elements.current_file_states.take_blob(node.targets);
        let mut system_clone = system.clone();
        let mut local_cache_clone = elements.cache.clone();

        handles.push(
            thread::spawn(
                move || -> Result<(), WorkError>
                {
                    clean_targets(
                        blob,
                        &mut system_clone,
                        &mut local_cache_clone)
                }
            )
        );
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
        BuildParams,
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
        Blob,
        FileState
    };
    use std::io::Write;

    fn make_default_build_params() -> BuildParams
    {
        BuildParams
        {
            directory_path : ".ruler".to_string(),
            rulefile_paths : vec!["build.rules".to_string()],
            urlfile_path_opt : None,
            goal_target_opt : Some("poem.txt".to_string()),
        }
    }

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
        write_str_to_file(&mut system, "build.rules", rules).unwrap();



        build(
            system.clone(),
            &mut EmptyPrinter::new(),
            make_default_build_params()
        ).unwrap();

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
        write_str_to_file(&mut system, "build.rules", rules).unwrap();

        match build(
            system.clone(),
            &mut EmptyPrinter::new(),
            make_default_build_params())
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
        write_str_to_file(&mut system, "build.rules", rules).unwrap();

        

        match build(
            system.clone(),
            &mut EmptyPrinter::new(),
            make_default_build_params()
        )
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
        write_str_to_file(&mut system, "build.rules", rules).unwrap();



        match build(
            system.clone(),
            &mut EmptyPrinter::new(),
            make_default_build_params())
        {
            Ok(_) =>
            {
                assert_eq!(read_file_to_string(&mut system, "poem.txt").unwrap(),
                    "I looked over Jordan, and what did I see?\n");
            },
            Err(error) => panic!("Unexpected error: {}", error),
        }
    }

    /*  Rules for a poem with two verses and a refrain.  Try building the poem three times, once with each source file omitted.
        Check that the error matches the missing file. */
    #[test]
    fn build_poem_with_various_omitted_sources()
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

            write_str_to_file(&mut system, "build.rules", rules).unwrap();

            match build(
                system.clone(),
                &mut EmptyPrinter::new(),
                make_default_build_params())
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
        system.create_file("build.rules").unwrap().write_all(&[0x80u8]).unwrap();

        match build(
            system.clone(),
            &mut EmptyPrinter::new(),
            make_default_build_params())
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
        write_str_to_file(&mut system, "build.rules", rules).unwrap();

        build(
            system.clone(),
            &mut EmptyPrinter::new(),
            make_default_build_params()
        ).unwrap();

        system.time_passes(1);

        assert_eq!(read_file_to_string(&mut system, "poem.txt").unwrap(), "Roses are red.\nViolets are blue.\n");
        write_str_to_file(&mut system, "verse2.txt", "Violets are violet.\n").unwrap();
        write_str_to_file(&mut system, "poem.txt", "Wrong content forcing a rebuild").unwrap();

        match build(
            system.clone(),
            &mut EmptyPrinter::new(),
            make_default_build_params())
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
        write_str_to_file(&mut system, "build.rules", rules).unwrap();

        build(
            system.clone(),
            &mut EmptyPrinter::new(),
            make_default_build_params()
        ).unwrap();

        system.time_passes(1);

        assert_eq!(read_file_to_string(&mut system, "poem.txt").unwrap(),
            "Roses are red.\nViolets are blue.\n");

        let ticket = TicketFactory::from_file(&system, "poem.txt").unwrap().result();
        write_str_to_file(&mut system, "verse2.txt", "Violets are violet.\n").unwrap();

        build(
            system.clone(),
            &mut EmptyPrinter::new(),
            make_default_build_params()
        ).unwrap();

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
            &mut EmptyPrinter::new(),
            make_default_build_params()
        ).unwrap();

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
        write_str_to_file(&mut system, "build.rules", rules).unwrap();

        match build(
            system.clone(),
            &mut EmptyPrinter::new(),
            make_default_build_params()
        )
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

    /*  In a file system, create source files and rules file for a poem.
        Access the .ruler direcotry, and use the take() function to get the state of the poem.
        Verify that it is uninitialized.  Then run the build.  Verify that the build imparted
        the new FileState on the poem. */
    #[test]
    fn build_check_file_state()
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
        write_str_to_file(&mut system, "build.rules", rules).unwrap();

        let expected_poem_blob_before = Blob::from_paths(vec!["poem.txt".to_string()], |_path|{FileState::empty()});
        let expected_poem_blob_after = Blob::from_paths(vec!["poem.txt".to_string()], |_path|
            {FileState::new(
                TicketFactory::from_str("Roses are red.\nViolets are violet.\n").result(), 17)
            });

        {
            let mut elements = directory::init(&mut system, ".ruler").unwrap();
            assert_eq!(elements.current_file_states.take_blob(vec!["poem.txt".to_string()]), expected_poem_blob_before);
        }

        build(
            system.clone(),
            &mut EmptyPrinter::new(),
            make_default_build_params()
            ).unwrap();

        assert_eq!(read_file_to_string(&mut system, "poem.txt").unwrap(),
            "Roses are red.\nViolets are violet.\n");

        {
            let mut elements = directory::init(&mut system, ".ruler").unwrap();
            assert_eq!(elements.current_file_states.take_blob(vec!["poem.txt".to_string()]), expected_poem_blob_after);
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
        write_str_to_file(&mut system, "build.rules", rules).unwrap();

        build(
            system.clone(),
            &mut EmptyPrinter::new(),
            make_default_build_params()
        ).unwrap();

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
