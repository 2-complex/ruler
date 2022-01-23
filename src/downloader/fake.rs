use crate::ticket::
{
    Ticket
};
use crate::downloader::
{
    Downloader
};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct FakeDownloader
{
    /*  key = source-ticket
        value = a target ticket for each target */
    source_to_targets : HashMap<Ticket, Vec<Ticket>>,
}

impl FakeDownloader
{
    pub fn new() -> Self
    {
        FakeDownloader
        {
            source_to_targets : HashMap::new()
        }
    }

    pub fn insert(
        &mut self,
        source_ticket: &Ticket,
        target_tickets: Vec<Ticket>)
    {
        self.source_to_targets.insert(source_ticket.clone(), target_tickets);
    }
}

impl Downloader for FakeDownloader
{
    fn get_target_tickets(&self, source_ticket: &Ticket) -> Option<Vec<Ticket>>
    {
        match self.source_to_targets.get(source_ticket)
        {
            Some(target_tickets) => Some(target_tickets.clone()),
            None => None,
        }
    }
}

#[cfg(test)]
mod test
{
    use crate::downloader::
    {
        Downloader,
        fake::FakeDownloader,
    };
    use crate::ticket::
    {
        TicketFactory
    };

    #[test]
    fn fake_downloader_empty()
    {
        let downloader = FakeDownloader::new();
        let source_ticket = TicketFactory::new().result();

        match downloader.get_target_tickets(&source_ticket)
        {
            Some(_) => panic!("Unexpected target tickets vector"),
            None => {},
        }
    }

    #[test]
    fn fake_downloader_with_a_target_ticket_vector()
    {
        let mut downloader = FakeDownloader::new();
        let source_ticket = TicketFactory::new().result();

        let target_tickets = vec![
            TicketFactory::from_str("apple").result(),
            TicketFactory::from_str("banana").result()
        ];

        downloader.insert(&source_ticket, target_tickets);

        match downloader.get_target_tickets(&source_ticket)
        {
            Some(target_tickets) =>
            {
                assert_eq!(
                    vec![
                        TicketFactory::from_str("apple").result(),
                        TicketFactory::from_str("banana").result()],
                    target_tickets
                );
            },
            None => panic!("No response from downloader when target tickets expected"),
        }
    }
}
