extern crate filesystem;
use crate::memory::RuleHistory;
use crate::ticket::{Ticket, TicketFactory};
use filesystem::FileSystem;

pub struct Station<FSType: FileSystem>
{
    rule_history: RuleHistory,
    file_system: FSType,
}

impl<FSType: FileSystem> Station<FSType>
{
    pub fn new(file_system : FSType, rule_history: RuleHistory) -> Station<FSType>
    {
        Station
        {
            rule_history: rule_history,
            file_system : file_system,
        }
    }

    pub fn remember_target_tickets(&self, _source_ticket : &Ticket) -> Vec<Ticket>
    {
        vec![TicketFactory::new().result()]
    }

    pub fn get_target_ticket(&self, target_path : &str) -> Result<Ticket, std::io::Error>
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
    fn station_get_tickets()
    {
        let rule_history = RuleHistory::new();
        let mut file_system = FakeFileSystem::new();

        match file_system.write_file("quine.sh", "cat $0")
        {
            Ok(_) => {},
            Err(why) => panic!("Failed to make fake file: {}", why),
        }

        let station = Station::new(file_system, rule_history);

        match station.get_target_ticket("quine.sh")
        {
            Ok(ticket) => assert_eq!(ticket, TicketFactory::from_str("cat $0").result()),
            Err(err) => panic!(format!("Could not get ticket: {}", err)),
        }
    }
}
