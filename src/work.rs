use crate::packet::
{
    Packet,
    PacketError,
};
use crate::ticket::
{
    Ticket,
    TicketFactory,
};
use crate::system::
{
    CommandLineOutput,
    ReadWriteError,
    System,
    SystemError,
};
use crate::history::
{
    RuleHistory,
    DownloaderRuleHistory,
    RuleHistoryInsertError,
};
use crate::blob::
{
    TargetTickets,
    FileResolution,
    TargetFileInfo,
    ResolutionError,
    GetCurrentFileInfoError,
    TargetContentInfo,
    get_file_ticket,
    get_current_file_info,
    resolve_remembered_target_tickets,
    resolve_with_no_memory,
};
use crate::cache::
{
    SysCache,
    DownloaderCache,
};

use std::sync::mpsc::
{
    Receiver,
    RecvError,
    SendError,
};
use std::fmt;

pub enum WorkOption
{
    SourceOnly,
    Resolutions(Vec<FileResolution>),
    CommandExecuted(CommandLineOutput),
}

pub struct WorkResult
{
    pub target_tickets : Vec<Ticket>,
    pub target_infos : Vec<TargetFileInfo>,
    pub work_option : WorkOption,
    pub rule_history : Option<RuleHistory>,
}

#[derive(Debug)]
pub enum WorkError
{
    Canceled,
    ReceiverError(RecvError),
    SenderError(SendError<Packet>),
    TicketAlignmentError(ReadWriteError),
    FileNotFound(String),
    TargetFileNotGenerated(String),
    FileNotAvailableToCache(String, ReadWriteError),
    ReadWriteError(String, ReadWriteError),
    ResolutionError(ResolutionError),
    GetCurrentFileInfoError(GetCurrentFileInfoError),
    CommandExecutedButErrored,
    CommandFailedToExecute(SystemError),
    Contradiction(Vec<String>),
    Weird,
}

impl fmt::Display for WorkError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            WorkError::Canceled =>
                write!(formatter, "Canceled by a depdendence"),

            WorkError::ReceiverError(error) =>
                write!(formatter, "Failed to recieve anything from source: {}", error),

            WorkError::SenderError(error) =>
                write!(formatter, "Failed to send to dependent {}", error),

            WorkError::TicketAlignmentError(error) =>
                write!(formatter, "File IO error when attempting to get hash of sources: {}", error),

            WorkError::FileNotFound(path) =>
                write!(formatter, "File not found: {}", path),

            WorkError::TargetFileNotGenerated(path) =>
                write!(formatter, "Target file missing after running build command: {}", path),

            WorkError::FileNotAvailableToCache(path, error) =>
                write!(formatter, "File not available to be cached: {} : {}", path, error),

            WorkError::ReadWriteError(path, error) =>
                write!(formatter, "Error reading file: {}: {}", path, error),

            WorkError::ResolutionError(error) =>
                write!(formatter, "Error resolving rule: {}", error),

            WorkError::GetCurrentFileInfoError(error) =>
                write!(formatter, "Error getting ticket and timestamp: {}", error),

            WorkError::CommandExecutedButErrored =>
                write!(formatter, "Command executed but errored"),

            WorkError::CommandFailedToExecute(error) =>
                write!(formatter, "Failed to execute command: {}", error),

            WorkError::Contradiction(contradicting_target_paths) =>
            {
                let mut message = "The following targets failed to record into history because they contradict an existing target history:\n".to_string();
                for path in contradicting_target_paths
                {
                    message.push_str(path);
                    message.push_str("\n");
                }
                message.push_str("This likely means a real dependence is not reflected in the rule.\n");
                write!(formatter, "{}", message)
            },

            WorkError::Weird =>
                write!(formatter, "Weird! How did you do that!"),
        }
    }
}

fn get_current_target_tickets<SystemType: System>
(
    system : &SystemType,
    target_infos : &Vec<TargetFileInfo>,
)
-> Result<Vec<Ticket>, WorkError>
{
    let mut target_tickets = Vec::new();
    for target_info in target_infos.iter()
    {
        match get_file_ticket(system, target_info)
        {
            Ok(ticket_opt) =>
            {
                match ticket_opt
                {
                    Some(ticket) => target_tickets.push(ticket),
                    None => return Err(WorkError::FileNotFound(target_info.path.clone())),
                }
            },
            Err(error) => return Err(WorkError::ReadWriteError(target_info.path.clone(), error)),
        }
    }

    Ok(target_tickets)
}

fn handle_source_only_node<SystemType: System>
(
    system : SystemType,
    target_infos : Vec<TargetFileInfo>
)
->
Result<WorkResult, WorkError>
{
    let current_target_tickets = match get_current_target_tickets(
        &system,
        &target_infos)
    {
        Ok(target_tickets) => target_tickets,
        Err(WorkError::FileNotFound(path)) =>
        {
            return Err(WorkError::FileNotFound(path));
        },
        Err(error) => return Err(error),
    };

    Ok(
        WorkResult
        {
            target_tickets : current_target_tickets,
            target_infos : target_infos,
            work_option : WorkOption::SourceOnly,
            rule_history : None
        }
    )
}

