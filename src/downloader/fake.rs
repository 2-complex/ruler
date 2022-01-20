use crate::ticket::
{
    Ticket
};
use crate::downloader::
{
    Downloader
};

pub struct FakeDownloader
{
}

impl FakeDownloader
{
    pub fn new() -> Self
    {
        FakeDownloader
        {
        }
    }
}

impl Downloader for FakeDownloader
{
    fn get_target_tickets(&self, source_ticket: &Ticket) -> Option<Vec<Ticket>>
    {
        None
    }
}

#[cfg(test)]
mod test
{
}
