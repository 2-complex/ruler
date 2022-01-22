use crate::ticket::
{
    Ticket
};
use crate::downloader::
{
    Downloader
};

#[derive(Debug, Clone)]
pub struct RealDownloader
{
}

impl RealDownloader
{
    pub fn new() -> Self
    {
        RealDownloader
        {
        }
    }
}

impl Downloader for RealDownloader
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