/*  Takes a vector of receivers, and waits for them all to receive, so it can
    hash together all their results into one Ticket obejct.  Returns an error
    if the receivers error or if the packet produces an error when it tries to
    get the ticket from it. */
fn wait_for_sources_ticket
(
    receivers : Vec<Receiver<Packet>>
)
->
Result<Ticket, WorkError>
{
    let mut factory = TicketFactory::new();

    for receiver in receivers.iter()
    {
        match receiver.recv()
        {
            Ok(packet) => 
            {
                match packet.get_ticket()
                {
                    Ok(ticket) => factory.input_ticket(ticket),
                    Err(PacketError::Cancel) => return Err(WorkError::Canceled),
                }
            },
            Err(error) => return Err(WorkError::ReceiverError(error)),
        }
    }

    Ok(factory.result())
}

/*  Takes a vector of resolutions, and returns true if any of them are NeedsRebuild */
fn needs_rebuild(resolutions : &Vec<FileResolution>) -> bool
{
    for resolution in resolutions
    {
        match resolution
        {
            FileResolution::NeedsRebuild =>
            {
                return true
            },
            _ => {},
        }
    }

    false
}

/*  Handles the case where at least one target is irrecoverable and therefore the command
    needs to execute to rebuild the node.  When successful, returns a WorkResult with option
    indicating that the command executed (WorkResult contains the commandline result) */
fn rebuild_node<SystemType : System>
(
    system : &mut SystemType,
    mut rule_history : RuleHistory,
    sources_ticket : Ticket,
    command : Vec<String>,
    mut target_infos : Vec<TargetFileInfo>
)
->
Result<WorkResult, WorkError>
{
    let command_result =
    match system.execute_command(command)
    {
        Ok(command_result) => command_result,
        Err(error) =>
        {
            return Err(WorkError::CommandFailedToExecute(error));
        },
    };

    if ! command_result.success
    {
        return Err(WorkError::CommandExecutedButErrored);
    }

    let mut infos = vec![];
    for target_info in target_infos.iter_mut()
    {
        match get_current_file_info(system, &target_info)
        {
            Ok(current_info) =>
            {
                target_info.history = current_info.clone();
                infos.push(
                    TargetContentInfo
                    {
                        ticket : current_info.ticket,
                        executable : current_info.executable,
                    });
            },
            Err(GetCurrentFileInfoError::TargetFileNotFound(path, _system_error)) => return Err(WorkError::TargetFileNotGenerated(path)),
            Err(error) => return Err(WorkError::GetCurrentFileInfoError(error)),
        }
    }

    let target_tickets = infos.iter().map(|info| info.ticket.clone()).collect();

    match rule_history.insert(sources_ticket, TargetTickets::from_infos(infos))
    {
        Ok(_) => {},
        Err(error) =>
        {
            match error
            {
                RuleHistoryInsertError::Contradiction(contradicting_indices) =>
                {
                    let mut contradicting_target_paths = Vec::new();
                    for index in contradicting_indices
                    {
                        contradicting_target_paths.push(target_infos[index].path.clone())
                    }
                    return Err(WorkError::Contradiction(contradicting_target_paths));
                }

                RuleHistoryInsertError::TargetSizesDifferWeird =>
                    return Err(WorkError::Weird),
            }
        },
    }

    Ok(
        WorkResult
        {
            target_tickets : target_tickets,
            target_infos : target_infos,
            work_option : WorkOption::CommandExecuted(command_result),
            rule_history : Some(rule_history),
        }
    )
}

/*  Takes a vector of target_infos and attempts to resolve the targets using cache or download-urls.

    If there are remembered tickets, then this function appeals to resolve_single_target
    to try to retrieve a backup copy either from the cache or from the internet (backing up the current copy
    of each target as it goes)

    If there are no remembered tickets, then this function goes through each target, backs up the current version
    if it's there, and returns a vector full of NeedsRebuild
*/
fn resolve_with_cache<SystemType : System>
(
    system : &mut SystemType,
    cache : &mut SysCache<SystemType>,
    downloader_cache_opt : &Option<DownloaderCache>,
    rule_history : &RuleHistory,
    downloader_rule_history_opt : &Option<DownloaderRuleHistory>,
    sources_ticket : &Ticket,
    target_infos : &Vec<TargetFileInfo>,
)
->
Result<Vec<FileResolution>, WorkError>
{
    match rule_history.get_target_tickets(sources_ticket)
    {
        Some(remembered_target_tickets) =>
        {
            return match resolve_remembered_target_tickets(
                system, cache, downloader_cache_opt, target_infos, remembered_target_tickets)
            {
                Ok(file_resolution) => Ok(file_resolution),
                Err(resolution_error) => Err(WorkError::ResolutionError(resolution_error)),
            };
        },

        None => {},
    }

    match downloader_rule_history_opt
    {
        Some(downloader_rule_history) =>
        {
            match downloader_rule_history.get_target_tickets(sources_ticket)
            {
                Some(target_tickets) =>
                {
                    return match resolve_remembered_target_tickets(
                        system, cache, downloader_cache_opt, target_infos, &target_tickets)
                    {
                        Ok(file_resolution) => Ok(file_resolution),
                        Err(resolution_error) => Err(WorkError::ResolutionError(resolution_error)),
                    };
                },

                None => {},
            }
        },

        None => {},
    }

    match resolve_with_no_memory(system, cache, target_infos)
    {
        Ok(file_resolution) => Ok(file_resolution),
        Err(resolution_error) => Err(WorkError::ResolutionError(resolution_error)),
    }
}

