use crate::ticket::
{
    Ticket
};

pub mod fake;
pub mod real;

pub enum DownloaderError
{
    SourceTicketNotFound
}

/*  Downloader abstracts the rule-history and file network-based-cache.  An
    implementation can appeal to the real network, or it can fake it for
    testing. */
pub trait Downloader
{
    fn get_target_tickets(&self, source_ticket: &Ticket) -> Option<&Vec<Ticket>>;
}