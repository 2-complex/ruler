use crate::packet::Packet;
use crate::ticket::
{
    Ticket,
    TicketFactory
};
use crate::system::
{
    CommandLineOutput,
    ReadWriteError,
    System,
    SystemError,
};
use crate::system::util::get_timestamp;
use crate::memory::
{
    RuleHistory,
    TargetHistory,
    RuleHistoryInsertError
};
use crate::cache::{LocalCache, RestoreResult};

use std::sync::mpsc::{Sender, Receiver, RecvError};
use std::fmt;
use std::time::
{
    SystemTimeError
};

pub struct TargetFileInfo
{
    pub path : String,
    pub history : TargetHistory,
}

/*  Takes a System and a filepath as a string.

    If the file exists, returns a ticket.
    If the file does not exist, returns Ok, but with no Ticket inside
    If the file exists but does not open or some other error occurs when generating
    the ticket, returns an error. */
pub fn get_file_ticket_from_path<SystemType: System>
(
    system : &SystemType,
    path : &str
)
-> Result<Option<Ticket>, ReadWriteError>
{
    if system.is_file(&path) || system.is_dir(&path)
    {
        match TicketFactory::from_file(system, &path)
        {
            Ok(mut factory) => Ok(Some(factory.result())),
            Err(error) => Err(error),
        }
    }
    else
    {
        Ok(None)
    }
}

/*  Takes a system and a TargetFileInfo, and obtains a ticket for the file described.
    If the modified date of the file matches the one in TargetHistory exactly. */
pub fn get_file_ticket<SystemType: System>
(
    system : &SystemType,
    target_info : &TargetFileInfo
)
-> Result<Option<Ticket>, ReadWriteError>
{
    /*  The body of this match looks like it has unhandled errors.  What's happening is:
        if any error occurs with the timestamp optimization, we skip the optimization. */
    match system.get_modified(&target_info.path)
    {
        Ok(system_time) =>
        {
            match get_timestamp(system_time)
            {
                Ok(timestamp) =>
                {
                    if timestamp == target_info.history.timestamp
                    {
                        return Ok(Some(target_info.history.ticket.clone()))
                    }
                },
                Err(_) => {},
            }
        },
        Err(_) => {},
    }

    get_file_ticket_from_path(system, &target_info.path)
}

/*  Takes a system and a TargetFileInfo, and obtains a ticket for the file described,
    and also a timestamp.

    If the modified date of the file matches the one in TargetHistory exactly, it
    doesn't bother recomputing the ticket. */
pub fn get_file_ticket_and_timestamp<SystemType: System>
(
    system : &SystemType,
    target_info : &TargetFileInfo
)
-> Result<(Ticket, u64), WorkError>
{
    match system.get_modified(&target_info.path)
    {
        Ok(system_time) =>
        {
            match get_timestamp(system_time)
            {
                Ok(timestamp) =>
                {
                    if timestamp == target_info.history.timestamp
                    {
                        Ok((target_info.history.ticket.clone(), timestamp))
                    }
                    else
                    {
                        match TicketFactory::from_file(system, &target_info.path)
                        {
                            Ok(mut factory) => Ok((factory.result(), timestamp)),
                            Err(error) => Err(WorkError::ReadWriteError(target_info.path.clone(), error)),
                        }
                    }
                },
                Err(error) => Err(WorkError::SystemTimeError(target_info.path.clone(), error)),
            }
        },

        // Note: possibly there are other ways get_modified can fail than the file being absent.
        // Maybe this logic should change.
        Err(_error) => Err(WorkError::TargetFileNotGenerated(target_info.path.clone())),
    }
}

pub enum FileResolution
{
    AlreadyCorrect,
    Recovered,
    #[allow(dead_code)] Downloaded,
    NeedsRebuild,
}

pub enum WorkOption
{
    SourceOnly,
    Resolutions(Vec<FileResolution>),
    CommandExecuted(CommandLineOutput),
}

pub struct WorkResult
{
    pub target_infos : Vec<TargetFileInfo>,
    pub work_option : WorkOption,
    pub rule_history : Option<RuleHistory>,
}

pub enum WorkError
{
    ReceivedErrorFromSource(String),
    ReceiverError(RecvError),
    SenderError,
    TicketAlignmentError(ReadWriteError),
    FileNotFound(String),
    TargetFileNotGenerated(String),
    FileNotAvailableToCache(String, ReadWriteError),
    ReadWriteError(String, ReadWriteError),
    SystemTimeError(String, SystemTimeError),
    CommandExecutedButErrored,
    CommandFailedToExecute(SystemError),
    Contradiction(Vec<String>),
    CacheDirectoryMissing,
    CacheMalfunction(SystemError),
    CommandWithNoRuleHistory,
    Weird,
}

impl fmt::Display for WorkError
{
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result
    {
        match self
        {
            WorkError::ReceivedErrorFromSource(error) =>
                write!(formatter, "Received error from source: {}", error),

            WorkError::ReceiverError(error) =>
                write!(formatter, "Failed to recieve anything from source: {}", error),

            WorkError::SenderError =>
                write!(formatter, "Failed to send to dependent"),

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

            WorkError::SystemTimeError(path, error) =>
                write!(formatter, "Error when getting modified timestamp: {}: {}", path, error),

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

            WorkError::CacheDirectoryMissing =>
                write!(formatter, "Cache directory missing"),

            WorkError::CacheMalfunction(error) =>
                write!(formatter, "Cache file i/o failed: {}", error),

            WorkError::CommandWithNoRuleHistory =>
                write!(formatter, "Command provided but no rule history, that should be impossible"),

            WorkError::Weird =>
                write!(formatter, "Weird! How did you do that!"),
        }
    }
}

/*  Given a target-info and a remembered ticket for that target file, check the current
    ticket, and if it matches, return AlreadyCorrect.  If it doesn't match, back up the current
    file, and then attempt to restore the remembered file from cache, if the cache doesn't have it
    attempt to download.  If no recovery or download works, shrug and return NeedsRebuild */