pub struct RuleExt<SystemType: System>
{
    pub command : Vec<String>,
    pub rule_history : RuleHistory,
    pub cache : SysCache<SystemType>,
    pub downloader_cache_opt : Option<DownloaderCache>,
    pub downloader_rule_history_opt : Option<DownloaderRuleHistory>,
}

impl<SystemType: System> RuleExt<SystemType>
{
    #[cfg(test)]
    fn new(cache: SysCache<SystemType>) -> RuleExt<SystemType>
    {
        return RuleExt
        {
            cache : cache,
            command : Vec::new(),
            rule_history : RuleHistory::new(),
            downloader_cache_opt : None,
            downloader_rule_history_opt : None,
        };
    }
}

pub enum NodeType<SystemType: System>
{
    SourceOnly,
    Rule(RuleExt<SystemType>)
}

pub struct HandleNodeInfo<SystemType: System>
{
    pub system : SystemType,
    pub target_infos : Vec<TargetFileInfo>,
    pub receivers : Vec<Receiver<Packet>>,
    pub node_type : NodeType<SystemType>,
}

impl<SystemType: System> HandleNodeInfo<SystemType>
{
    pub fn new(system : SystemType) -> HandleNodeInfo<SystemType>
    {
        HandleNodeInfo
        {
            system : system,
            target_infos : Vec::new(),
            receivers : Vec::new(),
            node_type : NodeType::SourceOnly,
        }
    }
}

/*  This is a central, public function for handling a node in the depednece graph.
    It is meant to be called by a dedicated thread, and as such, it eats all its arguments.

    The RuleHistory gets modified when appropriate, and gets returned as part of the result.

    The possible parameters to this function are so many that they warrant a dedicated struct:
    HandleNodeInfo.
*/
pub fn handle_node<SystemType: System>
(
    mut info : HandleNodeInfo<SystemType>
)
->
Result<WorkResult, WorkError>
{
    let sources_ticket = wait_for_sources_ticket(info.receivers)?;

    /*  If there's a rule-history that means the node is a rule,
        otherwise, it is a plain source file. */
    match info.node_type
    {
        NodeType::SourceOnly =>
            handle_source_only_node(info.system, info.target_infos),

        NodeType::Rule(mut rule_ext) =>
        {
            match resolve_with_cache(
                &mut info.system,
                &mut rule_ext.cache,
                & rule_ext.downloader_cache_opt,
                & rule_ext.rule_history,
                & rule_ext.downloader_rule_history_opt,
                & sources_ticket,
                & info.target_infos)
            {
                Ok(resolutions) =>
                {
                    if needs_rebuild(&resolutions)
                    {
                        rebuild_node(
                            &mut info.system,
                            rule_ext.rule_history,
                            sources_ticket,
                            rule_ext.command,
                            info.target_infos)
                    }
                    else
                    {
                        let target_tickets = match get_current_target_tickets(
                            &info.system,
                            &info.target_infos)
                        {
                            Ok(target_tickets) => target_tickets,
                            Err(error) => return Err(error),
                        };

                        Ok(
                            WorkResult
                            {
                                target_tickets : target_tickets,
                                target_infos : info.target_infos,
                                work_option : WorkOption::Resolutions(resolutions),
                                rule_history : Some(rule_ext.rule_history),
                            }
                        )
                    }
                },

                Err(error) => Err(error),
            }
        },
    }
}

