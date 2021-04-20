use std::net::
{
    UdpSocket,
    Ipv4Addr,
    SocketAddrV4
};
use serde::
{
    Serialize,
    Deserialize
};
use crate::system::
{
    System,
    SystemError,
};
use crate::printer::Printer;
use std::io::Read;
use std::io::Write;
use std::time::Duration;

#[derive(Serialize, Deserialize, PartialEq, Debug)]
enum ReceivePacket
{
    FileData(String, usize, Vec<u8>),
    FileIndexCount(String, usize),
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
enum SendPacket
{
    Want(String),
}

pub enum NetworkError
{
    SocketBindFailed,
    SendFailed,
}

pub fn serve
<
    SystemType : System + Clone + Send + 'static,
    PrinterType : Printer,
>
(
    mut system : SystemType,
    directory : &str,
    rulefile_paths : Vec<String>,
    printer : &mut PrinterType,
)
->
Result<(), NetworkError>
{
    {
        let socket =
        match UdpSocket::bind("127.0.0.1:34254")
        {
            Ok(socket) =>
            {
                socket
            },
            Err(_) =>
            {
                return Err(NetworkError::SocketBindFailed);
            }
        };

        /*  Receives a single datagram message on the socket. If `buf` is too small to hold
            the message, it will be cut off. */
        let mut buf = [0; 256];

        loop
        {
            printer.print("About to receive a packet");

            match socket.recv_from(&mut buf)
            {
                Ok((amt, src)) =>
                {
                    let decoded_packet : SendPacket = bincode::deserialize(&buf[..amt]).unwrap();

                    match decoded_packet
                    {
                        SendPacket::Want(path_str) =>
                        {
                            printer.print(&format!("Want: {}", path_str));
                            match system.open(&path_str)
                            {
                                Ok(mut reader) =>
                                {
                                    printer.print("about to send the file back");
                                    let mut in_buffer = [0u8; 256];
                                    let mut i = 0usize;
                                    loop
                                    {
                                        match reader.read(&mut in_buffer)
                                        {
                                            Ok(0) =>
                                            {
                                                break;
                                            }
                                            Ok(size) =>
                                            {
                                                let packet = ReceivePacket::FileData(path_str.clone(), i, in_buffer[..size].to_vec());
                                                i+=1;
                                                let encoded = bincode::serialize(&packet).unwrap();
                                                match socket.send_to(&encoded, &src)
                                                {
                                                    Ok(_) => {},
                                                    Err(_) => return Err(NetworkError::SendFailed),
                                                }
                                            },
                                            Err(error) =>
                                            {
                                                printer.print(&format!("file io error: {}", error));
                                            },
                                        }
                                    }

                                    let packet = ReceivePacket::FileIndexCount(path_str.clone(), i);
                                    let encoded = bincode::serialize(&packet).unwrap();
                                    match socket.send_to(&encoded, &src)
                                    {
                                        Ok(_) => {},
                                        Err(_) => return Err(NetworkError::SendFailed),
                                    }
                                },
                                Err(error) =>
                                {
                                    printer.print(&format!("File read error: {}", error));
                                }
                            }
                        },
                    }
                },

                Err(_) =>
                {
                    printer.print(&format!("breaking"));
                    break;
                }
            }
        }
    }
    Ok(())
}

pub fn download()
{
    println!("download\n");
}
