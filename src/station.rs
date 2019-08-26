extern crate filesystem;

use crate::memory::RuleHistory;
use crate::ticket::{Ticket, TicketFactory};
use crate::executor::CommandResult;

use filesystem::FileSystem;

pub struct Station<FSType: FileSystem>
{
    rule_history: RuleHistory,
    file_system: FSType,
}

impl<FSType: FileSystem> Station<FSType>
{
    pub fn new(
        file_system : FSType,
        rule_history: RuleHistory)
        -> Station<FSType>
    {
        Station
        {
            rule_history: rule_history,
            file_system : file_system,
        }
    }

    pub fn remember_target_tickets(&self, source_ticket : &Ticket) -> &[Ticket]
    {
        match self.rule_history.get(source_ticket)
        {
            Some(tickets) => tickets,
            None => &[],
        }
    }

    pub fn get_file_ticket(&self, target_path : &str) -> Result<Ticket, std::io::Error>
    {
        match TicketFactory::from_file(&self.file_system, target_path)
        {
            Ok(mut factory) => Ok(factory.result()),
            Err(err) => Err(err),
        }
    }
}

#[cfg(test)]
mod test
{
    use filesystem::{FileSystem, FakeFileSystem};
    use crate::memory::RuleHistory;
    use crate::station::Station;
    use crate::ticket::TicketFactory;

    #[test]
    fn station_get_tickets_from_filesystem()
    {
        let rule_history = RuleHistory::new();
        let file_system = FakeFileSystem::new();

        match file_system.write_file("quine.sh", "cat $0")
        {
            Ok(_) => {},
            Err(why) => panic!("Failed to make fake file: {}", why),
        }

        let station = Station::new(file_system, rule_history);

        match station.get_file_ticket("quine.sh")
        {
            Ok(ticket) => assert_eq!(ticket, TicketFactory::from_str("cat $0").result()),
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
        rule_history.insert(
            source_factory.result(),
            vec![TicketFactory::from_str(target_content).result()]
        );

        // Meanwhile, in the filesystem put some rubbish in game.cpp
        match file_system.write_file("game.cpp", source_content)
        {
            Ok(_) => {},
            Err(why) => panic!("Failed to make fake file: {}", why),
        }

        let station = Station::new(file_system, rule_history);

        // Then ask the station to get the ticket for the current source file:

        match station.get_file_ticket("game.cpp")
        {
            Ok(ticket) =>
            {
                // Make sure it's what we think it should be
                assert_eq!(ticket, TicketFactory::from_str(source_content).result());

                // Then create a source ticket for all (one) sources
                let mut source_factory = TicketFactory::new();
                source_factory.input_ticket(ticket);
                let source_ticket = source_factory.result();

                // Then ask the station to remember what the target
                // tickets were when built with that source before:
                let target_tickets = station.remember_target_tickets(&source_ticket);

                assert_eq!(
                    vec![
                        TicketFactory::from_str(target_content).result()
                    ],
                    target_tickets
                );
            }
            Err(err) => panic!(format!("Could not get ticket: {}", err)),
        }
    }
}