pub fn clean_targets<SystemType: System>
(
    target_infos : Vec<TargetFileInfo>,
    system : &mut SystemType,
    cache : &mut SysCache<SystemType>
)
-> Result<(), WorkError>
{
    for target_info in target_infos
    {
        if system.is_file(&target_info.path)
        {
            match get_file_ticket(system, &target_info)
            {
                Ok(Some(current_target_ticket)) =>
                {
                    {
                        match cache.back_up_file_with_ticket(
                            &current_target_ticket,
                            &target_info.path)
                        {
                            Ok(_) => {},
                            Err(error) =>
                                return Err(WorkError::FileNotAvailableToCache(
                                    target_info.path.clone(), error)),
                        }
                    }
                },
                Ok(None)=>
                {
                    match cache.back_up_file(
                        &target_info.path)
                    {
                        Ok(_) => {},
                        Err(error) =>
                            return Err(WorkError::FileNotAvailableToCache(
                                target_info.path.clone(), error)),
                    }
                },
                Err(error) => return Err(WorkError::TicketAlignmentError(error)),
            }
        }
    }

    Ok(())
}


#[cfg(test)]
mod test
{
    use crate::work::
    {
        FileResolution,
        WorkResult,
        WorkOption,
        WorkError,
        TargetFileInfo,
        HandleNodeInfo,
        RuleExt,
        NodeType,
        handle_node,
        wait_for_sources_ticket,
    };
    use crate::ticket::
    {
        TicketFactory,
        Ticket,
    };
    use crate::history::
    {
        RuleHistory,
    };
    use crate::blob::
    {
        TargetHistory,
        TargetTickets,
        ResolutionError,
        get_file_ticket
    };
    use crate::packet::Packet;
    use crate::cache::
    {
        SysCache,
    };
    use crate::system::util::
    {
        write_str_to_file,
        read_file_to_string,
    };
    use crate::system::
    {
        System,
        fake::FakeSystem,
    };

    use std::sync::mpsc::{self, Receiver};
    use std::thread::{self, JoinHandle};

    /*  For testing, it's useful to be able to check the ticket of a list of source files.
        So, this function creates a bunch of channels just for the purpose of sending source files
        through and getting a source ticket using wait_for_sources_ticket */
    fn current_sources_ticket
    <
        SystemType : System + 'static,
    >
    (
        system : &SystemType,
        paths : Vec<String>
    )
    -> Result<Ticket, WorkError>
    {
        let mut receivers = vec![];

        for path in paths
        {
            let (sender, receiver) = mpsc::channel();
            receivers.push(receiver);

            let system_clone = system.clone();
            match thread::spawn(
                move || -> Result<(), WorkError>
                {
                    match TicketFactory::from_file(&system_clone, &path)
                    {
                        Ok(mut factory) =>
                        {
                            match sender.send(Packet::from_ticket(factory.result()))
                            {
                                Ok(_) => Ok(()),
                                Err(error) => {
                                    println!("It's here 2");
                                    Err(WorkError::SenderError(error))
                                },
                            }
                        },
                        Err(error) => Err(WorkError::ReadWriteError(path.to_string(), error)),
                    }
                }
            ).join()
            {
                Ok(result) =>
                {
                    match result
                    {
                        Ok(_) => {},
                        Err(error) => return Err(error),
                    }
                },
                Err(_error) => return Err(WorkError::Weird),
            }
        }

        wait_for_sources_ticket(receivers)
    }

    fn to_info(mut targets : Vec<String>) -> Vec<TargetFileInfo>
    {
        let mut result = Vec::new();

        for target_path in targets.drain(..)
        {
            result.push(
                TargetFileInfo
                {
                    path : target_path,
                    history : TargetHistory::new(
                        TicketFactory::new().result(),
                        0,
                    ),
                }
            );
        }

        result
    }

    /*  Create a rule-history and populate it simulating a game having been built from a
        single C++ source file.  Get the file-ticket for the source, use that ticket to
        get a target ticket from the rule-history, and check it is what's expected. */
    #[test]
    fn work_get_tickets_from_history()
    {
        let mut rule_history = RuleHistory::new();
        let mut system = FakeSystem::new(10);

        let source_content = "int main(){printf(\"my game\"); return 0;}";
        let target_content = "machine code for my game";

        let mut source_factory = TicketFactory::new();
        source_factory.input_ticket(TicketFactory::from_str(source_content).result());

        // Make rule history remembering that the source c++ code built
        // to the target executable.
        match rule_history.insert(
            source_factory.result(),
            TargetTickets::from_vec(vec![TicketFactory::from_str(target_content).result()]))
        {
            Ok(_) => {},
            Err(_) => panic!("Rule history failed to insert"),
        }

        // Meanwhile, in the filesystem put some rubbish in game.cpp
        match write_str_to_file(&mut system, "game.cpp", source_content)
        {
            Ok(_) => {},
            Err(why) => panic!("Failed to make fake file: {}", why),
        }

        // Then get the file-ticket for the current source file:
        match get_file_ticket(
            &system,
            &TargetFileInfo
            {
                path : "game.cpp".to_string(),
                history : TargetHistory::new_with_ticket(TicketFactory::new().result()),
            })
        {
            Ok(ticket_opt) =>
            {
                match ticket_opt
                {
                    Some(ticket) =>
                    {
                        // Make sure it matches the content of the file that we wrote
                        assert_eq!(ticket, TicketFactory::from_str(source_content).result());

                        // Then create a source ticket for all (one) sources
                        let mut source_factory = TicketFactory::new();
                        source_factory.input_ticket(ticket);
                        let source_ticket = source_factory.result();

                        // Remember what the target tickets were when built with that source before:
                        let target_tickets =
                        match rule_history.get_target_tickets(&source_ticket)
                        {
                            Some(target_tickets) => target_tickets,
                            None => panic!("Tickets not in history as expected"),
                        };

                        // Check that the target tickets in the history match the ones for the target
                        assert_eq!(
                            *target_tickets,
                            TargetTickets::from_vec(vec![
                                TicketFactory::from_str(target_content).result()
                            ])
                        );
                    },
                    None => panic!("No ticket found where expected"),
                }
            }
            Err(err) => panic!("Could not get ticket: {}", err),
        }
    }

