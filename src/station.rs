extern crate filesystem;

use crate::memory::{RuleHistory, TargetHistory};
use crate::ticket::{Ticket, TicketFactory};
use crate::metadata::MetadataGetter;

use filesystem::FileSystem;
use std::time::{SystemTime, SystemTimeError};

pub struct TargetFileInfo
{
    pub path : String,
    pub history : TargetHistory,
}

pub struct Station<FileSystemType : FileSystem, MetadataGetterType : MetadataGetter>
{
    pub target_infos : Vec<TargetFileInfo>,
    pub command : Vec<String>,
    pub rule_history : Option<RuleHistory>,
    pub file_system : FileSystemType,
    pub metadata_getter : MetadataGetterType,
}

fn get_timestamp(system_time : SystemTime) -> Result<u64, SystemTimeError>
{
    match system_time.duration_since(SystemTime::UNIX_EPOCH)
    {
        Ok(duration) => Ok(1_000_000u64 * duration.as_secs() + u64::from(duration.subsec_micros())),
        Err(e) => Err(e),
    }
}

impl<
    FileSystemType: FileSystem,
    MetadataGetterType: MetadataGetter
>
Station<
    FileSystemType,
    MetadataGetterType
>
{
    pub fn new(
        target_infos : Vec<TargetFileInfo>,
        command : Vec<String>,
        rule_history: Option<RuleHistory>,
        file_system : FileSystemType,
        metadata_getter: MetadataGetterType,
        ) -> Station<FileSystemType, MetadataGetterType>
    {
        Station
        {
            target_infos : target_infos,
            command : command,
            rule_history: rule_history,
            file_system : file_system,
            metadata_getter : metadata_getter,
        }
    }
}

pub fn get_file_ticket<
    FileSystemType: FileSystem,
    MetadataGetterType: MetadataGetter>
(
    file_system : &FileSystemType,
    metadata_getter : &MetadataGetterType,
    target_info : &TargetFileInfo
)
-> Result<Option<Ticket>, std::io::Error>
{
    match metadata_getter.get_modified(&target_info.path)
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

    if file_system.is_file(&target_info.path) || file_system.is_dir(&target_info.path)
    {
        match TicketFactory::from_file(file_system, &target_info.path)
        {
            Ok(mut factory) => Ok(Some(factory.result())),
            Err(err) => Err(err),
        }
    }
    else
    {
        Ok(None)
    }
}


#[cfg(test)]
mod test
{
    use filesystem::{FileSystem, FakeFileSystem};
    use crate::memory::{RuleHistory, TargetHistory};
    use crate::station::{Station, TargetFileInfo, get_file_ticket};
    use crate::ticket::TicketFactory;
    use crate::metadata::FakeMetadataGetter;

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

    #[test]
    fn station_get_tickets_from_filesystem()
    {
        let file_system = FakeFileSystem::new();

        match file_system.write_file("quine.sh", "cat $0")
        {
            Ok(_) => {},
            Err(why) => panic!("Failed to make fake file: {}", why),
        }

        match get_file_ticket(
            &file_system,
            &FakeMetadataGetter::new(),
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

    #[test]
    fn station_get_tickets_from_history()
    {
        let mut rule_history = RuleHistory::new();
        let file_system = FakeFileSystem::new();

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
        match file_system.write_file("game.cpp", source_content)
        {
            Ok(_) => {},
            Err(why) => panic!("Failed to make fake file: {}", why),
        }

        let station = Station::new(
            to_info(vec!["A".to_string()]),
            vec!["noop".to_string()],
            Some(rule_history),
            file_system.clone(),
            FakeMetadataGetter::new());

        // Then ask the station to get the ticket for the current source file:

        match get_file_ticket(
            &file_system,
            &FakeMetadataGetter::new(),
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

                        // Then ask the station to remember what the target
                        // tickets were when built with that source before:
                        let target_tickets =
                        match &station.rule_history
                        {
                            Some(rule_history) => rule_history.remember_target_tickets(&source_ticket),
                            None => panic!("History does not exist"),
                        };

                        assert_eq!(
                            vec![
                                TicketFactory::from_str(target_content).result()
                            ],
                            target_tickets
                        );
                    },
                    None => panic!("No ticket found where expected"),
                }
            }
            Err(err) => panic!(format!("Could not get ticket: {}", err)),
        }
    }
}