fn resolve_single_target<SystemType : System>
(
    system : &mut SystemType,
    cache : &LocalCache,
    remembered_ticket : &Ticket,
    target_info : &TargetFileInfo
)
->
Result<FileResolution, WorkError>
{
    match get_file_ticket(system, target_info)
    {
        Ok(Some(current_target_ticket)) =>
        {
            if *remembered_ticket == current_target_ticket
            {
                return Ok(FileResolution::AlreadyCorrect);
            }

            match cache.back_up_file_with_ticket(
                system,
                &current_target_ticket,
                &target_info.path)
            {
                Ok(_) => {},
                Err(error) =>
                {
                    return Err(WorkError::FileNotAvailableToCache(
                        target_info.path.clone(), error));
                },
            }

            match cache.restore_file(
                system,
                &remembered_ticket,
                &target_info.path)
            {
                RestoreResult::Done =>
                    Ok(FileResolution::Recovered),

                RestoreResult::NotThere =>
                    Ok(FileResolution::NeedsRebuild),
                    // TODO: attempt a download here

                RestoreResult::CacheDirectoryMissing =>
                    Err(WorkError::CacheDirectoryMissing),

                RestoreResult::SystemError(error) =>
                    Err(WorkError::CacheMalfunction(error)),
            }
        },

        // None means the file is not there, in which case, we just try to restore/download, and then go home.
        Ok(None) =>
        {
            match cache.restore_file(
                system,
                &remembered_ticket,
                &target_info.path)
            {
                RestoreResult::Done =>
                    Ok(FileResolution::Recovered),

                RestoreResult::NotThere =>
                    Ok(FileResolution::NeedsRebuild),
                    // TODO: attempt a download here

                RestoreResult::CacheDirectoryMissing =>
                    Err(WorkError::CacheDirectoryMissing),

                RestoreResult::SystemError(error) =>
                    Err(WorkError::CacheMalfunction(error)),
            }
        },
        Err(error) =>
            Err(WorkError::TicketAlignmentError(error)),
    }
}