    /*  Call handle_node with minimal connections and the empty list as a command.
        Check that runs without a hitch. */
    #[test]
    fn do_empty_command()
    {
        let mut system = FakeSystem::new(10);
        match write_str_to_file(&mut system, "A", "A-content")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        let mut info = HandleNodeInfo::new(system.clone());
        info.target_infos = to_info(vec!["A".to_string()]);

        match handle_node(info)
        {
            Ok(result) =>
            {
                match result.work_option
                {
                    WorkOption::SourceOnly => {},
                    _ => panic!("Wrong kind of WorkOption in result when command empty"),
                }
            },
            Err(why) => panic!("Command failed: {}", why),
        }
    }

    #[test]
    fn wait_for_channels()
    {
        let (sender_a, receiver_a) = mpsc::channel();
        let (sender_b, receiver_b) = mpsc::channel();

        let mut system = FakeSystem::new(10);

        match system.create_dir(".ruler-cache")
        {
            Ok(_) => {},
            Err(_) => panic!("Failed to make cache directory"),
        }

        match write_str_to_file(&mut system, "A-source.txt", "")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match write_str_to_file(&mut system, "A.txt", "")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match sender_a.send(Packet::from_ticket(TicketFactory::from_str("apples").result()))
        {
            Ok(p) => assert_eq!(p, ()),
            Err(e) => panic!("Unexpected error sending: {}", e),
        }

        match sender_b.send(Packet::from_ticket(TicketFactory::from_str("bananas").result()))
        {
            Ok(p) => assert_eq!(p, ()),
            Err(e) => panic!("Unexpected error sending: {}", e),
        }

        let mut rule_ext = RuleExt::new(SysCache::new(system.clone(), ".ruler-cache"));
        rule_ext.command = vec!["mycat".to_string(), "A-source.txt".to_string(), "A.txt".to_string()];

        let mut info = HandleNodeInfo::new(system.clone());
        info.target_infos = to_info(vec!["A.txt".to_string()]);
        info.receivers.push(receiver_a);
        info.receivers.push(receiver_b);
        info.node_type = NodeType::Rule(rule_ext);

        match handle_node(info)
        {
            Ok(result) =>
            {
                match result.work_option
                {
                    WorkOption::CommandExecuted(output) =>
                    {
                        assert_eq!(output.out, "");
                        assert_eq!(output.err, "");
                        assert_eq!(output.code, Some(0));
                        assert_eq!(output.success, true);
                    },
                    _ => panic!("Wrong type of work option.  Command was supposed to execute."),
                }
            },
            Err(err) => panic!("Command failed: {}", err),
        }
    }


    #[test]
    fn command_errors()
    {
        let (sender_a, receiver_a) = mpsc::channel();
        let (sender_b, receiver_b) = mpsc::channel();

        let mut system = FakeSystem::new(10);

        write_str_to_file(&mut system, "verse1.txt", "Roses are red\n").unwrap();
        write_str_to_file(&mut system, "verse2.txt", "Violets are violet\n").unwrap();

        match sender_a.send(Packet::from_ticket(TicketFactory::from_str("Roses are red\n").result()))
        {
            Ok(p) => assert_eq!(p, ()),
            Err(e) => panic!("Unexpected error sending: {}", e),
        }

        match sender_b.send(Packet::from_ticket(TicketFactory::from_str("Violets are violet\n").result()))
        {
            Ok(p) => assert_eq!(p, ()),
            Err(e) => panic!("Unexpected error sending: {}", e),
        }

        let mut rule_ext = RuleExt::new(SysCache::new(system.clone(), ".ruler-cache"));
        rule_ext.command = vec!["error".to_string()];

        let mut info = HandleNodeInfo::new(system.clone());
        info.target_infos = to_info(vec!["poem.txt".to_string()]);
        info.receivers = vec![receiver_a, receiver_b];
        info.node_type = NodeType::Rule(rule_ext);

        match handle_node(info)
        {
            Ok(_) => panic!("Unexpected command success"),
            Err(WorkError::CommandExecutedButErrored) => {},
            Err(error) => panic!("Wrong kind of error when command errors: {}", error),
        }
    }


