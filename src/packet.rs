use crate::ticket::Ticket;

pub struct Packet
{
    ticket_opt: Option<Ticket>,
    error_message: String,
}

impl Packet
{
    pub fn from_ticket(ticket: Ticket) -> Packet
    {
        Packet
        {
            ticket_opt: Some(ticket),
            error_message: "".to_string(),
        }
    }

    pub fn from_error(message: String) -> Packet
    {
        Packet
        {
            ticket_opt: None,
            error_message: message,
        }
    }

    pub fn get_ticket(self) -> Result<Ticket, String>
    {
        match self.ticket_opt
        {
            Some(ticket) => Ok(ticket),
            None => Err(self.error_message),
        }
    }
}