/*  Takes a vector of target_infos and attempts to resolve the targets using cache or download urls.

    If there are remembered tickets, then this function appeals to resolve_single_target
    to try to retrieve a backup copy either from the cache or from the internet (backing up the current copy
    of each target as it goes)

    If there are no remembered tickets, then this function goes through each target, backs up the current version
    if it's there, and returns a vector full of NeedsRebuild
*/
fn resolve_with_cache<SystemType : System>
(
    system : &mut SystemType,
    cache : &LocalCache,
    rule_history : &RuleHistory,
    sources_ticket : &Ticket,
    target_infos : &Vec<TargetFileInfo>,
)
->
Result<Vec<FileResolution>, WorkError>
{
    let mut resolutions = Vec::new();

    match rule_history.get_target_tickets(sources_ticket)
    {
        Some(remembered_target_tickets) =>
        {
            for (i, target_info) in target_infos.iter().enumerate()
            {
                let remembered_ticket =
                match remembered_target_tickets.get(i)
                {
                    Some(ticket) => ticket,
                    None => return Err(WorkError::Weird),
                };

                match resolve_single_target(
                    system,
                    cache,
                    remembered_ticket,
                    target_info)
                {
                    Ok(resolution) => resolutions.push(resolution),
                    Err(error) => return Err(error),
                }
            }
        },

        None =>
        {
            for target_info in target_infos.iter()
            {
                match get_file_ticket(system, target_info)
                {
                    Ok(Some(current_target_ticket)) =>
                    {
                        match cache.back_up_file_with_ticket(
                            system,
                            &current_target_ticket,
                            &target_info.path)
                        {
                            Ok(_) =>
                            {
                                // TODO: Maybe encode whether it was cached in the FileResoluton
                                resolutions.push(FileResolution::NeedsRebuild);
                            },
                            Err(error) =>
                            {
                                return Err(
                                    WorkError::FileNotAvailableToCache(
                                        target_info.path.clone(), error));
                            }
                        }
                    },

                    Ok(None) => resolutions.push(FileResolution::NeedsRebuild),

                    Err(error) =>
                        return Err(WorkError::TicketAlignmentError(error)),
                }
            }
        }
    }

    Ok(resolutions)
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

/*  A rule_history of None means that the node in question doesn't represent a rule.
    So, error if there is a command specified, and at the end return SourceOnly. */
fn handle_source_only_node<SystemType: System>
(
    target_infos : Vec<TargetFileInfo>,
    command : Vec<String>,
    system : SystemType,
    senders : Vec<(usize, Sender<Packet>)>
)
->
Result<WorkResult, WorkError>
{
    if command.len() != 0
    {
        return Err(WorkError::CommandWithNoRuleHistory)
    }

    let current_target_tickets = match get_current_target_tickets(
        &system,
        &target_infos)
    {
        Ok(target_tickets) => target_tickets,
        Err(error) => return Err(error),
    };

    for (sub_index, sender) in senders
    {
        match sender.send(Packet::from_ticket(
            current_target_tickets[sub_index].clone()))
        {
            Ok(_) => {},
            Err(_error) => return Err(WorkError::SenderError),
        }
    }

    Ok(
        WorkResult
        {
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
                    Err(error) => return Err(WorkError::ReceivedErrorFromSource(error)),
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
    senders : Vec<(usize, Sender<Packet>)>,
    mut target_infos : Vec<TargetFileInfo>
)
->
Result<WorkResult, WorkError>
{
    match system.execute_command(command)
    {
        Ok(command_result) =>
        {
            if ! command_result.success
            {
                return Err(WorkError::CommandExecutedButErrored);
            }

            let mut post_command_target_tickets = Vec::new();
            for target_info in target_infos.iter_mut()
            {
                match get_file_ticket_and_timestamp(system, &target_info)
                {
                    Ok((ticket, timestamp)) =>
                    {
                        target_info.history = TargetHistory::new(ticket.clone(), timestamp);
                        post_command_target_tickets.push(ticket);
                    },
                    Err(error) => return Err(error),
                }
            }

            for (sub_index, sender) in senders
            {
                match sender.send(Packet::from_ticket(
                    post_command_target_tickets[sub_index].clone()))
                {
                    Ok(_) => {},
                    Err(_error) =>
                    {
                        return Err(WorkError::SenderError);
                    },
                }
            }

            match rule_history.insert(sources_ticket, post_command_target_tickets)
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
                    target_infos : target_infos,
                    work_option : WorkOption::CommandExecuted(command_result),
                    rule_history : Some(rule_history),
                }
            )
        },
        Err(error) =>
        {
            return Err(WorkError::CommandFailedToExecute(error))
        },
    }
}

/*  This is a central, public function for handling a node in the depednece graph.
    It is meant to be called by a dedicated thread, and as such, it eats all its arguments.

    The RuleHistory gets modified when appropriate, and gets returned as part of the result. */
pub fn handle_node<SystemType: System>
(
    target_infos : Vec<TargetFileInfo>,
    command : Vec<String>,
    rule_history_opt : Option<RuleHistory>,
    mut system : SystemType,
    senders : Vec<(usize, Sender<Packet>)>,
    receivers : Vec<Receiver<Packet>>,
    cache : LocalCache
)
->
Result<WorkResult, WorkError>
{
    let sources_ticket = match wait_for_sources_ticket(receivers)
    {
        Ok(ticket) => ticket,
        Err(error) => return Err(error),
    };

    /*  If there's a rule-history that means the node is rule,
        otherwise, it is a plain source file. */
    match rule_history_opt
    {
        None => handle_source_only_node(
            target_infos,
            command,
            system,
            senders),

        Some(rule_history) =>
        {
            match resolve_with_cache(
                &mut system,
                &cache,
                &rule_history,
                &sources_ticket,
                &target_infos)
            {
                Ok(resolutions) =>
                {
                    if needs_rebuild(&resolutions)
                    {
                        rebuild_node(
                            &mut system,
                            rule_history,
                            sources_ticket,
                            command,
                            senders,
                            target_infos
                        )
                    }
                    else
                    {
                        let target_tickets = match get_current_target_tickets(
                            &system,
                            &target_infos)
                        {
                            Ok(target_tickets) => target_tickets,
                            Err(error) => return Err(error),
                        };

                        for (sub_index, sender) in senders
                        {
                            match sender.send(
                                Packet::from_ticket(target_tickets[sub_index].clone()))
                            {
                                Ok(_) => {},
                                Err(_error) => return Err(WorkError::SenderError),
                            }
                        }

                        Ok(
                            WorkResult
                            {
                                target_infos : target_infos,
                                work_option : WorkOption::Resolutions(resolutions),
                                rule_history : Some(rule_history),
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
    cache : &LocalCache
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
                            system,
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
                        system,
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
        handle_node,
        get_file_ticket_from_path,
        get_file_ticket,
        FileResolution,
        WorkResult,
        WorkOption,
        WorkError,
        TargetFileInfo,
        wait_for_sources_ticket,
    };
    use crate::ticket::
    {
        TicketFactory,
        Ticket,
    };
    use crate::memory::{RuleHistory, TargetHistory};
    use crate::packet::Packet;
    use crate::cache::LocalCache;
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

    use std::sync::mpsc::{self, Sender, Receiver};
    use std::thread::{self, JoinHandle};

    /*  For testing, it's useful to be able to check the ticket of a list of source files.
        So, this function creates a bunch of channels just for the purpose of sending source files
        through and getting a source ticket using wait_for_sources_ticket */
    fn current_sources_ticket
    <
        SystemType : System + Clone + Send + 'static,
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
                                Err(_error) => Err(WorkError::SenderError),
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

    /*  Use a fake system to create a file, and write a string to it.  Then use
        get_file_ticket_from_path to obtain a ticket for that file, and compare
        that against a ticket made directly from the string. */
    #[test]
    fn work_get_file_ticket_from_path()
    {
        let mut system = FakeSystem::new(10);

        match write_str_to_file(&mut system, "quine.sh", "cat $0")
        {
            Ok(_) => {},
            Err(why) => panic!("Failed to make fake file: {}", why),
        }

        match get_file_ticket_from_path(
            &system,
            "quine.sh")
        {
            Ok(ticket_opt) => match ticket_opt
            {
                Some(ticket) => assert_eq!(ticket, TicketFactory::from_str("cat $0").result()),
                None => panic!(format!("Could not get ticket")),
            }
            Err(err) => panic!(format!("Could not get ticket: {}", err)),
        }
    }

    /*  Use the system to create a file, and write a string to it.  Then use get_file_ticket
        to obtain a ticket for that file, and compare that against a ticket made directly
        from the string. */
    #[test]
    fn work_get_tickets_from_filesystem()
    {
        let mut system = FakeSystem::new(10);

        match write_str_to_file(&mut system, "quine.sh", "cat $0")
        {
            Ok(_) => {},
            Err(why) => panic!("Failed to make fake file: {}", why),
        }

        match get_file_ticket(
            &system,
            &TargetFileInfo
            {
                path : "quine.sh".to_string(),
                history : TargetHistory
                {
                    ticket : TicketFactory::new().result(),
                    timestamp : 0,
                }
            })
        {
            Ok(ticket_opt) => match ticket_opt
            {
                Some(ticket) => assert_eq!(ticket, TicketFactory::from_str("cat $0").result()),
                None => panic!(format!("Could not get ticket")),
            }
            Err(err) => panic!(format!("Could not get ticket: {}", err)),
        }
    }

    /*  Create a file and a TargetFileInfo for that file with matching timestamp.  Then fill the file
        with some other data.  Make sure that when we get_file_ticket, we get the one from the history
        instead of the one from the file. */
    #[test]
    fn work_test_timestamp_optimization()
    {
        // Set the clock to 11
        let mut system = FakeSystem::new(11);

        let content = "int main(){printf(\"my game\"); return 0;}";
        let content_ticket = TicketFactory::from_str(content).result();

        // Doctor a TargetFileInfo to indicate the game.cpp was written at time 11
        let target_file_info = TargetFileInfo
        {
            path : "game.cpp".to_string(),
            history : TargetHistory
            {
                ticket : content_ticket.clone(),
                timestamp : 11,
            }
        };

        // Meanwhile, in the filesystem put some incorrect rubbish in game.cpp
        match write_str_to_file(&mut system, "game.cpp", "some rubbish")
        {
            Ok(_) => {},
            Err(why) => panic!("Failed to make fake file: {}", why),
        }

        // Then get the ticket for the current target file, passing the TargetFileInfo
        // with timestamp 11.  Check that it gives the ticket for the C++ code.
        match get_file_ticket(
            &system,
            &target_file_info)
        {
            Ok(ticket_opt) =>
            {
                match ticket_opt
                {
                    Some(ticket) => assert_eq!(ticket, content_ticket),
                    None => panic!("Failed to generate ticket"),
                }
            },
            Err(_) => panic!("Unexpected error getting file ticket"),
        }
    }

    /*  Create a file and a TargetFileInfo for that file with not-matching timestamp.  Fill the file
        with new and improved code.  Make sure that when we get_file_ticket, we get the one from the
        file because the history doesn't match. */
    #[test]
    fn work_test_timestamp_mismatch()
    {
        // Set the clock to 11
        let mut system = FakeSystem::new(11);

        let previous_content = "int main(){printf(\"my game\"); return 0;}";
        let previous_ticket = TicketFactory::from_str(previous_content).result();

        let current_content = "int main(){printf(\"my better game\"); return 0;}";
        let current_ticket = TicketFactory::from_str(current_content).result();

        // Doctor a TargetFileInfo to indicate the game.cpp was written at time 9
        let target_file_info = TargetFileInfo
        {
            path : "game.cpp".to_string(),
            history : TargetHistory
            {
                ticket : previous_ticket.clone(),
                timestamp : 9,
            }
        };

        // Meanwhile, in the filesystem, put new and improved game.cpp
        match write_str_to_file(&mut system, "game.cpp", current_content)
        {
            Ok(_) => {},
            Err(why) => panic!("Failed to make fake file: {}", why),
        }

        // Then get the ticket for the current target file, passing the TargetFileInfo
        // with timestamp 11.  Check that it gives the ticket for the C++ code.
        match get_file_ticket(
            &system,
            &target_file_info)
        {
            Ok(ticket_opt) =>
            {
                match ticket_opt
                {
                    Some(ticket) => assert_eq!(ticket, current_ticket),
                    None => panic!("Failed to generate ticket"),
                }
            },
            Err(_) => panic!("Unexpected error getting file ticket"),
        }
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
            vec![TicketFactory::from_str(target_content).result()])
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
                history : TargetHistory
                {
                    ticket : TicketFactory::new().result(),
                    timestamp : 0,
                }
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
                            vec![
                                TicketFactory::from_str(target_content).result()
                            ],
                            *target_tickets
                        );
                    },
                    None => panic!("No ticket found where expected"),
                }
            }
            Err(err) => panic!(format!("Could not get ticket: {}", err)),
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

        match handle_node(
            to_info(vec!["A".to_string()]),
            vec![],
            None,
            system.clone(),
            Vec::new(),
            Vec::new(),
            LocalCache::new(".ruler-cache"))
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
        let (sender_c, receiver_c) = mpsc::channel();

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

        match handle_node(
            to_info(vec!["A.txt".to_string()]),
            vec!["mycat".to_string(), "A-source.txt".to_string(), "A.txt".to_string()],
            Some(RuleHistory::new()),
            system.clone(),
            vec![(0, sender_c)],
            vec![receiver_a, receiver_b],
            LocalCache::new(".ruler-cache"))
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

                match receiver_c.recv()
                {
                    Ok(packet) =>
                    {
                        match packet.get_ticket()
                        {
                            Ok(ticket) => assert_eq!(ticket, TicketFactory::new().result()),
                            Err(why) => panic!("Unexpected error doing command: {}", why),
                        }
                    }
                    Err(_) => panic!("Unexpected fail to receive"),
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
        let (sender_c, receiver_c) = mpsc::channel();

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

        match handle_node(
            to_info(vec!["poem.txt".to_string()]),
            vec!["error".to_string()],
            Some(RuleHistory::new()),
            system.clone(),
            vec![(0, sender_c)],
            vec![receiver_a, receiver_b],
            LocalCache::new(".ruler-cache"))
        {
            Ok(_) => panic!("Unexpected command success"),
            Err(WorkError::CommandExecutedButErrored) =>
            {
                match receiver_c.recv()
                {
                    Ok(_) => panic!("Unexpected successful receive"),
                    Err(_) => {}
                }
            },
            Err(error) => panic!("Wrong kind of error when command errors: {}", error),
        }
    }


    #[test]
    fn command_fails_to_generate_target()
    {
        let (sender_a, receiver_a) = mpsc::channel();
        let (sender_b, receiver_b) = mpsc::channel();
        let (sender_c, _receiver_c) = mpsc::channel();

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

        match handle_node(
            to_info(vec!["poem.txt".to_string()]),
            vec![
                "mycat".to_string(),
                "verse1.txt".to_string(),
                "verse2.txt".to_string(),
                "wrong.txt".to_string()
            ],
            Some(RuleHistory::new()),
            system.clone(),
            vec![(0, sender_c)],
            vec![receiver_a, receiver_b],
            LocalCache::new(".ruler-cache"))
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
        let (sender_c, receiver_c) = mpsc::channel();

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

        match handle_node(
            to_info(vec!["poem.txt".to_string()]),
            vec![
                "mycat".to_string(),
                "verse1.txt".to_string(),
                "verse2.txt".to_string(),
                "poem.txt".to_string()
            ],
            Some(RuleHistory::new()),
            system.clone(),
            vec![(0, sender_c)],
            vec![receiver_a, receiver_b],
            LocalCache::new(".ruler-cache"))
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

                match receiver_c.recv()
                {
                    Ok(packet) =>
                    {
                        match packet.get_ticket()
                        {
                            Ok(ticket) =>
                            {
                                assert_eq!(
                                    ticket,
                                    TicketFactory::from_str("Roses are red\nViolets are violet\n").result());
                            },
                            Err(_) => panic!("Failed to receive ticket"),
                        }
                    }
                    Err(_) => panic!("Unexpected fail to receive"),
                }
            },
            Err(err) => panic!("Command failed: {}", err),
        }
    }


    #[test]
    fn poem_already_correct()
    {
        let (sender_a, receiver_a) = mpsc::channel();
        let (sender_b, receiver_b) = mpsc::channel();
        let (sender_c, receiver_c) = mpsc::channel();

        let mut rule_history = RuleHistory::new();

        let mut factory = TicketFactory::new();
        factory.input_ticket(TicketFactory::from_str("Roses are red\n").result());
        factory.input_ticket(TicketFactory::from_str("Violets are violet\n").result());

        match rule_history.insert(
            factory.result(),
            vec![
                TicketFactory::from_str("Roses are red\nViolets are violet\n").result()
            ]
        )
        {
            Ok(_) => {},
            Err(_) => panic!("Rule history failed to insert"),
        }

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

        match handle_node(
            to_info(vec!["poem.txt".to_string()]),
            vec![
                "error".to_string(),
                "poem is already correct".to_string(),
                "this command should not run".to_string(),
                "the end".to_string()],
            Some(rule_history),
            system.clone(),
            vec![(0, sender_c)],
            vec![receiver_a, receiver_b],
            LocalCache::new(".ruler-cache"))
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
                            _ => panic!("Expected poem to already be correct, was some other work option {}"),
                        }
                    },
                    _ => panic!("Expected poem to already be resolved, was some other work option {}"),
                }

                match receiver_c.recv()
                {
                    Ok(packet) =>
                    {
                        match packet.get_ticket()
                        {
                            Ok(ticket) =>
                            {
                                assert_eq!(
                                    ticket,
                                    TicketFactory::from_str("Roses are red\nViolets are violet\n").result());
                            },
                            Err(message) =>
                            {
                                panic!("Failed to receive ticket: {}", message);
                            },
                        }
                    }
                    Err(_) => panic!("Unexpected fail to receive"),
                }
            },
            Err(err) => panic!("Command failed: {}", err),
        }
    }


    #[test]
    fn poem_contradicts_history()
    {
        let (sender_a, receiver_a) = mpsc::channel();
        let (sender_b, receiver_b) = mpsc::channel();
        let (sender_c, _receiver_c) = mpsc::channel();

        let mut rule_history = RuleHistory::new();

        let mut factory = TicketFactory::new();
        factory.input_ticket(TicketFactory::from_str("Roses are red\n").result());
        factory.input_ticket(TicketFactory::from_str("Violets are violet\n").result());

        match rule_history.insert(
            factory.result(),
            vec![
                TicketFactory::from_str("Roses are red\nViolets are blue\n").result()
            ]
        )
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

        match handle_node(
            to_info(vec!["poem.txt".to_string()]),
            vec![
                "mycat".to_string(),
                "verse1.txt".to_string(),
                "verse2.txt".to_string(),
                "poem.txt".to_string()
            ],
            Some(rule_history),
            system.clone(),
            vec![(0, sender_c)],
            vec![receiver_a, receiver_b],
            LocalCache::new(".ruler-cache"))
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
        let (sender_c, _receiver_c) = mpsc::channel();

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

        match handle_node(
            to_info(vec!["poem.txt".to_string()]),
            vec![
                "mycat".to_string(),
                "verse1.txt".to_string(),
                "verse2.txt".to_string(),
                "poem.txt".to_string()
            ],
            Some(rule_history),
            system.clone(),
            vec![(0, sender_c)],
            vec![receiver_a, receiver_b],
            LocalCache::new(".ruler-cache"))
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
                            vec![
                                TicketFactory::from_str("Roses are red\nViolets are violet\n").result()
                            ]
                        );
                    },
                    None => panic!("Expected RuleHistory, got none"),
                }

            },
            Err(error) =>  panic!("Unexpected error: {}", error),
        }
    }


    #[test]
    fn file_not_there()
    {
        let mut system = FakeSystem::new(10);

        match write_str_to_file(&mut system, "some-other-file.txt", "Arbitrary content\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match handle_node(
            to_info(vec!["verse1.txt".to_string()]),
            vec![],
            None,
            system.clone(),
            vec![],
            vec![],
            LocalCache::new(".ruler-cache"))
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

        match write_str_to_file(&mut system, "verse1.txt", "Arbitrary content\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match handle_node(
            to_info(vec!["verse1.txt".to_string()]),
            vec!["rm".to_string(), "verse1.txt".to_string()],
            Some(RuleHistory::new()),
            system.clone(),
            vec![],
            vec![],
            LocalCache::new(".rule-cache"))
        {
            Ok(_) =>
            {
                panic!("Expected failure when file not present")
            },
            Err(error) =>
            {
                match error
                {
                    WorkError::FileNotAvailableToCache(path, _error) => assert_eq!(path, "verse1.txt"),
                    _ => panic!("Wrong kind of error!  Incorrect error: {}", error),
                }
            },
        }
    }

    fn spawn_command<SystemType: System + Send + 'static>
    (
        target_infos : Vec<TargetFileInfo>,
        command : Vec<String>,
        rule_history_opt : Option<RuleHistory>,
        system : SystemType,
        senders : Vec<(usize, Sender<Packet>)>,
        receivers : Vec<Receiver<Packet>>,
    )
    -> JoinHandle<Result<WorkResult, WorkError>>
    {
        thread::spawn(
            move || -> Result<WorkResult, WorkError>
            {
                handle_node(
                    target_infos,
                    command,
                    rule_history_opt,
                    system,
                    senders,
                    receivers,
                    LocalCache::new(".ruler-cache"))
            }
        )
    }

    #[test]
    fn one_dependence_only()
    {
        let mut system = FakeSystem::new(10);

        match write_str_to_file(&mut system, "verse1.txt", "I wish I were a windowsill")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        let (sender, receiver) = mpsc::channel();

        let handle1 = spawn_command(
            to_info(vec!["stanza1.txt".to_string()]),
            vec![
                "mycat".to_string(),
                "verse1.txt".to_string(),
                "stanza1.txt".to_string()
            ],
            Some(RuleHistory::new()),
            system.clone(),
            vec![(0, sender)],
            vec![],
        );

        let handle2 = spawn_command(
            to_info(vec!["poem.txt".to_string()]),
            vec![
                "mycat".to_string(),
                "stanza1.txt".to_string(),
                "poem.txt".to_string()
            ],
            Some(RuleHistory::new()),
            system.clone(),
            vec![],
            vec![receiver],
        );

        match handle1.join()
        {
            Ok(result) =>
            {
                match result
                {
                    Ok(work_result) =>
                    {
                        assert_eq!(work_result.target_infos[0].path, "stanza1.txt");

                        match work_result.rule_history
                        {
                            Some(_) => {},
                            None => panic!("Thread left with rule-history, came back with nothing."),
                        }

                        match work_result.work_option
                        {
                            WorkOption::CommandExecuted(output) =>
                            {
                                assert_eq!(output.out, "");
                            },
                            _ => panic!("Thread was supposed to execute command, did something else, got wrong work-option."),
                        }

                        match read_file_to_string(&mut system, "stanza1.txt")
                        {
                            Ok(text) => assert_eq!(text, "I wish I were a windowsill"),
                            Err(error) => panic!("Failed to read stanza1.txt.  Error: {}", error),
                        }
                    },
                    Err(_) => panic!("Thread inside failed"),
                }
            },


            Err(_) => panic!("Thread mechanics failed"),
        }

        match handle2.join()
        {
            Ok(_) =>
            {
                match read_file_to_string(&mut system, "poem.txt")
                {
                    Ok(text) => assert_eq!(text, "I wish I were a windowsill"),
                    Err(_) => panic!("Failed to read poem"),
                }
            },
            Err(_) => panic!("Second thread failed"),
        }
    }

    #[test]
    fn one_dependence_intermediate_already_present()
    {
        let mut system = FakeSystem::new(10);

        match system.create_dir(".ruler-cache")
        {
            Ok(_) => {},
            Err(_) => panic!("Failed to create directory"),
        }

        match write_str_to_file(&mut system, "verse1.txt", "I wish I were a windowsill")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match write_str_to_file(&mut system, "stanza1.txt", "Some content")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        let (sender, receiver) = mpsc::channel();

        let handle1 = spawn_command(
            to_info(vec!["stanza1.txt".to_string()]),
            vec![
                "mycat".to_string(),
                "verse1.txt".to_string(),
                "stanza1.txt".to_string()
            ],
            Some(RuleHistory::new()),
            system.clone(),
            vec![(0, sender)],
            vec![],
        );

        let handle2 = spawn_command(
            to_info(vec!["poem.txt".to_string()]),
            vec![
                "mycat".to_string(),
                "stanza1.txt".to_string(),
                "poem.txt".to_string()
            ],
            Some(RuleHistory::new()),
            system.clone(),
            vec![],
            vec![receiver],
        );

        match handle1.join()
        {
            Ok(result) =>
            {
                match result
                {
                    Ok(work_result) =>
                    {
                        assert_eq!(work_result.target_infos[0].path, "stanza1.txt");

                        match work_result.rule_history
                        {
                            Some(_) => {},
                            None => panic!("Thread left with rule-history, came back with nothing."),
                        }

                        match work_result.work_option
                        {
                            WorkOption::CommandExecuted(output) =>
                            {
                                assert_eq!(output.out, "");
                            },
                            _ => panic!("Thread was supposed to execute command, did something else, got wrong work-option."),
                        }

                        match read_file_to_string(&mut system, "stanza1.txt")
                        {
                            Ok(text) => assert_eq!(text, "I wish I were a windowsill"),
                            Err(_) => panic!("Failed to read stanza1.txt"),
                        }
                    },
                    Err(error) => panic!("Thread inside failed: {}", error),
                }
            },


            Err(_) => panic!("Thread mechanics failed"),
        }

        match handle2.join()
        {
            Ok(_) =>
            {
                match read_file_to_string(&mut system, "poem.txt")
                {
                    Ok(text) => assert_eq!(text, "I wish I were a windowsill"),
                    Err(_) => panic!("Failed to read poem"),
                }
            },
            Err(_) => panic!("Second thread failed"),
        }
    }

    #[test]
    fn two_targets_both_not_there()
    {
        let mut system = FakeSystem::new(10);

        match write_str_to_file(&mut system, "verse1.txt", "I wish I were a windowsill")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        let (sender, _receiver) = mpsc::channel();

        let handle1 = spawn_command(
            to_info(vec!["stanza1.txt".to_string()]),
            vec![
                "mycat2".to_string(),
                "verse1.txt".to_string(),
                "stanza1.txt".to_string(),
                "stanza2.txt".to_string(),
            ],
            Some(RuleHistory::new()),
            system.clone(),
            vec![(0, sender)],
            vec![],
        );

        match handle1.join()
        {
            Ok(join_result) =>
            {
                match join_result
                {
                    Ok(work_result) =>
                    {
                        assert_eq!(work_result.target_infos[0].path, "stanza1.txt");

                        match work_result.rule_history
                        {
                            Some(_) => {},
                            None => panic!("Thread left with rule-history, came back with nothing."),
                        }

                        match work_result.work_option
                        {
                            WorkOption::CommandExecuted(output) =>
                            {
                                assert_eq!(output.out, "");
                            },
                            _ => panic!("Thread was supposed to execute command, did something else, got wrong work-option."),
                        }

                        match read_file_to_string(&mut system, "stanza1.txt")
                        {
                            Ok(text) => assert_eq!(text, "I wish I were a windowsill"),
                            Err(_) => panic!("Failed to read stanza1"),
                        }

                        match read_file_to_string(&mut system, "stanza2.txt")
                        {
                            Ok(text) => assert_eq!(text, "I wish I were a windowsill"),
                            Err(_) => panic!("Failed to read stanza2"),
                        }
                    },
                    Err(error) => panic!("Thread inside failed {}", error),
                }
            },

            Err(_) => panic!("Thread execution failed"),
        }
    }


    #[test]
    fn two_targets_one_already_present()
    {
        let mut system = FakeSystem::new(10);

        match system.create_dir(".ruler-cache")
        {
            Ok(_) => {},
            Err(_) => panic!("Failed to create directory"),
        }

        match write_str_to_file(&mut system, "verse1.txt", "I wish I were a windowsill")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match write_str_to_file(&mut system, "stanza1.txt", "Some content")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        let (sender, _receiver) = mpsc::channel();

        let handle1 = spawn_command(
            to_info(
                vec![
                    "stanza1.txt".to_string(),
                    "stanza2.txt".to_string()
                ]),
            vec![
                "mycat2".to_string(),
                "verse1.txt".to_string(),
                "stanza1.txt".to_string(),
                "stanza2.txt".to_string(),
            ],
            Some(RuleHistory::new()),
            system.clone(),
            vec![(0, sender)],
            vec![],
        );

        match handle1.join()
        {
            Ok(join_result) =>
            {
                match join_result
                {
                    Ok(work_result) =>
                    {
                        assert_eq!(work_result.target_infos[0].path, "stanza1.txt");
                        assert_eq!(work_result.target_infos[1].path, "stanza2.txt");

                        match work_result.rule_history
                        {
                            Some(_) => {},
                            None => panic!("Thread left with rule-history, came back with nothing."),
                        }

                        match work_result.work_option
                        {
                            WorkOption::CommandExecuted(output) =>
                            {
                                assert_eq!(output.out, "");
                            },
                            _ => panic!("Thread was supposed to execute command, did something else, got wrong work-option."),
                        }

                        match read_file_to_string(&mut system, "stanza1.txt")
                        {
                            Ok(text) => assert_eq!(text, "I wish I were a windowsill"),
                            Err(_) => panic!("Failed to read stanza1"),
                        }

                        match read_file_to_string(&mut system, "stanza2.txt")
                        {
                            Ok(text) => assert_eq!(text, "I wish I were a windowsill"),
                            Err(_) => panic!("Failed to read stanza2"),
                        }
                    },
                    Err(error) => panic!("Thread inside failed: {}", error),
                }
            },

            Err(_) => panic!("Thread execution failed"),
        }
    }


    #[test]
    fn one_target_already_correct_only()
    {
        let mut system = FakeSystem::new(10);
        let mut rule_history = RuleHistory::new();

        match write_str_to_file(&mut system, "verse1.txt", "I wish I were a windowsill")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match write_str_to_file(&mut system, "poem.txt", "I wish I were a windowsill")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        let mut factory = TicketFactory::new();
        factory.input_ticket(
            TicketFactory::from_str("I wish I were a windowsill").result()
        );

        match rule_history.insert(
            factory.result(),
            vec![TicketFactory::from_str("I wish I were a windowsill").result()])
        {
            Ok(_) => {},
            Err(_) => panic!("Rule history failed to insert"),
        }

        let (sender, receiver) = mpsc::channel();

        let handle1 = spawn_command(
            to_info(vec!["verse1.txt".to_string()]),
            vec![],
            None,
            system.clone(),
            vec![(0, sender)],
            vec![],
        );

        let handle2 = spawn_command(
            to_info(vec!["poem.txt".to_string()]),
            vec![
                "error".to_string(),
                "this file should".to_string(),
                "already be correct".to_string()
            ],
            Some(rule_history),
            system.clone(),
            vec![],
            vec![receiver],
        );

        match handle1.join()
        {
            Ok(_) =>
            {
                match read_file_to_string(&mut system, "verse1.txt")
                {
                    Ok(text) => assert_eq!(text, "I wish I were a windowsill"),
                    Err(_) => panic!("Failed to read verse1"),
                }
            },
            Err(_) => panic!("First thread failed"),
        }

        match handle2.join()
        {
            Ok(_) =>
            {
                match read_file_to_string(&mut system, "poem.txt")
                {
                    Ok(text) => assert_eq!(text, "I wish I were a windowsill"),
                    Err(_) => panic!("Failed to read poem"),
                }
            },
            Err(_) => panic!("Second thread failed"),
        }
    }


    #[test]
    fn one_target_not_there_error_in_command()
    {
        let mut system = FakeSystem::new(10);
        let mut rule_history2 = RuleHistory::new();

        match write_str_to_file(&mut system, "verse1.txt", "I wish I were a windowsill")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        let mut factory = TicketFactory::new();
        factory.input_ticket(
            TicketFactory::from_str("I wish I were a windowsill").result()
        );

        match rule_history2.insert(
            factory.result(),
            vec![TicketFactory::from_str("I wish I were a windowsill").result()])
        {
            Ok(_) => {},
            Err(_) => panic!("Rule history failed to insert"),
        }

        let (sender, receiver) = mpsc::channel();

        let handle1 = spawn_command(
            to_info(vec!["verse1.txt".to_string()]),
            vec![],
            None,
            system.clone(),
            vec![(0, sender)],
            vec![],
        );

        let handle2 = spawn_command(
            to_info(vec!["poem.txt".to_string()]),
            vec![
                "nonsense".to_string(),
                "should".to_string(),
                "error".to_string(),
            ],
            Some(rule_history2),
            system.clone(),
            vec![],
            vec![receiver],
        );

        match handle1.join()
        {
            Ok(_) =>
            {
                match read_file_to_string(&mut system, "verse1.txt")
                {
                    Ok(text) => assert_eq!(text, "I wish I were a windowsill"),
                    Err(_) => panic!("Failed to read verse1"),
                }
            },
            Err(_) => panic!("First thread failed"),
        }

        match handle2.join()
        {
            Ok(thread_result) =>
            {
                match thread_result
                {
                    Ok(_) => panic!("Second thread failed to error"),
                    Err(_) =>
                    {
                        assert!(!system.is_file("poem.txt") && !system.is_dir("poem.txt"));
                    },
                }
            }
            Err(_) => panic!("Second thread failed"),
        }
    }


    #[test]
    fn one_dependence_with_error()
    {
        let mut system = FakeSystem::new(10);

        match write_str_to_file(&mut system,  "some-other-file.txt", "Arbitrary content\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match write_str_to_file(&mut system,  "stanza1.txt", "Arbitrary content\n")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        let (sender, receiver) = mpsc::channel();

        let handle1 = spawn_command(
            to_info(vec!["stanza1.txt".to_string()]),
            vec![
                "mycat".to_string(),
                "verse1.txt".to_string(),
                "stanza1.txt".to_string()
            ],
            None,
            system.clone(),
            vec![(0, sender)],
            vec![],
        );

        let handle2 = spawn_command(
            to_info(vec!["poem.txt".to_string()]),
            vec![
                "mycat".to_string(),
                "stanza1.txt".to_string(),
                "poem.txt".to_string()
            ],
            None,
            system.clone(),
            vec![],
            vec![receiver],
        );

        match handle1.join()
        {
            Ok(thread_result) =>
            {
                match thread_result
                {
                    Ok(_) => panic!("First thread failed to error"),
                    Err(_) => {},
                }
            }
            Err(_) => panic!("First thread join failed"),
        }

        match handle2.join()
        {
            Ok(thread_result) =>
            {
                match thread_result
                {
                    Ok(_) => panic!("Second thread failed to error"),
                    Err(_) => {},
                }
            },
            Err(_) => panic!("Second thread join failed"),
        }
    }


    #[test]
    fn one_target_already_correct_according_to_timestamp()
    {
        let mut system = FakeSystem::new(10);
        let mut rule_history2 = RuleHistory::new();

        match write_str_to_file(&mut system,  "verse1.txt", "I wish I were a windowsill")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match write_str_to_file(&mut system,  "poem.txt", "Content actually wrong")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        system.time_passes(17);

        let mut factory = TicketFactory::new();
        factory.input_ticket(
            TicketFactory::from_str("I wish I were a windowsill").result()
        );

        match rule_history2.insert(
            factory.result(),
            vec![TicketFactory::from_str("I wish I were a windowsill").result()])
        {
            Ok(_) => {},
            Err(_) => panic!("Rule history failed to insert"),
        }

        let (sender, receiver) = mpsc::channel();

        let handle1 = spawn_command(
            to_info(vec!["verse1.txt".to_string()]),
            vec![],
            None,
            system.clone(),
            vec![(0, sender)],
            vec![],
        );

        let handle2 = spawn_command(
            vec![
                TargetFileInfo
                {
                    path : "poem.txt".to_string(),
                    history : TargetHistory::new(
                        TicketFactory::from_str("I wish I were a windowsill").result(),
                        17,
                    ),
                }
            ],
            vec![
                "error".to_string(),
                "this file should".to_string(),
                "already be correct".to_string()
            ],
            Some(rule_history2),
            system.clone(),
            vec![],
            vec![receiver],
        );

        match handle1.join()
        {
            Ok(_) =>
            {
                match read_file_to_string(&mut system, "verse1.txt")
                {
                    Ok(text) => assert_eq!(text, "I wish I were a windowsill"),
                    Err(_) => panic!("Failed to read verse1"),
                }
            },
            Err(_) => panic!("First thread failed"),
        }

        match handle2.join()
        {
            Ok(_) =>
            {
                match read_file_to_string(&mut system, "poem.txt")
                {
                    Ok(text) => assert_eq!(text, "Content actually wrong"),
                    Err(_) => panic!("Failed to read poem"),
                }
            },
            Err(_) => panic!("Second thread failed"),
        }
    }


    #[test]
    fn one_target_correct_hash_incorrect_timestamp()
    {
        let mut system = FakeSystem::new(10);

        match system.create_dir(".ruler-cache")
        {
            Ok(_) => {},
            Err(_) => panic!("Failed to create directory"),
        }

        let mut rule_history2 = RuleHistory::new();

        match write_str_to_file(&mut system,  "verse1.txt", "I wish I were a windowsill")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        match write_str_to_file(&mut system,  "poem.txt", "Content wrong at first")
        {
            Ok(_) => {},
            Err(_) => panic!("File write operation failed"),
        }

        system.time_passes(18);

        let mut factory = TicketFactory::new();
        factory.input_ticket(
            TicketFactory::from_str("I wish I were a windowsill").result()
        );

        match rule_history2.insert(
            factory.result(),
            vec![TicketFactory::from_str("I wish I were a windowsill").result()])
        {
            Ok(_) => {},
            Err(_) => panic!("Rule history failed to insert"),
        }

        let (sender, receiver) = mpsc::channel();

        let handle1 = spawn_command(
            to_info(vec!["verse1.txt".to_string()]),
            vec![],
            None,
            system.clone(),
            vec![(0, sender)],
            vec![],
        );

        let handle2 = spawn_command(
            vec![
                TargetFileInfo
                {
                    path : "poem.txt".to_string(),
                    history : TargetHistory::new(
                        TicketFactory::from_str("I wish I were a windowsill").result(),
                        17,
                    ),
                }
            ],
            vec![
                "mycat".to_string(),
                "verse1.txt".to_string(),
                "poem.txt".to_string()
            ],
            Some(rule_history2),
            system.clone(),
            vec![],
            vec![receiver],
        );

        match handle1.join()
        {
            Ok(_) =>
            {
                match read_file_to_string(&mut system, "verse1.txt")
                {
                    Ok(text) => assert_eq!(text, "I wish I were a windowsill"),
                    Err(_) => panic!("Failed to read verse1"),
                }
            },
            Err(_) => panic!("First thread failed"),
        }

        match handle2.join()
        {
            Ok(_) =>
            {
                match read_file_to_string(&mut system, "poem.txt")
                {
                    Ok(text) => assert_eq!(text, "I wish I were a windowsill"),
                    Err(_) => panic!("Failed to read verse1"),
                }
            },
            Err(_) => panic!("Second thread failed"),
        }
    }
}