    #[test]
    fn command_fails_to_generate_target()
    {
        let (sender_a, receiver_a) = mpsc::channel();
        let (sender_b, receiver_b) = mpsc::channel();

        let mut system = FakeSystem::new(10);

        write_str_to_file(&mut system, "verse1.txt", "Roses are red\n").unwrap();
        write_str_to_file(&mut system, "verse2.txt", "Violets are violet\n").unwrap();
        sender_a.send(Packet::from_ticket(TicketFactory::from_str("Roses are red\n").result())).unwrap();
        sender_b.send(Packet::from_ticket(TicketFactory::from_str("Violets are violet\n").result())).unwrap();

        let mut rule_ext = RuleExt::new(SysCache::new(system.clone(), ".ruler-cache"));
        rule_ext.command = vec!["mycat".to_string(),"verse1.txt".to_string(),"verse2.txt".to_string(),"wrong.txt".to_string()];

        let mut info = HandleNodeInfo::new(system.clone());
        info.target_infos = to_info(vec!["poem.txt".to_string()]);
        info.receivers = vec![receiver_a, receiver_b];
        info.node_type = NodeType::Rule(rule_ext);

        match handle_node(info)
        {
            Ok(_) => panic!("Unexpected command success"),
            Err(WorkError::TargetFileNotGenerated(path)) =>
            {
                assert_eq!(path, "poem.txt");
            },
            Err(error) => panic!("Wrong kind of error when command errors: {}", error),
        }
    }

