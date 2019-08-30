extern crate clap;
extern crate sqlite;
extern crate multimap;
extern crate filesystem;

use clap::{Arg, App};

use std::thread::{self, JoinHandle};

use filesystem::{FileSystem, OsFileSystem};
use std::sync::mpsc::{Sender, Receiver};
use std::sync::mpsc;
use multimap::MultiMap;

mod file;
mod rule;
mod ticket;
mod work;
mod memory;
mod station;
mod executor;
mod packet;

use self::rule::Record;
use self::packet::Packet;
use self::executor::CommandLineOutput;
use self::work::{OsExecutor, OsMetadataGetter, do_command};
use self::station::Station;
use self::memory::Memory;

fn spawn_command<FSType: FileSystem + Send + 'static>(
    record: Record,
    senders : Vec<(usize, Sender<Packet>)>,
    receivers : Vec<Receiver<Packet>>,
    station : Station<FSType> )
    -> JoinHandle<Result<CommandLineOutput, String>>
{
    thread::spawn(
        move || -> Result<CommandLineOutput, String>
        {
            do_command(
                record,
                senders,
                receivers,
                station,
                OsExecutor::new(),
                OsMetadataGetter::new())
        }
    )
}

fn make_multimaps(records : &Vec<Record>)
    -> (
        MultiMap<usize, (usize, Sender<Packet>)>,
        MultiMap<usize, (Receiver<Packet>)>
    )
{
    let mut senders : MultiMap<usize, (usize, Sender<Packet>)> = MultiMap::new();
    let mut receivers : MultiMap<usize, (Receiver<Packet>)> = MultiMap::new();

    for (target_index, record) in records.iter().enumerate()
    {
        for (source_index, sub_index) in record.source_indices.iter()
        {
            let (sender, receiver) : (Sender<Packet>, Receiver<Packet>) = mpsc::channel();
            senders.insert(*source_index, (*sub_index, sender));
            receivers.insert(target_index, receiver);
        }
    }

    (senders, receivers)
}

fn main()
{
    let matches = App::new("build")
        .version("0.1.0")
        .author("Peterson Trethewey <ptrethewey@roblox.com>")
        .about("You know when you have files that depend on other files?  This is for that situation.")
        .arg(Arg::with_name("RULEFILE")
             .required(true)
             .takes_value(true)
             .index(1)
             .help("path to build rules file"))
        .arg(Arg::from_usage("-t --target=[TARGET] 'Sets which target to build'"))
        .arg(Arg::from_usage("-t --memory=[MEMORY] 'Where to read/write cached file content data'"))
        .get_matches();

    let rulefile = matches.value_of("RULEFILE").unwrap();
    let memoryfile = matches.value_of("MEMORY").unwrap();
    let mut os_file_system = OsFileSystem::new();

    match matches.value_of("target")
    {
        Some(target) =>
        {
            println!("Reading rulefile: {}", rulefile);
            println!("Reading memory file: {}", memoryfile);
            println!("Building target: {}", target);

            match Memory::from_file(&mut os_file_system, memoryfile)
            {
                Err(why) => eprintln!("ERROR: {}", why),
                Ok(mut memory) =>
                {
                    match file::read(&rulefile.to_string())
                    {
                        Err(why) => eprintln!("ERROR: {}", why),
                        Ok(content) =>
                        {
                            match rule::parse(content)
                            {
                                Err(why) => eprintln!("ERROR: {}", why),
                                Ok(rules) =>
                                {
                                    let os_file_system = OsFileSystem::new();
                                    match rule::topological_sort(rules, &target)
                                    {
                                        Err(why) => eprintln!("ERROR: {}", why),
                                        Ok(mut records) =>
                                        {
                                            let (mut senders, mut receivers) = make_multimaps(&records);

                                            let mut handles = Vec::new();
                                            let mut index : usize = 0;

                                            for record in records.drain(..)
                                            {
                                                let sender_vec = match senders.remove(&index)
                                                {
                                                    Some(v) => v,
                                                    None => vec![],
                                                };

                                                let receiver_vec = match receivers.remove(&index)
                                                {
                                                    Some(v) => v,
                                                    None => vec![],
                                                };

                                                let station = Station::new(
                                                    os_file_system.clone(),
                                                    memory.get_rule_history(&record.ticket)
                                                );

                                                handles.push(
                                                    spawn_command(
                                                        record,
                                                        sender_vec,
                                                        receiver_vec,
                                                        station
                                                    )
                                                );

                                                index+=1;
                                            }

                                            for handle in handles
                                            {
                                                match handle.join()
                                                {
                                                    Err(_) =>
                                                    {
                                                        eprintln!("ERROR");
                                                    },
                                                    Ok(command_result) =>
                                                    {
                                                        match command_result
                                                        {
                                                            Ok(r) =>
                                                            {
                                                                println!("success: {}", r.success);
                                                                println!("code: {}", match r.code
                                                                {
                                                                    Some(code) => format!("{}", code),
                                                                    None => "None".to_string(),
                                                                });

                                                                println!("output: {}", r.out);
                                                                println!("error: {}", r.err);
                                                            },
                                                            Err(why) =>
                                                            {
                                                                eprintln!("ERROR {}", why);
                                                            },
                                                        }
                                                    }
                                                }
                                            }
                                        },
                                    }
                                }
                            }
                        },
                    }

                }
            }

        },
        None =>
        {
            eprintln!("ERROR no target to build");
        },
    };
}
