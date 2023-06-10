use crate::ticket::Ticket;

#[derive(Debug)]
pub enum PacketError
{
    Cancel,
}

pub struct Packet
{
    ticket_result: Result<Ticket, PacketError>,
}

impl Packet
{
    pub fn from_ticket(ticket: Ticket) -> Packet
    {
        Packet
        {
            ticket_result: Ok(ticket),
        }
    }

    pub fn cancel() -> Packet
    {
        Packet
        {
            ticket_result: Err(PacketError::Cancel)
        }
    }

    pub fn get_ticket(self) -> Result<Ticket, PacketError>
    {
        self.ticket_result
    }
}
