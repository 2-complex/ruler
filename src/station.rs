extern crate filesystem;
use crate::memory::RuleHistory;
use crate::ticket::{Ticket, TicketFactory};
use filesystem::FileSystem;

pub struct Station<FSType: FileSystem>
{
    file_system: FSType,
}

impl<FSType: FileSystem> Station<FSType>
{
    pub fn new(file_system : FSType, rule_history: RuleHistory) -> Station<FSType>
    {
        Station
        {
            file_system : file_system
        }
    }

    pub fn remember_target_tickets(&self, _source_ticket : &Ticket) -> Vec<Ticket>
    {
        vec![TicketFactory::new().result()]
    }

    pub fn get_target_ticket(&self, _target_path : &str) -> Ticket
    {
        TicketFactory::from_str("abc").result()
    }
}

#[cfg(test)]
mod test
{
    #[test]
    fn station_get_tickets()
    {
        
    }
}