    #[test]
    fn poem_concatination()
    {
        let (sender_a, receiver_a) = mpsc::channel();
        let (sender_b, receiver_b) = mpsc::channel();

        let mut system = FakeSystem::new(10);

        match write_str_to_file(&mut system, "verse1.txt", "Roses are red\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match write_str_to_file(&mut system, "verse2.txt", "Violets are violet\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match sender_a.send(Packet::from_ticket(TicketFactory::from_str("Roses are red\n").result()))
        {
            Ok(p) => assert_eq!(p, ()),
            Err(e) => panic!("Unexpected error sending: {}", e),
        }

        match sender_b.send(Packet::from_ticket(TicketFactory::from_str("Violets are violet\n").result()))
        {
            Ok(p) => assert_eq!(p, ()),
            Err(e) => panic!("Unexpected error sending: {}", e),
        }

        let mut rule_ext = RuleExt::new(SysCache::new(system.clone(), ".ruler-cache"));
        rule_ext.command = vec!["mycat".to_string(),"verse1.txt".to_string(),"verse2.txt".to_string(),"poem.txt".to_string()];

        let mut info = HandleNodeInfo::new(system.clone());
        info.target_infos = to_info(vec!["poem.txt".to_string()]);
        info.receivers = vec![receiver_a, receiver_b];
        info.node_type = NodeType::Rule(rule_ext);

        match handle_node(info)
        {
            Ok(result) =>
            {
                match result.work_option
                {
                    WorkOption::CommandExecuted(output)=>
                    {
                        assert_eq!(output.out, "");
                        assert_eq!(output.err, "");
                        assert_eq!(output.code, Some(0));
                        assert_eq!(output.success, true);
                    },
                    _ => panic!("Comand was supposed to execute.  Output wasn't there."),
                }
            },
            Err(err) => panic!("Command failed: {}", err),
        }
    }


    /*  Create sourcefiles with the two lines in a two-line poem.  Fabricate a rule history
        that recalls the two lines concatinated as the result In the fake file system, make
        the source files and create poem with already correct content.  Then called handle_node.
        Check that handle_node behaves as if the poem is already correct.
    */
    #[test]
    fn poem_already_correct()
    {
        let (sender_a, receiver_a) = mpsc::channel();
        let (sender_b, receiver_b) = mpsc::channel();

        let mut rule_history = RuleHistory::new();

        let mut factory = TicketFactory::new();
        factory.input_ticket(TicketFactory::from_str("Roses are red\n").result());
        factory.input_ticket(TicketFactory::from_str("Violets are violet\n").result());
        let sources_ticket = factory.result();

        match rule_history.insert(
            sources_ticket,
            TargetTickets::from_vec(vec![
                TicketFactory::from_str("Roses are red\nViolets are violet\n").result()
            ])
        )
        {
            Ok(_) => {},
            Err(_) => panic!("Rule history failed to insert"),
        }

        let mut system = FakeSystem::new(10);

        match system.create_dir(".ruler-cache")
        {
            Ok(_) => {},
            Err(_) => panic!("Failed to make cache directory"),
        }

        match write_str_to_file(&mut system, "verse1.txt", "Roses are red\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match write_str_to_file(&mut system, "verse2.txt", "Violets are violet\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match write_str_to_file(&mut system, "poem.txt", "Roses are red\nViolets are violet\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match sender_a.send(Packet::from_ticket(TicketFactory::from_str("Roses are red\n").result()))
        {
            Ok(p) => assert_eq!(p, ()),
            Err(e) => panic!("Unexpected error sending: {}", e),
        }

        match sender_b.send(Packet::from_ticket(TicketFactory::from_str("Violets are violet\n").result()))
        {
            Ok(p) => assert_eq!(p, ()),
            Err(e) => panic!("Unexpected error sending: {}", e),
        }

        let mut rule_ext = RuleExt::new(SysCache::new(system.clone(), ".ruler-cache"));
        rule_ext.command = vec!["mycat".to_string(), "verse1.txt".to_string(), "verse2.txt".to_string(), "poem.txt".to_string()];
        rule_ext.rule_history = rule_history;

        let mut info = HandleNodeInfo::new(system.clone());
        info.target_infos = to_info(vec!["poem.txt".to_string()]);
        info.receivers = vec![receiver_a, receiver_b];
        info.node_type = NodeType::Rule(rule_ext);

        match handle_node(info)
        {
            Ok(result) =>
            {
                match result.work_option
                {
                    WorkOption::Resolutions(resolutions) =>
                    {
                        assert_eq!(resolutions.len(), 1);

                        match resolutions[0]
                        {
                            FileResolution::AlreadyCorrect => {},
                            _ => panic!("Expected poem to already be correct, was some other work option"),
                        }
                    },
                    _ => panic!("Expected poem to already be resolved, was some other work option"),
                }
            },
            Err(err) => panic!("Command failed: {}", err),
        }
    }


    /*  Create source files for a two-line poem.  Fabricate a hisotry which describes
        a different result when the two source files are compiled.

        Populate poem.txt with arbitrary content, so that it mush rebuild.  Send the
        tickets for the file-system contents of the source files through the channels.

        Check that handle_node produces the correct contradiction error.
    */
    #[test]
    fn poem_contradicts_history()
    {
        let (sender_a, receiver_a) = mpsc::channel();
        let (sender_b, receiver_b) = mpsc::channel();

        let mut factory = TicketFactory::new();
        factory.input_ticket(TicketFactory::from_str("Roses are red\n").result());
        factory.input_ticket(TicketFactory::from_str("Violets are violet\n").result());
        let source_ticket = factory.result();

        let mut rule_history = RuleHistory::new();
        match rule_history.insert(
            source_ticket,
            TargetTickets::from_vec(
                vec![TicketFactory::from_str("Roses are red\nViolets are blue\n").result()]
            ))
        {
            Ok(_) => {},
            Err(_) => panic!("Rule history failed to insert"),
        }

        let mut system = FakeSystem::new(10);

        match system.create_dir(".ruler-cache")
        {
            Ok(_) => {},
            Err(_) => panic!("Failed to create directory"),
        }

        match write_str_to_file(&mut system, "verse1.txt", "Roses are red\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match write_str_to_file(&mut system, "verse2.txt", "Violets are violet\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match write_str_to_file(&mut system, "poem.txt", "Arbitrary content")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match sender_a.send(Packet::from_ticket(TicketFactory::from_str("Roses are red\n").result()))
        {
            Ok(p) => assert_eq!(p, ()),
            Err(e) => panic!("Unexpected error sending: {}", e),
        }

        match sender_b.send(Packet::from_ticket(TicketFactory::from_str("Violets are violet\n").result()))
        {
            Ok(p) => assert_eq!(p, ()),
            Err(e) => panic!("Unexpected error sending: {}", e),
        }

        let mut rule_ext = RuleExt::new(SysCache::new(system.clone(), ".ruler-cache"));
        rule_ext.command = vec!["mycat".to_string(), "verse1.txt".to_string(), "verse2.txt".to_string(), "poem.txt".to_string()];
        rule_ext.rule_history = rule_history;

        let mut info = HandleNodeInfo::new(system.clone());
        info.target_infos = to_info(vec!["poem.txt".to_string()]);
        info.receivers = vec![receiver_a, receiver_b];
        info.node_type = NodeType::Rule(rule_ext);

        match handle_node(info)
        {
            Ok(_result) =>
            {
                panic!("Build was success when it should have been a contradition")
            },
            Err(error) =>
            {
                match error
                {
                    WorkError::Contradiction(paths) => 
                    {
                        assert_eq!(paths.len(), 1);
                    },
                    _ => panic!("Wrong error: {}", error),
                }
            }
        }
    }

    /*  Build a poem by concatinating two verses.  When the build succeeds (panic if it does not)
        check that the rule history has a new pair in it with the source-ticket and target ticket according
        to what was built. */
    #[test]
    fn poem_work_populates_rule_history()
    {
        let (sender_a, receiver_a) = mpsc::channel();
        let (sender_b, receiver_b) = mpsc::channel();

        let rule_history = RuleHistory::new();

        let mut system = FakeSystem::new(10);

        match system.create_dir(".ruler-cache")
        {
            Ok(_) => {},
            Err(_) => panic!("Failed to create directory"),
        }

        match write_str_to_file(&mut system, "verse1.txt", "Roses are red\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match write_str_to_file(&mut system, "verse2.txt", "Violets are violet\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match write_str_to_file(&mut system, "poem.txt", "Arbitrary content")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match sender_a.send(Packet::from_ticket(TicketFactory::from_str("Roses are red\n").result()))
        {
            Ok(p) => assert_eq!(p, ()),
            Err(e) => panic!("Unexpected error sending: {}", e),
        }

        match sender_b.send(Packet::from_ticket(TicketFactory::from_str("Violets are violet\n").result()))
        {
            Ok(p) => assert_eq!(p, ()),
            Err(e) => panic!("Unexpected error sending: {}", e),
        }

        let mut rule_ext = RuleExt::new(SysCache::new(system.clone(), ".ruler-cache"));
        rule_ext.command = vec!["mycat".to_string(), "verse1.txt".to_string(), "verse2.txt".to_string(), "poem.txt".to_string()];
        rule_ext.rule_history = rule_history;

        let mut info = HandleNodeInfo::new(system.clone());
        info.target_infos = to_info(vec!["poem.txt".to_string()]);
        info.receivers = vec![receiver_a, receiver_b];
        info.node_type = NodeType::Rule(rule_ext);

        match handle_node(info)
        {
            Ok(result) =>
            {
                match result.work_option
                {
                    WorkOption::CommandExecuted(_command_result) =>
                    {
                    },
                    _ => panic!("Wrong kind of WorkOption"),
                }

                let source_ticket = 
                match current_sources_ticket(
                    &system,
                    vec![
                        "verse1.txt".to_string(),
                        "verse2.txt".to_string(),
                    ])
                {
                    Ok(ticket) => ticket,
                    Err(error) => panic!("Expected ticket, got error: {}", error),
                };

                match result.rule_history
                {
                    Some(rule_history) => 
                    {
                        let target_tickets = rule_history.get_target_tickets(&source_ticket).unwrap();
                        assert_eq!(
                            *target_tickets,
                            TargetTickets::from_vec(vec![
                                TicketFactory::from_str("Roses are red\nViolets are violet\n").result()
                            ])
                        );
                    },
                    None => panic!("Expected RuleHistory, got none"),
                }

            },
            Err(error) =>  panic!("Unexpected error: {}", error),
        }
    }


    #[test]
    fn source_file_not_there()
    {
        let mut system = FakeSystem::new(10);

        match write_str_to_file(&mut system, "some-other-file.txt", "Arbitrary content\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        let mut info = HandleNodeInfo::new(system.clone());
        info.target_infos = to_info(vec!["verse1.txt".to_string()]);
        info.node_type = NodeType::SourceOnly;

        match handle_node(info)
        {
            Ok(_) =>
            {
                panic!("Expected failure when file not present")
            },
            Err(error) =>
            {
                match error
                {
                    WorkError::FileNotFound(path) => assert_eq!(path, "verse1.txt"),
                    _=> panic!("Wrong kind of error"),
                }
            },
        }
    }


    #[test]
    fn target_removed_by_command()
    {
        let mut system = FakeSystem::new(10);

        match write_str_to_file(&mut system, "verse1.txt", "Arbitrary content\n").unwrap();

        let mut rule_ext = RuleExt::new(SysCache::new(system.clone(), ".rule-cache"));
        rule_ext.command = vec!["rm".to_string(), "verse1.txt".to_string()];

        let mut info = HandleNodeInfo::new(system.clone());
        info.target_infos = to_info(vec!["verse1.txt".to_string()]);
        info.node_type = NodeType::Rule(rule_ext);

        match handle_node(info)
        {
            Ok(_) =>
            {
                panic!("Expected failure when file not present")
            },
            Err(error) =>
            {
                match error
                {
                    WorkError::ResolutionError(ResolutionError::FileNotAvailableToCache(path, _error)) => assert_eq!(path, "verse1.txt"),
                    _ => panic!("Wrong kind of error!  Incorrect error: {}", error),
                }
            },
        }
    }


    #[test]
    fn one_dependence_intermediate_already_present()
    {
    }

    #[test]
    fn two_targets_both_not_there()
    {
    }


    #[test]
    fn two_targets_one_already_present()
    {
    }


    #[test]
    fn one_target_already_correct_only()
    {
    }


    #[test]
    fn one_target_not_there_error_in_command()
    {
    }


    #[test]
    fn one_dependence_with_error()
    {
    }

    #[test]
    fn one_target_already_correct_according_to_timestamp()
    {
    }


    #[test]
    fn one_target_correct_hash_incorrect_timestamp()
    {
    }
}
